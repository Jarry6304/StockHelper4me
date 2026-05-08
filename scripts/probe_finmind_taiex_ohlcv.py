"""
scripts/probe_finmind_taiex_ohlcv.py
====================================
PR #22 / B-1/B-2 動工前 probe — TAIEX OHLCV 兩個候選 source 真實格式:

1. TaiwanStockTotalReturnIndex:已知 daily price index(market_index_tw 在用)
2. TaiwanVariousIndicators5Seconds:5-sec intraday(spec §六 提到的 source,需 aggregate)
3. 順便 probe 別個可能 daily OHLCV TAIEX dataset(萬一 FinMind 有更直接的 source)

依賴:`FINMIND_TOKEN` 環境變數 + aiohttp。

用法:python scripts/probe_finmind_taiex_ohlcv.py
"""
from __future__ import annotations

import asyncio
import os
import sys

import aiohttp


FINMIND_BASE_URL = "https://api.finmindtrade.com/api/v4/data"
FINMIND_DATALIST_URL = "https://api.finmindtrade.com/api/v4/datalist"


CANDIDATES = [
    # 已知會用的(spec §六 提)
    ("TaiwanStockTotalReturnIndex",         "TAIEX",   "2025-01-01", "2025-01-15"),
    ("TaiwanVariousIndicators5Seconds",     "TAIEX",   "2025-01-02", "2025-01-02"),
    # 順便試別的可能 daily OHLCV TAIEX source(萬一 FinMind 有更直接的)
    ("TaiwanStockMarketIndex",              "TAIEX",   "2025-01-01", "2025-01-15"),
    ("TaiwanStockMarketDailyTrade",         None,      "2025-01-01", "2025-01-15"),  # all_market
    ("TaiwanStockTradingDailyReport",       None,      "2025-01-01", "2025-01-15"),  # all_market
    ("TaiwanStockOHLC",                     "TAIEX",   "2025-01-01", "2025-01-15"),
]


async def probe(
    session: aiohttp.ClientSession,
    token: str,
    dataset: str,
    data_id: str | None,
    start_date: str,
    end_date: str,
) -> None:
    """跑 1 個 segment 看 status + 回欄位。"""
    params = {
        "dataset":    dataset,
        "start_date": start_date,
        "end_date":   end_date,
        "token":      token,
    }
    if data_id:
        params["data_id"] = data_id
    label = f"{dataset:42} (data_id={data_id or 'NONE'}, {start_date}~{end_date})"
    try:
        async with session.get(FINMIND_BASE_URL, params=params, timeout=60) as resp:
            text = await resp.text()
            if resp.status == 200:
                import json
                body = json.loads(text)
                rows = body.get("data", [])
                if rows:
                    fields = sorted(rows[0].keys())
                    print(f"  ✅ {label}")
                    print(f"      → {len(rows)} rows, fields={fields}")
                    print(f"      sample: {json.dumps(rows[0], indent=8, ensure_ascii=False)[:500]}")
                    if len(rows) > 1:
                        print(f"      last:   {json.dumps(rows[-1], indent=8, ensure_ascii=False)[:300]}")
                else:
                    print(f"  ⚠️  {label} → 200 OK but 0 rows")
            else:
                print(f"  ❌ {label} → {resp.status}: {text[:200]}")
    except Exception as e:
        print(f"  💥 {label} → exception: {e}")


async def main() -> int:
    token = os.environ.get("FINMIND_TOKEN")
    if not token:
        print("FINMIND_TOKEN env var not set")
        return 1

    async with aiohttp.ClientSession() as session:
        # 1. /datalist 篩 TAIEX / Index / OHLCV / market 相關 dataset
        print("=" * 80)
        print("Phase 1: /datalist 全部 dataset(篩 Index / OHLCV / Market 相關)")
        print("=" * 80)
        try:
            async with session.get(FINMIND_DATALIST_URL, params={"token": token}, timeout=30) as resp:
                if resp.status == 200:
                    body = await resp.json()
                    items = body.get("data", []) if isinstance(body, dict) else []
                    names = [
                        (item if isinstance(item, str) else item.get("table_id", str(item)))
                        for item in items
                    ]
                    keywords = ["index", "taiex", "tpex", "market", "ohlc", "indicator", "totalreturn", "5sec"]
                    matches = [n for n in names if any(kw in n.lower() for kw in keywords)]
                    print(f"  total: {len(names)},matching: {len(matches)}")
                    for m in sorted(matches):
                        print(f"    - {m}")
                else:
                    print(f"  /datalist HTTP {resp.status}: {await resp.text()[:300]}")
        except Exception as e:
            print(f"  /datalist error: {e}")
        print()

        # 2. probe candidate datasets
        print("=" * 80)
        print("Phase 2: probe 候選 dataset(看 fields + sample row)")
        print("=" * 80)
        for ds, did, sd, ed in CANDIDATES:
            await probe(session, token, ds, did, sd, ed)
            await asyncio.sleep(0.5)

    return 0


if __name__ == "__main__":
    sys.exit(asyncio.run(main()))
