//! Scrape Donald Trump's Truth Social posts from 2026 onward into the same
//! 18-column schema as `djt_posts_dec2025.csv`, so the output can be appended.
//!
//! Fetches via the ScrapeCreators API (https://scrapecreators.com), which
//! returns Mastodon-shaped Truth Social posts and sidesteps Truth Social's
//! own login/Cloudflare. Each page (~20 posts) costs 1 API credit.
//!
//! Setup (put the key in .env or export it):
//!     export SCRAPECREATORS_API_KEY="your_key"
//!
//! Run:
//!     cargo run --release --bin run                          # 2026-01-01 -> now
//!     cargo run --release --bin run -- --since 2026-01-01 --out djt_posts_2026.csv

use std::env;

use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use regex::Regex;
use serde_json::Value;

const ENDPOINT: &str = "https://api.scrapecreators.com/v1/truthsocial/user/posts";

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
  max_id: Option<String>,
}

fn parse_args() -> Result<Config> {
  let mut since = "2026-01-01".to_string();
  let mut out = "djt_posts_2026.csv".to_string();
  let mut handle = "realDonaldTrump".to_string();
  let mut max_id: Option<String> = None;

  let args: Vec<String> = env::args().skip(1).collect();
  let mut i = 0;
  while i < args.len() {
    match args[i].as_str() {
      "--since" => {
        since = take(&args, &mut i)?;
      }
      "--out" => {
        out = take(&args, &mut i)?;
      }
      "--handle" => {
        handle = take(&args, &mut i)?;
      }
      // Resume paging just older than this post id (skip already-fetched pages).
      "--max-id" => {
        max_id = Some(take(&args, &mut i)?);
      }
      other => return Err(anyhow!("unknown argument: {other}")),
    }
    i += 1;
  }

  let date = NaiveDate::parse_from_str(&since, "%Y-%m-%d")
    .with_context(|| format!("--since must be YYYY-MM-DD, got {since}"))?;
  let since = Utc.from_utc_datetime(&date.and_hms_opt(0, 0, 0).unwrap());
  Ok(Config {
    since,
    out,
    handle,
    max_id,
  })
}

fn take(args: &[String], i: &mut usize) -> Result<String> {
  *i += 1;
  args
    .get(*i)
    .cloned()
    .ok_or_else(|| anyhow!("missing value for {}", args[*i - 1]))
}

/// Strip Truth Social HTML content down to plain text.
fn html_to_text(content: &str, br: &Regex, block: &Regex, tag: &Regex) -> String {
  let s = br.replace_all(content, "\n");
  let s = block.replace_all(&s, "\n");
  let s = tag.replace_all(&s, "");
  let s = html_unescape(&s);
  s.trim().to_string()
}

fn html_unescape(s: &str) -> String {
  s.replace("&amp;", "&")
    .replace("&lt;", "<")
    .replace("&gt;", ">")
    .replace("&quot;", "\"")
    .replace("&#39;", "'")
    .replace("&apos;", "'")
    .replace("&nbsp;", " ")
}

fn parse_created_at(raw: &str) -> Result<DateTime<Utc>> {
  Ok(DateTime::parse_from_rfc3339(raw)?.with_timezone(&Utc))
}

/// Build one CSV row (18 fields) from a status JSON value.
fn to_row(
  status: &Value,
  handle: &str,
  href_re: &Regex,
  br: &Regex,
  block: &Regex,
  tag: &Regex,
) -> Result<Vec<String>> {
  let content = status["content"].as_str().unwrap_or("");
  let text = html_to_text(content, br, block, tag);
  let word_count = text.split_whitespace().count();

  let hashtags = join_field(&status["tags"], "name");
  let user_mentions = join_field(&status["mentions"], "username");
  let media = status["media_attachments"]
    .as_array()
    .cloned()
    .unwrap_or_default();
  let media_urls = media
    .iter()
    .filter_map(|m| m["url"].as_str())
    .collect::<Vec<_>>()
    .join(",");
  let urls = href_re
    .captures_iter(content)
    .map(|c| c[1].to_string())
    .collect::<Vec<_>>()
    .join(",");

  let date = parse_created_at(status["created_at"].as_str().unwrap_or_default())?
    .format("%Y-%m-%d %H:%M:%S%:z")
    .to_string();

  let post_url = status["url"]
    .as_str()
    .or_else(|| status["uri"].as_str())
    .unwrap_or("");
  let in_reply_to = status["in_reply_to_id"].as_str().unwrap_or("");

  Ok(vec![
    status["id"].as_str().unwrap_or("").to_string(),
    date,
    "Truth Social".to_string(),
    handle.to_string(),
    text,
    num(&status["favourites_count"]),
    num(&status["reblogs_count"]),
    pybool(!status["quote"].is_null()),
    pybool(!status["reblog"].is_null()),
    pybool(false),
    word_count.to_string(),
    hashtags,
    urls,
    user_mentions,
    media.len().to_string(),
    media_urls,
    post_url.to_string(),
    in_reply_to.to_string(),
  ])
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

/// Load ScrapeCreators API keys in priority order. Rotation uses them in this
/// order, moving to the next only when the current one runs out of credits.
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
    return Err(anyhow!(
      "no API keys found; set FIRST_API_KEY (and optionally SECOND_API_KEY, THRID_API_KEY) in .env"
    ));
  }
  Ok(keys)
}

/// A cursor may arrive as a JSON string or number; normalize to a string.
fn cursor_str(v: &Value) -> Option<String> {
  if v.is_null() {
    None
  } else if let Some(s) = v.as_str() {
    (!s.is_empty()).then(|| s.to_string())
  } else {
    Some(v.to_string())
  }
}

#[tokio::main]
async fn main() -> Result<()> {
  let cfg = parse_args()?;
  let keys = load_keys()?;
  eprintln!("Loaded {} API key(s).", keys.len());

  let client = reqwest::Client::builder().build()?;
  eprintln!("Fetching @{} posts since {}…", cfg.handle, cfg.since);

  // Regexes compiled once.
  let br = Regex::new(r"(?i)<br\s*/?>").unwrap();
  let block = Regex::new(r"(?i)</(p|div)>").unwrap();
  let tag = Regex::new(r"<[^>]+>").unwrap();
  let href_re = Regex::new(r#"href="([^"]+)""#).unwrap();

  let mut wtr = csv::Writer::from_path(&cfg.out)?;
  wtr.write_record(COLUMNS)?;

  let mut next_max_id: Option<String> = cfg.max_id.clone();
  let mut count: usize = 0;
  let mut done = false;
  let mut key_idx = 0usize; // rotate through keys as each runs out of credits
  if let Some(ref m) = next_max_id {
    eprintln!("Resuming from cursor {m} (posts older than it).");
  }

  while !done {
    let mut req = client
      .get(ENDPOINT)
      .header("x-api-key", keys[key_idx].as_str())
      .query(&[("handle", cfg.handle.as_str())]);
    if let Some(ref m) = next_max_id {
      req = req.query(&[("next_max_id", m.as_str())]);
    }

    let resp = req.send().await?;
    let status_code = resp.status();
    let body: Value = resp.json().await.unwrap_or(Value::Null);

    if !status_code.is_success() || body["success"].as_bool() != Some(true) {
      let detail = body
        .get("error")
        .or_else(|| body.get("message"))
        .and_then(|v| v.as_str())
        .unwrap_or("no detail");
      // Likely out of credits on this key — roll over to the next one and
      // retry the SAME page (cursor unchanged).
      if key_idx + 1 < keys.len() {
        eprintln!(
          "  key #{} exhausted ({status_code}: {detail}); switching to key #{}…",
          key_idx + 1,
          key_idx + 2
        );
        key_idx += 1;
        continue;
      }
      // No keys left — save what we have and stop gracefully.
      wtr.flush()?;
      eprintln!(
        "\nStopped: all {} keys exhausted (last: {status_code} — {detail}).\nWrote {count} posts to {} before stopping.",
        keys.len(),
        cfg.out
      );
      return Ok(());
    }

    let posts = body["posts"].as_array().cloned().unwrap_or_default();
    if posts.is_empty() {
      break;
    }

    for status in &posts {
      let created = parse_created_at(status["created_at"].as_str().unwrap_or_default())?;
      if created < cfg.since {
        done = true;
        break;
      }
      wtr.write_record(&to_row(status, &cfg.handle, &href_re, &br, &block, &tag)?)?;
      count += 1;
    }

    if let Some(credits) = body["credits_remaining"].as_i64() {
      eprintln!(
        "  …{count} posts (key #{}, credits left: {credits})",
        key_idx + 1
      );
    } else {
      eprintln!("  …{count} posts (key #{})", key_idx + 1);
    }

    match cursor_str(&body["next_max_id"]) {
      Some(c) => next_max_id = Some(c),
      None => break, // no more pages
    }
  }

  wtr.flush()?;
  eprintln!("Done. Wrote {count} posts to {}", cfg.out);
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;
  #[test]
  fn sample_row_matches_schema() {
    let br = Regex::new(r"(?i)<br\s*/?>").unwrap();
    let block = Regex::new(r"(?i)</(p|div)>").unwrap();
    let tag = Regex::new(r"<[^>]+>").unwrap();
    let href_re = Regex::new(r#"href="([^"]+)""#).unwrap();
    let s: Value = serde_json::json!({
        "id": "115900000000000001",
        "created_at": "2026-01-15T21:54:21.000Z",
        "content": "<p>The Radical Left is at it AGAIN! Visit <a href=\"http://example.com/x\">this</a> now. MAGA!</p>",
        "favourites_count": 52, "reblogs_count": 16,
        "quote": null, "reblog": null,
        "tags": [{"name":"MAGA"}], "mentions": [{"username":"JDVance"}],
        "media_attachments": [{"url":"https://m.example.com/a.jpg"}],
        "url": "https://truthsocial.com/@realDonaldTrump/posts/115900000000000001",
        "in_reply_to_id": null
    });
    let row = to_row(&s, "realDonaldTrump", &href_re, &br, &block, &tag).unwrap();
    assert_eq!(row[1], "2026-01-15 21:54:21+00:00");
    assert_eq!(
      row[4],
      "The Radical Left is at it AGAIN! Visit this now. MAGA!"
    );
    assert_eq!(row[7], "False"); // quote_flag
    assert_eq!(row[10], "11"); // word_count
    assert_eq!(row[11], "MAGA");
    assert_eq!(row[12], "http://example.com/x");
    assert_eq!(row[13], "JDVance");
    eprintln!("ROW = {:?}", row);
  }
}
