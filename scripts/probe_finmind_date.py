"""
scripts/probe_finmind_date.py
==============================
針對指定日期直接 probe FinMind(all_market,不帶 data_id),看 FinMind 端到底
有沒有資料。用於診斷某 dataset 沒更新是「FinMind 還沒出」還是「pipeline 丟掉」。

每個 dataset 跑兩個 probe:single-day(--date)+ multi-day(--date 往前 7 天)。
multi-day 的 distinct_dates 可揭露 all_market「單請求只回 1 日」quirk。

用法:
    python scripts/probe_finmind_date.py --date 2026-05-20
    python scripts/probe_finmind_date.py --date 2026-05-20 --datasets TaiwanStockPrice,TaiwanStockTradingDate
"""
from __future__ import annotations

import argparse
import asyncio
import json
import os
import sys
from datetime import date, timedelta

import aiohttp

# Windows cp950 console UTF-8 修法
if sys.platform == "win32":
    try:
        sys.stdout.reconfigure(encoding="utf-8", errors="replace")
        sys.stderr.reconfigure(encoding="utf-8", errors="replace")
    except AttributeError:
        pass

FINMIND_BASE_URL = "https://api.finmindtrade.com/api/v4/data"
DEFAULT_DATASETS = [
    "TaiwanStockPrice",                          # 對照組(已知 all_market 正常)
    "TaiwanStockTradingDate",                    # 交易日曆(trading_date_ref 來源)
    "TaiwanStockInstitutionalInvestorsBuySell",  # 三大法人
]
RATE_LIMIT_SEC = 1.5


async def _probe(session, token, dataset, start_date, end_date) -> str:
    params = {
        "dataset": dataset,
        "start_date": start_date,
        "end_date": end_date,
        "token": token,
    }
    try:
        async with session.get(FINMIND_BASE_URL, params=params, timeout=60) as resp:
            text = await resp.text()
            if resp.status != 200:
                return f"HTTP {resp.status}: {text[:120]}"
            rows = json.loads(text).get("data") or []
            dates = sorted({str(r.get("date")) for r in rows if r.get("date")})
            distinct = len({
                str(r.get("stock_id")) for r in rows if r.get("stock_id") is not None
            })
            span = f"{dates[0]}~{dates[-1]}" if dates else "-"
            return (f"rows={len(rows)} distinct_stocks={distinct} "
                    f"distinct_dates={len(dates)} span={span}")
    except Exception as e:  # noqa: BLE001
        return f"EXCEPTION {type(e).__name__}: {e}"


async def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--date", required=True, help="single-day probe 的日期 YYYY-MM-DD")
    parser.add_argument("--datasets", default=",".join(DEFAULT_DATASETS))
    args = parser.parse_args()

    token = os.environ.get("FINMIND_TOKEN")
    if not token:
        print("FINMIND_TOKEN env var 未設", file=sys.stderr)
        return 1

    single = date.fromisoformat(args.date)
    multi_start = single - timedelta(days=7)
    datasets = [d.strip() for d in args.datasets.split(",") if d.strip()]

    print("probe FinMind all_market(no data_id)")
    print(f"  single-day = {single}    multi-day = {multi_start} ~ {single}")
    print("=" * 78)

    async with aiohttp.ClientSession() as session:
        for d in datasets:
            s = await _probe(session, token, d, single.isoformat(), single.isoformat())
            await asyncio.sleep(RATE_LIMIT_SEC)
            m = await _probe(session, token, d, multi_start.isoformat(), single.isoformat())
            await asyncio.sleep(RATE_LIMIT_SEC)
            print(f"  {d}")
            print(f"    single: {s}")
            print(f"    multi : {m}")

    return 0


if __name__ == "__main__":
    sys.exit(asyncio.run(main()))
