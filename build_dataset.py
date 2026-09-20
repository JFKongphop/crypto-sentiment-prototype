#!/usr/bin/env python3
"""Join each Trump post to the ETH price at its timestamp and label the move.

For every post (from Aug 2017, when ETH/USDT starts trading):
  entry = ETH close of the hour the post falls in
  exit  = ETH close `window` hours later
  label = UP if exit > entry else DOWN
Produces csv/dataset.csv with the price context, labels, and basic features.
"""

import csv
from datetime import datetime, timezone

POSTS = "csv/full.csv"          # merged 2009->2026 (Truth Social + Twitter/X)
PRICES = "csv/eth_price.csv"
OUT = "csv/dataset.csv"
HOUR_MS = 3_600_000

CRYPTO_KW = ["crypto", "bitcoin", "btc", "ethereum", "eth", "coin", "blockchain",
             "digital asset", "stablecoin", "defi", "web3"]
ECON_KW = ["tariff", "fed", "interest rate", "inflation", "economy", "dollar",
           "trade", "market", "stock", "China", "recession", "jobs", "gdp"]


def load_prices():
    """Map hour-start (ms, UTC) -> ETH close price."""
    px = {}
    with open(PRICES) as f:
        for r in csv.DictReader(f):
            px[int(r["open_ms"])] = float(r["close"])
    return px


def hour_floor_ms(dt: datetime) -> int:
    ms = int(dt.timestamp() * 1000)
    return (ms // HOUR_MS) * HOUR_MS


def to_int(s):
    try:
        return int(float(s))
    except (ValueError, TypeError):
        return 0


def main():
    px = load_prices()
    first_ms = min(px)
    last_ms = max(px)

    out_rows = []
    total = aligned = 0
    with open(POSTS) as f:
        for r in csv.DictReader(f):
            total += 1
            try:
                dt = datetime.fromisoformat(r["date"]).astimezone(timezone.utc)
            except ValueError:
                continue
            h0 = hour_floor_ms(dt)
            if h0 < first_ms or h0 > last_ms:
                continue  # outside ETH trading history
            entry = px.get(h0)
            p1 = px.get(h0 + HOUR_MS)
            p24 = px.get(h0 + 24 * HOUR_MS)
            if entry is None or p1 is None:
                continue
            aligned += 1

            text = r["text"] or ""
            low = text.lower()
            ret_1h = (p1 - entry) / entry
            ret_24h = (p24 - entry) / entry if p24 else ""
            out_rows.append({
                "id": r["id"],
                "date": r["date"],
                "platform": r["platform"],
                "text": text,
                # price context
                "eth_entry": round(entry, 2),
                "eth_1h": round(p1, 2),
                "eth_24h": round(p24, 2) if p24 else "",
                "ret_1h": round(ret_1h, 5),
                "ret_24h": round(ret_24h, 5) if p24 else "",
                # labels
                "label_1h": "UP" if ret_1h > 0 else "DOWN",
                "label_24h": ("UP" if ret_24h > 0 else "DOWN") if p24 else "",
                # simple features
                "word_count": to_int(r.get("word_count")) or len(text.split()),
                "favorite_count": to_int(r.get("favorite_count")),
                "repost_count": to_int(r.get("repost_count")),
                "hour_utc": dt.hour,
                "weekday": dt.weekday(),
                "has_crypto_kw": int(any(k in low for k in CRYPTO_KW)),
                "has_econ_kw": int(any(k.lower() in low for k in ECON_KW)),
            })

    fields = list(out_rows[0].keys())
    with open(OUT, "w", newline="") as f:
        w = csv.DictWriter(f, fieldnames=fields)
        w.writeheader()
        w.writerows(out_rows)

    # quick report
    from collections import Counter
    lab = Counter(x["label_1h"] for x in out_rows)
    yrs = Counter(x["date"][:4] for x in out_rows)
    print(f"posts total: {total} | aligned to ETH: {aligned}")
    print(f"1h label balance: {dict(lab)}")
    print(f"crypto-keyword posts: {sum(x['has_crypto_kw'] for x in out_rows)}")
    print(f"by year: {dict(sorted(yrs.items()))}")
    print(f"Wrote {len(out_rows)} rows to {OUT}")


if __name__ == "__main__":
    main()
