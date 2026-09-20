#!/usr/bin/env python3
"""Scrape @realDonaldTrump's X/Twitter posts via twscrape into the same
18-column schema as the master CSV. Free (no paid API), but X requires a
logged-in account, so you must add one to twscrape first (see SETUP below).

SETUP (do this once):
  1. Log into x.com in your browser.
  2. Open DevTools -> Application -> Cookies -> https://x.com, copy the values
     of `auth_token` and `ct0`.
  3. Add the account to twscrape (run in this folder):
       ./.venv/bin/twscrape add_cookie <username> "auth_token=<A>; ct0=<B>"
     (username can be anything, e.g. your @handle without the @)

RUN:
  ./.venv/bin/python x_scrape.py --since 2026-01-01 --until 2026-09-15 \
      --out csv/x_2026.csv
"""

from __future__ import annotations

import argparse
import asyncio
import csv
from datetime import datetime, timezone

from twscrape import API

COLUMNS = [
    "id", "date", "platform", "handle", "text",
    "favorite_count", "repost_count", "quote_flag", "repost_flag",
    "deleted_flag", "word_count", "hashtags", "urls", "user_mentions",
    "media_count", "media_urls", "post_url", "in_reply_to",
]


def to_row(t, handle: str) -> dict:
    text = t.rawContent or ""
    media = getattr(t, "media", None)
    photos = list(getattr(media, "photos", []) or []) if media else []
    videos = list(getattr(media, "videos", []) or []) if media else []
    animated = list(getattr(media, "animated", []) or []) if media else []
    media_all = photos + videos + animated
    media_urls = []
    for m in media_all:
        media_urls.append(getattr(m, "url", None) or getattr(m, "thumbnailUrl", "") or "")

    return {
        "id": t.id,
        "date": str(t.date.astimezone(timezone.utc)),  # 2026-09-12 03:13:01+00:00
        "platform": "Twitter",
        "handle": handle,
        "text": text,
        "favorite_count": getattr(t, "likeCount", 0) or 0,
        "repost_count": getattr(t, "retweetCount", 0) or 0,
        "quote_flag": getattr(t, "quotedTweet", None) is not None,
        "repost_flag": getattr(t, "retweetedTweet", None) is not None,
        "deleted_flag": False,
        "word_count": len(text.split()),
        "hashtags": ",".join(getattr(t, "hashtags", []) or []),
        "urls": ",".join(l.url for l in (getattr(t, "links", []) or [])),
        "user_mentions": ",".join(u.username for u in (getattr(t, "mentionedUsers", []) or [])),
        "media_count": len(media_all),
        "media_urls": ",".join(u for u in media_urls if u),
        "post_url": getattr(t, "url", "") or "",
        "in_reply_to": getattr(t, "inReplyToTweetId", "") or "",
    }


async def run(handle: str, since: datetime, until: datetime, out: str, passes: int):
    api = API()

    accounts = await api.pool.accounts_info()
    if not accounts:
        raise SystemExit(
            "No X account in twscrape. Add one first:\n"
            '  ./.venv/bin/twscrape add_cookie <username> "auth_token=<A>; ct0=<B>"'
        )
    print(f"Using {len(accounts)} account(s).")

    user = await api.user_by_login(handle)
    if not user:
        raise SystemExit(f"Could not resolve @{handle} (login/cookie may be invalid).")
    print(f"@{handle} -> id {user.id}, X reports {user.statusesCount} tweets total.")

    # X returns a different, non-chronological slice each pass, so DON'T break
    # early — scan the whole timeline and UNION multiple passes, deduped by id.
    seen: dict[int, dict] = {}
    sources = [
        ("posts", api.user_tweets),
        ("replies", api.user_tweets_and_replies),
    ]
    for p in range(1, passes + 1):
        before = len(seen)
        for name, fn in sources:
            scanned = 0
            async for t in fn(user.id, limit=-1):
                scanned += 1
                d = t.date.astimezone(timezone.utc)
                if since <= d < until and t.id not in seen:
                    seen[t.id] = to_row(t, handle)
        print(f"  pass {p}/{passes}: in-window unique so far {len(seen)} (+{len(seen) - before})")

    rows = sorted(seen.values(), key=lambda r: r["date"], reverse=True)
    with open(out, "w", newline="", encoding="utf-8") as f:
        w = csv.DictWriter(f, fieldnames=COLUMNS)
        w.writeheader()
        w.writerows(rows)
    print(f"Done. Wrote {len(rows)} unique X posts ({since.date()}..{until.date()}) to {out}.")


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--handle", default="realDonaldTrump")
    p.add_argument("--since", default="2026-01-01")
    p.add_argument("--until", default="2026-09-15")
    p.add_argument("--out", default="csv/x_2026.csv")
    p.add_argument("--passes", type=int, default=5,
                   help="how many timeline passes to union (X returns a different slice each time)")
    a = p.parse_args()
    since = datetime.strptime(a.since, "%Y-%m-%d").replace(tzinfo=timezone.utc)
    until = datetime.strptime(a.until, "%Y-%m-%d").replace(tzinfo=timezone.utc)
    asyncio.run(run(a.handle, since, until, a.out, a.passes))


if __name__ == "__main__":
    main()
