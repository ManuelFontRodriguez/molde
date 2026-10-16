#!/usr/bin/env python3
"""Deterministic 1900-stock dataset (same shape as demo data.json).
The original /tmp harness was lost; this regenerates an equivalent big dataset —
big-build numbers are timing-only per spec (never diffed against golden)."""
import json

SECTORS = ["Consumer Defensive", "Financial Services", "Communication Services",
           "Technology", "Healthcare", "Industrials", "Energy", "Utilities"]
stocks = []
for i in range(1900):
    n = i + 1
    stock = {
        "ticker": f"S{n:04d}",
        "name": f"Sample Company {n} Inc",
        "sector": SECTORS[i % len(SECTORS)],
    }
    if i % 5 == 2:
        stock["upcoming"] = None
    else:
        stock["upcoming"] = {
            "ex_date": f"2026-{(i % 12) + 1:02d}-{(i % 28) + 1:02d}",
            "amount": f"{(i % 300) / 100 + 0.05:.4f}",
            "currency": "USD",
            "confirmed": i % 3 != 1,
        }
    stocks.append(stock)

print(json.dumps({"cycle_id": "2026-09-18T09:00:00Z-demo-big", "stocks": stocks}))
