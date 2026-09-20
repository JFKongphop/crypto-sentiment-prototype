#!/usr/bin/env python3
"""Fetch hourly ETH/USDT price history from Binance into csv/eth_price.csv.

Free, no API key. Hourly candles let us compute an ETH move over any window
(e.g. +1h, +24h) relative to each Trump post timestamp.
"""

import csv
import time
import urllib.request
import json
from datetime import datetime, timezone

URL = "https://api.binance.com/api/v3/klines"
SYMBOL = "ETHUSDT"
INTERVAL = "1h"
START = int(datetime(2017, 8, 1, tzinfo=timezone.utc).timestamp() * 1000)
OUT = "csv/eth_price.csv"


def fetch(start_ms: int):
    url = f"{URL}?symbol={SYMBOL}&interval={INTERVAL}&startTime={start_ms}&limit=1000"
    with urllib.request.urlopen(url, timeout=30) as r:
        return json.load(r)


def main():
    now_ms = int(time.time() * 1000)
    rows = []
    start = START
    while start < now_ms:
        batch = fetch(start)
        if not batch:
            break
        for k in batch:
            open_ms = k[0]
            dt = datetime.fromtimestamp(open_ms / 1000, tz=timezone.utc)
            rows.append({
                "timestamp": str(dt),      # 2017-08-01 00:00:00+00:00
                "open_ms": open_ms,
                "open": k[1],
                "high": k[2],
                "low": k[3],
                "close": k[4],
                "volume": k[5],
            })
        start = batch[-1][6] + 1  # closeTime of last candle + 1ms
        if len(rows) % 10000 < 1000:
            print(f"  ...{len(rows)} candles ({rows[-1]['timestamp'][:10]})")
        time.sleep(0.15)  # be polite to the API

    with open(OUT, "w", newline="") as f:
        w = csv.DictWriter(f, fieldnames=["timestamp", "open_ms", "open", "high", "low", "close", "volume"])
        w.writeheader()
        w.writerows(rows)
    print(f"Done. Wrote {len(rows)} hourly candles to {OUT} "
          f"({rows[0]['timestamp'][:10]} -> {rows[-1]['timestamp'][:10]})")


if __name__ == "__main__":
    main()
