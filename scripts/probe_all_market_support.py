"""
scripts/probe_all_market_support.py
====================================
測 collector.toml 內 per_stock dataset 哪些支援 all_market(只給 date、不給
data_id 就回全市場)。對齊 v3.23 把 price_limit 從 per_stock 改 all_market 的
probe 流程(那次 probe 出 quirk:multi-day range 靜默回 0 rows → 要 segment_days=1)。

每個 dataset 跑 2 個 probe(都「不」帶 data_id):
  - single-day:start_date == end_date(單一交易日)
  - multi-day :start_date ~ end_date 跨 ~21 天

判定:
  - single-day 回多檔(distinct stock_id > 5)        → 支援 all_market
      multi-day 也回多檔                              → all_market(segment_days 維持)
      multi-day 回 0 檔                               → all_market + segment_days=1(quirk)
  - single-day 4xx 或只回 <= 5 檔                     → 不支援,維持 per_stock
  - single + multi 都 200 但 0 rows                  → INCONCLUSIVE(低頻 dataset 或非交易日)

依賴:`FINMIND_TOKEN` 環境變數 + aiohttp。
用法:
    python scripts/probe_all_market_support.py
    python scripts/probe_all_market_support.py --date 2026-05-15   # 指定單日交易日
"""
from __future__ import annotations

import argparse
import asyncio
import json
import os
import sys
import tomllib
from datetime import date, timedelta
from pathlib import Path

import aiohttp

# Windows cp950 console UTF-8 修法(對齊 scripts/probe_finmind_sponsor_unused.py)
if sys.platform == "win32":
    try:
        sys.stdout.reconfigure(encoding="utf-8", errors="replace")
        sys.stderr.reconfigure(encoding="utf-8", errors="replace")
    except AttributeError:
        pass

FINMIND_BASE_URL = "https://api.finmindtrade.com/api/v4/data"
COLLECTOR_TOML = Path(__file__).parent.parent / "config" / "collector.toml"
RATE_LIMIT_SEC = 1.5
PER_STOCK_MODES = {"per_stock", "per_stock_no_end"}
MULTI_DAY_SPAN = 21


def _recent_weekday(days_back: int) -> date:
    d = date.today() - timedelta(days=days_back)
    while d.weekday() >= 5:  # Sat=5 / Sun=6
        d -= timedelta(days=1)
    return d


def load_per_stock_entries() -> list[dict]:
    with open(COLLECTOR_TOML, "rb") as f:
        cfg = tomllib.load(f)
    out = []
    for api in cfg.get("api", []):
        if api.get("param_mode") in PER_STOCK_MODES and api.get("enabled", True):
            out.append({
                "name": api["name"],
                "dataset": api["dataset"],
                "param_mode": api["param_mode"],
                "segment_days": api.get("segment_days", 365),
            })
    return out


async def _probe(session, token, dataset, start_date, end_date) -> dict:
    """單次 probe(不帶 data_id)→ {status, rows, distinct, fields, error}。"""
    params = {
        "dataset": dataset,
        "start_date": start_date.isoformat(),
        "end_date": end_date.isoformat(),
        "token": token,
    }
    try:
        async with session.get(FINMIND_BASE_URL, params=params, timeout=60) as resp:
            text = await resp.text()
            if resp.status != 200:
                return {"status": resp.status, "rows": 0, "distinct": 0,
                        "fields": [], "error": text[:140]}
            rows = (json.loads(text).get("data") or [])
            distinct = len({
                str(r.get("stock_id")) for r in rows if r.get("stock_id") is not None
            })
            fields = sorted(rows[0].keys()) if rows else []
            return {"status": 200, "rows": len(rows), "distinct": distinct,
                    "fields": fields, "error": ""}
    except Exception as e:  # noqa: BLE001
        return {"status": -1, "rows": 0, "distinct": 0, "fields": [], "error": str(e)[:140]}


def _verdict(single: dict, multi: dict) -> tuple[str, str]:
    if single["status"] != 200:
        return "NO", f"single-day HTTP {single['status']}:{single['error']}"
    if single["distinct"] > 5:
        if multi["status"] == 200 and multi["distinct"] > 5:
            return "YES", "all_market(date range OK)"
        return "YES-1D", f"all_market + segment_days=1(multi-day distinct={multi['distinct']})"
    if single["rows"] == 0 and multi["rows"] == 0:
        return "INCONCLUSIVE", "single + multi 都 0 rows(低頻 dataset 或非交易日)"
    return "NO", f"single-day 只回 distinct={single['distinct']} 檔(疑似需 data_id)"


async def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--date", help="單日 probe 用的交易日 YYYY-MM-DD(預設:近一個 weekday)")
    args = parser.parse_args()

    token = os.environ.get("FINMIND_TOKEN")
    if not token:
        print("FINMIND_TOKEN env var 未設", file=sys.stderr)
        return 1

    single_day = date.fromisoformat(args.date) if args.date else _recent_weekday(6)
    multi_start = single_day - timedelta(days=MULTI_DAY_SPAN)
    entries = load_per_stock_entries()

    print("=" * 78)
    print(f"probe all_market support — {len(entries)} 個 per_stock dataset")
    print(f"  single-day = {single_day}    multi-day = {multi_start} ~ {single_day}")
    print("=" * 78)

    results = []
    async with aiohttp.ClientSession() as session:
        for e in entries:
            single = await _probe(session, token, e["dataset"], single_day, single_day)
            await asyncio.sleep(RATE_LIMIT_SEC)
            multi = await _probe(session, token, e["dataset"], multi_start, single_day)
            await asyncio.sleep(RATE_LIMIT_SEC)
            verdict, note = _verdict(single, multi)
            results.append((e, single, multi, verdict, note))
            print(f"  [{verdict:12}] {e['name']:32} {e['dataset']}")
            print(f"      single: status={single['status']} rows={single['rows']} "
                  f"distinct_stocks={single['distinct']}")
            print(f"      multi : status={multi['status']} rows={multi['rows']} "
                  f"distinct_stocks={multi['distinct']}")
            if single["error"]:
                print(f"      single err: {single['error']}")
            if multi["error"]:
                print(f"      multi  err: {multi['error']}")

    print()
    print("=" * 78)
    print("SUMMARY — collector.toml 建議改法")
    print("=" * 78)
    buckets = {"YES": [], "YES-1D": [], "NO": [], "INCONCLUSIVE": []}
    for row in results:
        buckets[row[3]].append(row)
    for e, _s, _m, _v, _n in buckets["YES"]:
        print(f"  {e['name']:32} → all_market(segment_days 維持 {e['segment_days']})")
    for e, _s, _m, _v, _n in buckets["YES-1D"]:
        print(f"  {e['name']:32} → all_market + segment_days=1")
    for e, _s, _m, _v, note in buckets["NO"]:
        print(f"  {e['name']:32} → 維持 per_stock({note})")
    for e, _s, _m, _v, note in buckets["INCONCLUSIVE"]:
        print(f"  {e['name']:32} → 待確認({note})")
    convertible = len(buckets["YES"]) + len(buckets["YES-1D"])
    print()
    print(f"可轉 all_market:{convertible} / {len(entries)}    "
          f"維持 per_stock:{len(buckets['NO'])}    待確認:{len(buckets['INCONCLUSIVE'])}")
    return 0


if __name__ == "__main__":
    sys.exit(asyncio.run(main()))
