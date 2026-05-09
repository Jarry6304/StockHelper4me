"""
inspect_db.py
-------------
PG v3.2 schema 快速檢視 — 從 v1.6 之前的 SQLite hardcode 升 PG 版(PR #21-A 收尾)。

執行(DATABASE_URL 從 .env 自動讀,db.create_writer 內 load_dotenv):
    python scripts/inspect_db.py            # 全部表筆數(分組:Reference / Bronze / Silver / Legacy / System)
    python scripts/inspect_db.py 2330       # 加印特定股票在主要表的明細

設計原則:
  - 對齊 v3.2 schema(PR #18+ 後 Bronze `*_tw` / Silver `*_derived` 並存 v2.0 legacy)
  - 砍掉舊版的「後復權正確性驗證」段(adjustment_factor 欄已在 PR #17 砍掉,改用
    `scripts/av3_spot_check.sql` 做完整驗證)
  - 保留:row count + price_daily / price_adjustment_events / price_*_fwd 明細
  - 新增:Silver `*_derived` 主要表 latest row spot-check
"""

from __future__ import annotations

import sys
from datetime import date, datetime
from decimal import Decimal
from pathlib import Path
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "src"))

from db import create_writer  # noqa: E402


# 分組對齊 v3.2 + PR #18 reverse-pivot + PR #18.5 dual-write 後的 schema 全貌
TABLE_GROUPS: dict[str, list[str]] = {
    "Reference": [
        "stock_info_ref", "trading_date_ref", "schema_metadata",
    ],
    "Bronze (raw FinMind)": [
        # Phase 1-3
        "market_index_tw", "market_ohlcv_tw",
        "price_daily", "price_limit", "price_adjustment_events",
        "_dividend_policy_staging", "stock_suspension_events",
        # PR #18 reverse-pivot 5 張
        "institutional_investors_tw", "margin_purchase_short_sale_tw",
        "foreign_investor_share_tw", "day_trading_tw", "valuation_per_tw",
        # PR #18.5 dual-write 3 張(PR #R3 後升格去 _tw 後綴)
        "monthly_revenue", "financial_statement", "holding_shares_per",
        # 其他 v3.2 Bronze
        "securities_lending_tw", "business_indicator_tw",
        # Phase 6
        "market_index_us", "exchange_rate", "market_margin_maintenance",
        "institutional_market_daily", "fear_greed_index", "index_weight_daily",
    ],
    "Silver (*_derived)": [
        "price_daily_fwd", "price_weekly_fwd", "price_monthly_fwd",
        "price_limit_merge_events",
        "institutional_daily_derived", "margin_daily_derived",
        "foreign_holding_derived", "holding_shares_per_derived",
        "valuation_daily_derived", "day_trading_derived",
        "monthly_revenue_derived", "financial_statement_derived",
        "taiex_index_derived", "us_market_index_derived",
        "exchange_rate_derived", "market_margin_maintenance_derived",
        "business_indicator_derived",
    ],
    "Legacy v2.0(PR #R2 已 rename;PR #R6 後 DROP)": [
        "stock_info", "trading_calendar",
        "institutional_daily", "margin_daily", "foreign_holding",
        "day_trading", "valuation_daily", "index_weight_daily",
        "monthly_revenue_legacy_v2",          # PR #R2 rename
        "financial_statement_legacy_v2",      # PR #R2 rename
        "holding_shares_per_legacy_v2",       # PR #R2 rename
    ],
    "System": [
        "api_sync_progress", "stock_sync_status",
    ],
}


# =============================================================================
# Format helpers
# =============================================================================

def _fmt_date(v: Any) -> str:
    if v is None:
        return "NULL"
    if isinstance(v, (date, datetime)):
        return v.isoformat()
    return str(v)


def _fmt_num(v: Any, fmt: str = ",.2f") -> str:
    if v is None:
        return "NULL"
    if isinstance(v, Decimal):
        v = float(v)
    try:
        return format(v, fmt)
    except (TypeError, ValueError):
        return str(v)


# =============================================================================
# Main
# =============================================================================

def main(argv: list[str]) -> int:
    db = create_writer()
    try:
        rows = db.query(
            "SELECT table_name FROM information_schema.tables "
            "WHERE table_schema = 'public' ORDER BY table_name"
        )
        existing = {r["table_name"] for r in rows}

        # schema_version
        meta = db.query_one(
            "SELECT value FROM schema_metadata WHERE key = 'schema_version'"
        ) if "schema_metadata" in existing else None
        sv = meta["value"] if meta else "(no schema_metadata)"
        print(f"=== PG public schema  schema_version = {sv} ===\n")

        # 各分組 row counts
        for group_name, tables in TABLE_GROUPS.items():
            print(f"--- {group_name} ---")
            print(f"  {'table':<40s} {'rows':>14s}")
            for t in tables:
                if t not in existing:
                    print(f"  {t:<40s} {'(no table)':>14s}")
                    continue
                row = db.query_one(f"SELECT COUNT(*) AS n FROM {t}")
                print(f"  {t:<40s} {row['n']:>14,}")
            print()

        # 未分類的 extras
        all_known = set()
        for ts in TABLE_GROUPS.values():
            all_known.update(ts)
        extras = sorted(existing - all_known)
        if extras:
            print("--- Extras(未分類)---")
            for t in extras:
                row = db.query_one(f"SELECT COUNT(*) AS n FROM {t}")
                print(f"  {t:<40s} {row['n']:>14,}")
            print()

        # 特定 stock 明細
        if len(argv) > 1:
            _per_stock(db, argv[1], existing)

    finally:
        db.close()
    return 0


def _per_stock(db: Any, stock_id: str, existing: set[str]) -> None:
    print(f"=== Stock {stock_id} 明細 ===\n")

    # 1. price_adjustment_events
    if "price_adjustment_events" in existing:
        rows = db.query(
            "SELECT event_type, COUNT(*) AS n FROM price_adjustment_events "
            "WHERE stock_id = %s GROUP BY event_type ORDER BY event_type",
            [stock_id],
        )
        print(f"--- price_adjustment_events for {stock_id} ---")
        if not rows:
            print("  (no events)\n")
        else:
            for r in rows:
                print(f"  {r['event_type']:<24s} {r['n']:>6d}")
            print()

            # 明細(PR #17 後無 adjustment_factor;改印 volume_factor)
            print(f"--- price_adjustment_events 明細 for {stock_id} ---")
            print(f"  {'date':<12s} {'event_type':<22s} {'cash_div':>10s} "
                  f"{'stock_div':>10s} {'vf':>10s}")
            rows = db.query(
                "SELECT date, event_type, cash_dividend, stock_dividend, volume_factor "
                "FROM price_adjustment_events WHERE stock_id = %s ORDER BY date",
                [stock_id],
            )
            for r in rows:
                print(
                    f"  {_fmt_date(r['date']):<12s} {r['event_type']:<22s} "
                    f"{_fmt_num(r['cash_dividend'], '.4f'):>10s} "
                    f"{_fmt_num(r['stock_dividend'], '.4f'):>10s} "
                    f"{_fmt_num(r['volume_factor'], '.4f'):>10s}"
                )
            print()

    # 2. price_daily 範圍 + 最近 5 筆
    if "price_daily" in existing:
        stat = db.query_one(
            "SELECT COUNT(*) AS n, MIN(date) AS d_min, MAX(date) AS d_max "
            "FROM price_daily WHERE stock_id = %s",
            [stock_id],
        )
        if stat and stat["n"] > 0:
            print(f"--- price_daily for {stock_id}(範圍 + 最近 5 筆)---")
            print(
                f"  rows={stat['n']:,}  range="
                f"{_fmt_date(stat['d_min'])} ~ {_fmt_date(stat['d_max'])}"
            )
            print(f"  {'date':<12s} {'open':>9s} {'high':>9s} {'low':>9s} "
                  f"{'close':>9s} {'volume':>14s} {'turnover':>16s}")
            rows = db.query(
                "SELECT date, open, high, low, close, volume, turnover "
                "FROM price_daily WHERE stock_id = %s ORDER BY date DESC LIMIT 5",
                [stock_id],
            )
            for r in rows:
                print(
                    f"  {_fmt_date(r['date']):<12s} "
                    f"{_fmt_num(r['open'], '.2f'):>9s} "
                    f"{_fmt_num(r['high'], '.2f'):>9s} "
                    f"{_fmt_num(r['low'], '.2f'):>9s} "
                    f"{_fmt_num(r['close'], '.2f'):>9s} "
                    f"{_fmt_num(r['volume'], ','):>14s} "
                    f"{_fmt_num(r['turnover'], ',.0f'):>16s}"
                )
            print()
        else:
            print(f"--- price_daily for {stock_id} ---  (no rows)\n")

    # 3. price_daily_fwd 範圍 + 最近 5 筆(含 cum AF / cum vf,PR #17 schema)
    if "price_daily_fwd" in existing:
        stat = db.query_one(
            "SELECT COUNT(*) AS n, MIN(date) AS d_min, MAX(date) AS d_max "
            "FROM price_daily_fwd WHERE stock_id = %s",
            [stock_id],
        )
        if stat and stat["n"] > 0:
            print(f"--- price_daily_fwd for {stock_id}(後復權,最近 5 筆)---")
            print(
                f"  rows={stat['n']:,}  range="
                f"{_fmt_date(stat['d_min'])} ~ {_fmt_date(stat['d_max'])}"
            )
            print(f"  {'date':<12s} {'close':>9s} {'volume':>14s} "
                  f"{'cum_af':>10s} {'cum_vf':>10s}")
            rows = db.query(
                "SELECT date, close, volume, cumulative_adjustment_factor, "
                "       cumulative_volume_factor "
                "FROM price_daily_fwd WHERE stock_id = %s "
                "ORDER BY date DESC LIMIT 5",
                [stock_id],
            )
            for r in rows:
                print(
                    f"  {_fmt_date(r['date']):<12s} "
                    f"{_fmt_num(r['close'], '.2f'):>9s} "
                    f"{_fmt_num(r['volume'], ','):>14s} "
                    f"{_fmt_num(r['cumulative_adjustment_factor'], '.4f'):>10s} "
                    f"{_fmt_num(r['cumulative_volume_factor'], '.4f'):>10s}"
                )
            print()
        else:
            print(f"--- price_daily_fwd for {stock_id} ---  "
                  f"(no rows — Phase 4 未跑或不在清單)\n")

    # 4. Silver `*_derived` 主要表 latest 1 筆 spot-check(PR #19+ 落地)
    silver_targets = [
        ("institutional_daily_derived",
         ["foreign_buy", "foreign_sell", "investment_trust_buy",
          "investment_trust_sell", "dealer_buy", "dealer_sell", "gov_bank_net"]),
        ("margin_daily_derived",
         ["margin_purchase", "margin_sell", "margin_balance",
          "short_sale", "short_balance"]),
        ("foreign_holding_derived",
         ["foreign_holding_shares", "foreign_holding_ratio"]),
        ("valuation_daily_derived",
         ["per", "pbr", "dividend_yield", "market_value_weight"]),
        ("day_trading_derived",
         ["day_trading_buy", "day_trading_sell", "day_trading_ratio"]),
        ("monthly_revenue_derived",
         ["revenue", "revenue_mom", "revenue_yoy"]),
    ]
    silver_printed = False
    for tbl, cols in silver_targets:
        if tbl not in existing:
            continue
        stat = db.query_one(
            f"SELECT COUNT(*) AS n FROM {tbl} WHERE stock_id = %s",
            [stock_id],
        )
        if stat["n"] == 0:
            continue
        if not silver_printed:
            print(f"--- Silver `*_derived` latest row for {stock_id} ---")
            silver_printed = True
        latest = db.query_one(
            f"SELECT date, {', '.join(cols)} FROM {tbl} "
            f"WHERE stock_id = %s ORDER BY date DESC LIMIT 1",
            [stock_id],
        )
        print(f"  {tbl:<36s} rows={stat['n']:>10,}  "
              f"date={_fmt_date(latest['date'])}")
        for c in cols:
            print(f"    {c:<36s} = {_fmt_num(latest.get(c), ','):>16s}")
    if silver_printed:
        print()


if __name__ == "__main__":
    sys.exit(main(sys.argv))
