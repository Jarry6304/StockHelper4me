"""
verify_pr20_triggers.py
=======================
PR #20 Bronze→Silver dirty trigger 整合測試。

每個 Bronze 表 INSERT 一筆 sentinel row → trigger 自動 mark Silver row dirty
→ 驗證對應 Silver row is_dirty=TRUE / dirty_at IS NOT NULL,跑完清掉 sentinel。

15 個 subtest 對映 alembic n3o4p5q6r7s8 落下的 15 個 trigger:
  10 generic 3-col(market, stock_id, date)
  5 special:
    - financial_statement(Bronze.event_type ↔ Silver.type)
    - exchange_rate(currency 在 PK)
    - market_margin_maintenance(2-col PK,無 stock_id)
    - business_indicator_tw(Bronze 2-col → Silver 注 sentinel '_market_')
    - price_adjustment_events → fwd 4 表整檔 dirty(全段歷史 mark)

Sentinel 慣例:
  - market="TW",stock_id="__PR20__",date="1900-01-01"(實機資料起點 ~1990,絕不衝突)
  - 對 currency PK 用 "PR20";對 PK 含 type 用 "income"
  - 對 fwd test 用 stock_id="__PR20_FWD__" 並 pre-INSERT 4 fwd rows 模擬全段歷史

執行:
  alembic upgrade head     # 確保 PR #20 trigger 已落
  python scripts/verify_pr20_triggers.py

退出碼:0 = 全 OK,1 = 任一 FAIL
"""

from __future__ import annotations

import logging
import sys
from datetime import date
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "src"))

from db import create_writer  # noqa: E402

logging.basicConfig(level=logging.WARNING, format="%(levelname)s %(name)s: %(message)s")
logger = logging.getLogger("verify_pr20")

MARKET = "TW"
STOCK = "__PR20__"
STOCK_FWD = "__PR20_FWD__"
SDATE = date(1900, 1, 1)
SDATE2 = date(1900, 1, 2)
CURRENCY = "PR20"


# =============================================================================
# Subtest 定義
# (name, bronze_insert_sql, bronze_params, bronze_cleanup_sql, bronze_cleanup_params,
#  silver_check_sql, silver_check_params, silver_cleanup_sql, silver_cleanup_params)
# =============================================================================

def _make_subtests() -> list[dict]:
    """15 個 subtest spec(不含 fwd 多表那組)。"""
    base = []

    # ---- 10 generic 3-col Bronze(market, stock_id, date)→ 同 PK Silver ----
    generic = [
        # (bronze, silver, extra_pk_cols, extra_pk_vals)
        ("institutional_investors_tw",  "institutional_daily_derived", ["investor_type"], ["__pr20__"]),
        ("margin_purchase_short_sale_tw", "margin_daily_derived",      [], []),
        ("securities_lending_tw",       "margin_daily_derived",        ["transaction_type", "fee_rate"], ["議借", 0.0]),
        ("foreign_investor_share_tw",   "foreign_holding_derived",     [], []),
        ("holding_shares_per_tw",       "holding_shares_per_derived",  ["holding_shares_level"], ["__pr20__"]),
        ("day_trading_tw",              "day_trading_derived",         [], []),
        ("valuation_per_tw",            "valuation_daily_derived",     [], []),
        ("monthly_revenue_tw",          "monthly_revenue_derived",     [], []),
        ("market_ohlcv_tw",             "taiex_index_derived",         [], []),
        ("market_index_us",             "us_market_index_derived",     [], []),
    ]
    for bronze, silver, extra_cols, extra_vals in generic:
        cols = ["market", "stock_id", "date"] + extra_cols
        params = [MARKET, STOCK, SDATE] + extra_vals
        placeholders = ", ".join(["%s"] * len(params))
        cleanup_where = " AND ".join([f"{c} = %s" for c in cols])
        base.append({
            "name": f"{bronze} → {silver}",
            "bronze_insert": f"INSERT INTO {bronze} ({', '.join(cols)}) VALUES ({placeholders})",
            "bronze_insert_params": params,
            "bronze_cleanup": f"DELETE FROM {bronze} WHERE {cleanup_where}",
            "bronze_cleanup_params": params,
            "silver_check": (
                f"SELECT is_dirty, dirty_at FROM {silver} "
                f"WHERE market = %s AND stock_id = %s AND date = %s"
            ),
            "silver_check_params": [MARKET, STOCK, SDATE],
            "silver_cleanup": (
                f"DELETE FROM {silver} "
                f"WHERE market = %s AND stock_id = %s AND date = %s"
            ),
            "silver_cleanup_params": [MARKET, STOCK, SDATE],
        })

    # ---- 5 special ----
    # financial_statement(Bronze 多 PK col,Silver 4-col PK,event_type → type)
    base.append({
        "name": "financial_statement_tw → financial_statement_derived (event_type→type)",
        "bronze_insert": (
            "INSERT INTO financial_statement_tw "
            "(market, stock_id, date, event_type, origin_name) "
            "VALUES (%s, %s, %s, %s, %s)"
        ),
        "bronze_insert_params": [MARKET, STOCK, SDATE, "income", "__pr20__"],
        "bronze_cleanup": (
            "DELETE FROM financial_statement_tw "
            "WHERE market = %s AND stock_id = %s AND date = %s "
            "AND event_type = %s AND origin_name = %s"
        ),
        "bronze_cleanup_params": [MARKET, STOCK, SDATE, "income", "__pr20__"],
        "silver_check": (
            "SELECT is_dirty, dirty_at FROM financial_statement_derived "
            "WHERE market = %s AND stock_id = %s AND date = %s AND type = %s"
        ),
        "silver_check_params": [MARKET, STOCK, SDATE, "income"],
        "silver_cleanup": (
            "DELETE FROM financial_statement_derived "
            "WHERE market = %s AND stock_id = %s AND date = %s AND type = %s"
        ),
        "silver_cleanup_params": [MARKET, STOCK, SDATE, "income"],
    })

    # exchange_rate(PK 含 currency)
    base.append({
        "name": "exchange_rate → exchange_rate_derived (currency PK)",
        "bronze_insert": (
            "INSERT INTO exchange_rate (market, date, currency, rate) "
            "VALUES (%s, %s, %s, %s)"
        ),
        "bronze_insert_params": [MARKET, SDATE, CURRENCY, 1.0],
        "bronze_cleanup": (
            "DELETE FROM exchange_rate "
            "WHERE market = %s AND date = %s AND currency = %s"
        ),
        "bronze_cleanup_params": [MARKET, SDATE, CURRENCY],
        "silver_check": (
            "SELECT is_dirty, dirty_at FROM exchange_rate_derived "
            "WHERE market = %s AND date = %s AND currency = %s"
        ),
        "silver_check_params": [MARKET, SDATE, CURRENCY],
        "silver_cleanup": (
            "DELETE FROM exchange_rate_derived "
            "WHERE market = %s AND date = %s AND currency = %s"
        ),
        "silver_cleanup_params": [MARKET, SDATE, CURRENCY],
    })

    # market_margin_maintenance(2-col PK,無 stock_id)
    base.append({
        "name": "market_margin_maintenance → market_margin_maintenance_derived (2-col PK)",
        "bronze_insert": (
            "INSERT INTO market_margin_maintenance (market, date, ratio) "
            "VALUES (%s, %s, %s)"
        ),
        "bronze_insert_params": [MARKET, SDATE, 100.0],
        "bronze_cleanup": "DELETE FROM market_margin_maintenance WHERE market = %s AND date = %s",
        "bronze_cleanup_params": [MARKET, SDATE],
        "silver_check": (
            "SELECT is_dirty, dirty_at FROM market_margin_maintenance_derived "
            "WHERE market = %s AND date = %s"
        ),
        "silver_check_params": [MARKET, SDATE],
        "silver_cleanup": "DELETE FROM market_margin_maintenance_derived WHERE market = %s AND date = %s",
        "silver_cleanup_params": [MARKET, SDATE],
    })

    # business_indicator(Bronze 2-col → Silver sentinel stock_id='_market_')
    base.append({
        "name": "business_indicator_tw → business_indicator_derived (sentinel '_market_')",
        "bronze_insert": "INSERT INTO business_indicator_tw (market, date) VALUES (%s, %s)",
        "bronze_insert_params": [MARKET, SDATE],
        "bronze_cleanup": "DELETE FROM business_indicator_tw WHERE market = %s AND date = %s",
        "bronze_cleanup_params": [MARKET, SDATE],
        "silver_check": (
            "SELECT is_dirty, dirty_at FROM business_indicator_derived "
            "WHERE market = %s AND stock_id = %s AND date = %s"
        ),
        "silver_check_params": [MARKET, "_market_", SDATE],
        "silver_cleanup": (
            "DELETE FROM business_indicator_derived "
            "WHERE market = %s AND stock_id = %s AND date = %s"
        ),
        "silver_cleanup_params": [MARKET, "_market_", SDATE],
    })

    return base


# =============================================================================
# fwd 全段歷史 trigger 測試
# =============================================================================

FWD_TABLES = [
    "price_daily_fwd",
    "price_weekly_fwd",
    "price_monthly_fwd",
    "price_limit_merge_events",
]


def _verify_fwd_trigger(db) -> bool:
    """price_adjustment_events 寫入 → 4 fwd 表整檔 dirty(全段歷史 mark)。

    流程:
      1. 對 STOCK_FWD pre-INSERT 2 row × 4 fwd 表(模擬全段歷史)
      2. INSERT 一筆 price_adjustment_events
      3. SELECT 4 fwd 表,確認該 stock 全部 row is_dirty=TRUE / dirty_at NOT NULL
      4. cleanup
    """
    name = "price_adjustment_events → 4 fwd tables (全段歷史 dirty)"

    try:
        # ── 1. pre-INSERT 4 fwd 表各 2 row(模擬已存在的後復權 cache)
        fwd_inserts = {
            "price_daily_fwd": (
                "INSERT INTO price_daily_fwd (market, stock_id, date, close) "
                "VALUES (%s, %s, %s, 100.0)"
            ),
            "price_weekly_fwd": (
                "INSERT INTO price_weekly_fwd (market, stock_id, year, week, close) "
                "VALUES (%s, %s, 1900, 1, 100.0)"
            ),
            "price_monthly_fwd": (
                "INSERT INTO price_monthly_fwd (market, stock_id, year, month, close) "
                "VALUES (%s, %s, 1900, 1, 100.0)"
            ),
            "price_limit_merge_events": (
                "INSERT INTO price_limit_merge_events (market, stock_id, date) "
                "VALUES (%s, %s, %s)"
            ),
        }
        for tbl, sql in fwd_inserts.items():
            if tbl in ("price_weekly_fwd", "price_monthly_fwd"):
                db.update(sql, [MARKET, STOCK_FWD])
            else:
                db.update(sql, [MARKET, STOCK_FWD, SDATE])
                db.update(sql, [MARKET, STOCK_FWD, SDATE2])  # 第 2 row 證明全段都被 mark

        # ── 2. INSERT price_adjustment_events 觸發 trigger
        db.update(
            "INSERT INTO price_adjustment_events "
            "(market, stock_id, date, event_type, volume_factor) "
            "VALUES (%s, %s, %s, %s, %s)",
            [MARKET, STOCK_FWD, SDATE, "split", 1.0],
        )

        # ── 3. 4 表全部 row(該 stock)should be dirty
        all_ok = True
        for tbl in FWD_TABLES:
            rows = db.query(
                f"SELECT is_dirty, dirty_at FROM {tbl} "
                f"WHERE market = %s AND stock_id = %s",
                [MARKET, STOCK_FWD],
            )
            if not rows:
                logger.error(f"  [FAIL] {name} | {tbl}: pre-INSERT row 不見了")
                all_ok = False
                continue
            for r in rows:
                if not r["is_dirty"] or r["dirty_at"] is None:
                    logger.error(
                        f"  [FAIL] {name} | {tbl} row not dirty: "
                        f"is_dirty={r['is_dirty']} dirty_at={r['dirty_at']}"
                    )
                    all_ok = False

        if all_ok:
            print(f"  [OK]   {name}")
        return all_ok

    finally:
        # ── 4. cleanup(逆序)
        db.update(
            "DELETE FROM price_adjustment_events "
            "WHERE market = %s AND stock_id = %s AND date = %s AND event_type = %s",
            [MARKET, STOCK_FWD, SDATE, "split"],
        )
        for tbl in FWD_TABLES:
            db.update(f"DELETE FROM {tbl} WHERE market = %s AND stock_id = %s",
                      [MARKET, STOCK_FWD])


# =============================================================================
# 主流程
# =============================================================================

def _run_subtest(db, spec: dict) -> bool:
    name = spec["name"]
    try:
        db.update(spec["bronze_insert"], spec["bronze_insert_params"])
        row = db.query_one(spec["silver_check"], spec["silver_check_params"])
        if row is None:
            logger.error(f"  [FAIL] {name}: Silver row 不存在(trigger 未觸發?)")
            return False
        if not row["is_dirty"]:
            logger.error(f"  [FAIL] {name}: is_dirty=FALSE")
            return False
        if row["dirty_at"] is None:
            logger.error(f"  [FAIL] {name}: dirty_at IS NULL")
            return False
        print(f"  [OK]   {name}")
        return True
    except Exception as e:
        logger.error(f"  [FAIL] {name}: {type(e).__name__}: {e}")
        return False
    finally:
        # 先 Silver 再 Bronze;Bronze cleanup 不會再觸發 trigger(只有 INSERT/UPDATE 觸發)
        try:
            db.update(spec["silver_cleanup"], spec["silver_cleanup_params"])
        except Exception as e:
            logger.warning(f"  silver cleanup err: {e}")
        try:
            db.update(spec["bronze_cleanup"], spec["bronze_cleanup_params"])
        except Exception as e:
            logger.warning(f"  bronze cleanup err: {e}")


def main() -> int:
    db = create_writer()
    db.init_schema()

    print()
    print("=" * 70)
    print("PR #20 Bronze→Silver dirty trigger 整合測試(15 個 trigger)")
    print("=" * 70)

    results = []
    try:
        for spec in _make_subtests():
            results.append(_run_subtest(db, spec))
        # fwd 全段歷史 trigger
        results.append(_verify_fwd_trigger(db))
    finally:
        db.close()

    total = len(results)
    ok = sum(results)
    print("-" * 70)
    print(f"TOTAL: {ok}/{total} OK")
    print()
    return 0 if ok == total else 1


if __name__ == "__main__":
    sys.exit(main())
