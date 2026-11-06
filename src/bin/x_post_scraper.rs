//! Scrape Donald Trump's X / Twitter posts into the same 18-column schema as
//! the master CSV, so the output can be appended alongside the Twitter-era rows.
//!
//! Fetches via ScrapeCreators' Twitter endpoint. NOTE: this endpoint returns
//! the most-recent window of tweets and exposes no pagination cursor, so it
//! captures recent X activity (which for Trump is sparse — a few posts/month).
//!
//! Setup (keys in .env): FIRST_API_KEY, SECOND_API_KEY, THRID_API_KEY, FOUTH_API_KEY
//!
//! Run:
//!     cargo run --release --bin x_post_scraper -- --since 2026-01-01 --out csv/x_2026.csv

use std::env;

use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use serde_json::Value;

const ENDPOINT: &str = "https://api.scrapecreators.com/v1/twitter/user-tweets";

const COLUMNS: &[&str] = &[
  "id",
  "date",
  "platform",
  "handle",
  "text",
  "favorite_count",
  "repost_count",
  "quote_flag",
  "repost_flag",
  "deleted_flag",
  "word_count",
  "hashtags",
  "urls",
  "user_mentions",
  "media_count",
  "media_urls",
  "post_url",
  "in_reply_to",
];

struct Config {
  since: DateTime<Utc>,
  out: String,
  handle: String,
}

fn parse_args() -> Result<Config> {
  let mut since = "2026-01-01".to_string();
  let mut out = "csv/x_2026.csv".to_string();
  let mut handle = "realDonaldTrump".to_string();

  let args: Vec<String> = env::args().skip(1).collect();
  let mut i = 0;
  while i < args.len() {
    match args[i].as_str() {
      "--since" => since = take(&args, &mut i)?,
      "--out" => out = take(&args, &mut i)?,
      "--handle" => handle = take(&args, &mut i)?,
      other => return Err(anyhow!("unknown argument: {other}")),
    }
    i += 1;
  }

  let date = NaiveDate::parse_from_str(&since, "%Y-%m-%d")
    .with_context(|| format!("--since must be YYYY-MM-DD, got {since}"))?;
  let since = Utc.from_utc_datetime(&date.and_hms_opt(0, 0, 0).unwrap());
  Ok(Config { since, out, handle })
}

fn take(args: &[String], i: &mut usize) -> Result<String> {
  *i += 1;
  args
    .get(*i)
    .cloned()
    .ok_or_else(|| anyhow!("missing value for {}", args[*i - 1]))
}

fn load_keys() -> Result<Vec<String>> {
  let keys: Vec<String> = [
    "FIRST_API_KEY",
    "SECOND_API_KEY",
    "THRID_API_KEY",
    "FOUTH_API_KEY",
  ]
  .iter()
  .filter_map(|name| env::var(name).ok())
  .map(|v| v.trim().to_string())
  .filter(|v| !v.is_empty())
  .collect();
  if keys.is_empty() {
    return Err(anyhow!("no API keys found; set FIRST_API_KEY (etc.) in .env"));
  }
  Ok(keys)
}

/// Parse Twitter's "Sat Sep 12 03:13:01 +0000 2026" timestamp.
fn parse_created_at(raw: &str) -> Result<DateTime<Utc>> {
  Ok(
    DateTime::parse_from_str(raw, "%a %b %d %H:%M:%S %z %Y")?
      .with_timezone(&Utc),
  )
}

fn join_field(arr: &Value, key: &str) -> String {
  arr
    .as_array()
    .map(|items| {
      items
        .iter()
        .filter_map(|it| it[key].as_str())
        .collect::<Vec<_>>()
        .join(",")
    })
    .unwrap_or_default()
}

fn num(v: &Value) -> String {
  v.as_i64()
    .map(|n| n.to_string())
    .unwrap_or_else(|| "0".to_string())
}

fn pybool(b: bool) -> String {
  if b {
    "True".to_string()
  } else {
    "False".to_string()
  }
}

/// Map one X tweet (ScrapeCreators shape) to the 18-field CSV row.
fn to_row(tweet: &Value, handle: &str) -> Result<Vec<String>> {
  let lg = &tweet["legacy"];
  let text = lg["full_text"].as_str().unwrap_or("").to_string();
  let word_count = text.split_whitespace().count();

  let entities = &lg["entities"];
  let hashtags = join_field(&entities["hashtags"], "text");
  let user_mentions = join_field(&entities["user_mentions"], "screen_name");
  let urls = join_field(&entities["urls"], "expanded_url");

  // Media lives under extended_entities (falls back to entities.media).
  let media = lg["extended_entities"]["media"]
    .as_array()
    .or_else(|| entities["media"].as_array())
    .cloned()
    .unwrap_or_default();
  let media_urls = media
    .iter()
    .filter_map(|m| m["media_url_https"].as_str())
    .collect::<Vec<_>>()
    .join(",");

  let date = parse_created_at(lg["created_at"].as_str().unwrap_or_default())?
    .format("%Y-%m-%d %H:%M:%S%:z")
    .to_string();

  let id = lg["id_str"]
    .as_str()
    .or_else(|| tweet["rest_id"].as_str())
    .unwrap_or("");
  let post_url = tweet["url"]
    .as_str()
    .map(String::from)
    .unwrap_or_else(|| format!("https://x.com/{handle}/status/{id}"));
  let in_reply_to = lg["in_reply_to_status_id_str"].as_str().unwrap_or("");
  let is_retweet = text.starts_with("RT @") || !lg["retweeted_status_result"].is_null();

  Ok(vec![
    id.to_string(),
    date,
    "Twitter".to_string(),
    handle.to_string(),
    text,
    num(&lg["favorite_count"]),
    num(&lg["retweet_count"]),
    pybool(lg["is_quote_status"].as_bool().unwrap_or(false)),
    pybool(is_retweet),
    pybool(false),
    word_count.to_string(),
    hashtags,
    urls,
    user_mentions,
    media.len().to_string(),
    media_urls,
    post_url,
    in_reply_to.to_string(),
  ])
}

#[tokio::main]
async fn main() -> Result<()> {
  let cfg = parse_args()?;
  let keys = load_keys()?;
  eprintln!("Loaded {} API key(s).", keys.len());

  let client = reqwest::Client::builder().build()?;
  eprintln!("Fetching @{} X posts since {}…", cfg.handle, cfg.since);

  let mut wtr = csv::Writer::from_path(&cfg.out)?;
  wtr.write_record(COLUMNS)?;

  let mut key_idx = 0usize;
  let body: Value = loop {
    let resp = client
      .get(ENDPOINT)
      .header("x-api-key", keys[key_idx].as_str())
      .query(&[("handle", cfg.handle.as_str())])
      .send()
      .await?;
    let code = resp.status();
    let body: Value = resp.json().await.unwrap_or(Value::Null);
    if code.is_success() && body["success"].as_bool() == Some(true) {
      break body;
    }
    let detail = body
      .get("error")
      .or_else(|| body.get("message"))
      .and_then(|v| v.as_str())
      .unwrap_or("no detail");
    if key_idx + 1 < keys.len() {
      eprintln!(
        "  key #{} failed ({code}: {detail}); switching to key #{}…",
        key_idx + 1,
        key_idx + 2
      );
      key_idx += 1;
      continue;
    }
    return Err(anyhow!("all {} keys failed (last {code}: {detail})", keys.len()));
  };

  if let Some(c) = body["credits_remaining"].as_i64() {
    eprintln!("credits left: {c}");
  }

  let tweets = body["tweets"].as_array().cloned().unwrap_or_default();
  let mut count = 0usize;
  let mut skipped_old = 0usize;
  for tweet in &tweets {
    let created = parse_created_at(tweet["legacy"]["created_at"].as_str().unwrap_or_default())?;
    if created < cfg.since {
      skipped_old += 1;
      continue;
    }
    wtr.write_record(&to_row(tweet, &cfg.handle)?)?;
    count += 1;
  }

  wtr.flush()?;
  eprintln!(
    "Done. Wrote {count} X posts (>= {}) to {} ({} tweets returned, {} older than cutoff).",
    cfg.since.date_naive(),
    cfg.out,
    tweets.len(),
    skipped_old
  );
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;
  #[test]
  fn maps_tweet_to_row() {
    let t: Value = serde_json::json!({
      "rest_id": "2098610807877595502",
      "url": "https://x.com/realDonaldTrump/status/2098610807877595502",
      "legacy": {
        "created_at": "Sat Sep 12 03:13:01 +0000 2026",
        "id_str": "2098610807877595502",
        "full_text": "MAKE AMERICA GREAT AGAIN! #MAGA",
        "favorite_count": 141272,
        "retweet_count": 20990,
        "is_quote_status": false,
        "entities": {
          "hashtags": [{"text": "MAGA"}],
          "user_mentions": [],
          "urls": []
        },
        "extended_entities": {
          "media": [{"media_url_https": "https://pbs.twimg.com/x.jpg"}]
        }
      }
    });
    let row = to_row(&t, "realDonaldTrump").unwrap();
    assert_eq!(row[0], "2098610807877595502");
    assert_eq!(row[1], "2026-09-12 03:13:01+00:00");
    assert_eq!(row[2], "Twitter");
    assert_eq!(row[5], "141272"); // favorite_count
    assert_eq!(row[6], "20990"); // repost_count
    assert_eq!(row[10], "5"); // word_count
    assert_eq!(row[11], "MAGA");
    assert_eq!(row[14], "1"); // media_count
  }
}
