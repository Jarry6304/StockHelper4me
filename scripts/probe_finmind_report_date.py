"""
scripts/probe_finmind_report_date.py
=====================================
Probe FinMind raw response for 3 fundamental datasets to find which field
encodes the publish/release date(report_date)。

對應 v0.3 phase 2(alembic b8c9d0e1f2g3):
- monthly_revenue 已知 = `create_time`(已落地)
- financial_statement / business_indicator_tw 待查 publish-date 欄

本 script 對 3 個 dataset 各 fetch 一筆近期資料,把 row 的所有 key 印出來,
讓 user 人工辨認哪個欄是 publish date。常見候選關鍵字:
    create_time / create_date / data_create_date / update_date / release_date /
    announce_date / disclosure_date / publish_date

用法:
    python scripts/probe_finmind_report_date.py
    python scripts/probe_finmind_report_date.py --stock 2330
    python scripts/probe_finmind_report_date.py --datasets TaiwanStockFinancialStatements

Token 來源優先序:--token > FINMIND_TOKEN env > .env 內 FINMIND_TOKEN。
"""

from __future__ import annotations

import argparse
import asyncio
import json
import os
import sys
from datetime import date, timedelta
from pathlib import Path

import aiohttp

# Windows cp950 console UTF-8 修法
if sys.platform == "win32":
    try:
        sys.stdout.reconfigure(encoding="utf-8", errors="replace")
        sys.stderr.reconfigure(encoding="utf-8", errors="replace")
    except AttributeError:
        pass

FINMIND_BASE_URL = "https://api.finmindtrade.com/api/v4/data"
RATE_LIMIT_SEC = 2.5

# (dataset, requires_data_id, default_stock, lookback_days, known_publish_field)
DATASETS = [
    ("TaiwanStockMonthRevenue", True,  "2330", 120, "create_time"),
    ("TaiwanStockFinancialStatements", True,  "2330", 365, None),
    ("TaiwanStockBalanceSheet", True,  "2330", 365, None),
    ("TaiwanStockCashFlowsStatement", True,  "2330", 365, None),
    ("TaiwanBusinessIndicator", False, None,   400, None),
]

# 已知不是 publish-date 的核心欄位 — 印出時標 [data] 區分
DATA_FIELDS_HINT = {
    "date", "stock_id", "stock_name", "country", "revenue", "revenue_year",
    "revenue_month", "type", "value", "origin_name",
    "leading_indicator", "coincident_indicator", "lagging_indicator",
    "monitoring", "monitoring_color",
}

# 可能的 publish-date 關鍵字(優先標出)
PUBLISH_CANDIDATES = {
    "create_time", "create_date", "data_create_date", "update_date",
    "release_date", "announce_date", "disclosure_date", "publish_date",
    "report_date", "filing_date", "post_date", "issued_date",
}


def _resolve_token(cli_token: str | None) -> str | None:
    if cli_token:
        return cli_token
    tok = os.environ.get("FINMIND_TOKEN")
    if tok:
        return tok
    # Try repo root .env
    try:
        from dotenv import load_dotenv
        repo_root = Path(__file__).resolve().parent.parent
        env_path = repo_root / ".env"
        if env_path.exists():
            load_dotenv(env_path)
            return os.environ.get("FINMIND_TOKEN")
    except ImportError:
        pass
    return None


async def _probe_one(
    session: aiohttp.ClientSession,
    token: str,
    dataset: str,
    data_id: str | None,
    start: date,
    end: date,
) -> dict | None:
    params = {
        "dataset": dataset,
        "start_date": start.isoformat(),
        "end_date": end.isoformat(),
        "token": token,
    }
    if data_id:
        params["data_id"] = data_id
    try:
        async with session.get(FINMIND_BASE_URL, params=params, timeout=60) as resp:
            txt = await resp.text()
            if resp.status != 200:
                return {"_error": f"HTTP {resp.status}", "_body_excerpt": txt[:300]}
            body = json.loads(txt)
            data = body.get("data") or []
            if not data:
                return {"_empty": True, "_status": body.get("status"), "_msg": body.get("msg")}
            return {"_sample": data[-1], "_n": len(data), "_keys": sorted(data[0].keys())}
    except Exception as e:
        return {"_error": str(e)}


async def main_async(args) -> int:
    token = _resolve_token(args.token)
    if not token:
        print("[ERROR] FINMIND_TOKEN 未設定。請 export FINMIND_TOKEN=... 或 --token", file=sys.stderr)
        return 2

    targets = DATASETS
    if args.datasets:
        wanted = set(args.datasets.split(","))
        targets = [t for t in DATASETS if t[0] in wanted]
        if not targets:
            print(f"[ERROR] 沒有匹配的 dataset:{args.datasets}", file=sys.stderr)
            return 2

    print(f"{'=' * 70}")
    print(f"FinMind report_date probe — {len(targets)} dataset")
    print(f"{'=' * 70}\n")

    async with aiohttp.ClientSession() as session:
        for ds_name, needs_id, default_stock, lookback, known in targets:
            stock = args.stock or default_stock
            end = date.today()
            start = end - timedelta(days=lookback)

            print(f"[{ds_name}]")
            print(f"  data_id = {stock if needs_id else '(none, market-level)'}")
            print(f"  range   = [{start}, {end}]")
            if known:
                print(f"  known publish field = {known!r}")

            result = await _probe_one(
                session, token, ds_name,
                stock if needs_id else None, start, end,
            )
            if result is None:
                print("  → no result")
            elif "_error" in result:
                print(f"  → ERROR: {result['_error']}")
                if "_body_excerpt" in result:
                    print(f"     body: {result['_body_excerpt']}")
            elif result.get("_empty"):
                print(f"  → empty (status={result.get('_status')}, msg={result.get('_msg')!r})")
            else:
                keys = result["_keys"]
                sample = result["_sample"]
                n = result["_n"]
                print(f"  → rows: {n}, fields: {len(keys)}")
                # 分類欄位
                publish_hits = [k for k in keys if k in PUBLISH_CANDIDATES]
                data_fields = [k for k in keys if k in DATA_FIELDS_HINT]
                other = [k for k in keys if k not in PUBLISH_CANDIDATES and k not in DATA_FIELDS_HINT]
                if publish_hits:
                    print(f"  [PUBLISH-DATE candidates]")
                    for k in publish_hits:
                        v = sample.get(k)
                        print(f"    {k} = {v!r}")
                if other:
                    print(f"  [other fields(請人工辨認 publish-date 候選)]")
                    for k in other:
                        v = sample.get(k)
                        # 截斷長字串
                        v_repr = repr(v)
                        if len(v_repr) > 80:
                            v_repr = v_repr[:77] + "..."
                        print(f"    {k} = {v_repr}")
                if data_fields:
                    print(f"  [data fields(skipped)]:{', '.join(data_fields)}")

            print()
            await asyncio.sleep(RATE_LIMIT_SEC)

    print("=" * 70)
    print("完成。對齊 plan phase 2 §2.2:")
    print("  - 從 [PUBLISH-DATE candidates] 或 [other fields] 找出真正的發布日欄")
    print("  - 更新 collector.toml 對應 entry 加 field_rename(若 FinMind 欄名 ≠ 'report_date')")
    print("  - 或在 field_mapper.py 加 dataset-specific 邏輯把該欄寫進 Bronze.report_date")
    print("  - 重跑 incremental phase 5 讓 collector 把新 row 的 report_date 寫對")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description="Probe FinMind for publish-date fields.")
    parser.add_argument("--token", help="FinMind API token(預設讀環境變數 FINMIND_TOKEN)")
    parser.add_argument("--stock", help="覆蓋 default stock(預設 2330)")
    parser.add_argument(
        "--datasets",
        help="只 probe 指定 dataset(逗號分隔,e.g. TaiwanStockFinancialStatements)",
    )
    args = parser.parse_args()
    return asyncio.run(main_async(args))


if __name__ == "__main__":
    sys.exit(main())
