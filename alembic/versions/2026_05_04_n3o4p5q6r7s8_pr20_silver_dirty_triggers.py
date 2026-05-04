"""pr20_silver_dirty_triggers

Revision ID: n3o4p5q6r7s8
Revises: m2n3o4p5q6r7
Create Date: 2026-05-04 12:00:00.000000

==============================================================================
PR #20 — Bronze→Silver dirty trigger ENABLE(blueprint v3.2 r1 §5.5 + §5.7)。

PR #19a 落了 14 張 Silver `*_derived` schema + dirty 欄位 + 14 個 partial index
(`WHERE is_dirty = TRUE`),但**不啟用 trigger**(避免 Bronze 雙寫期間每筆
upsert 都觸發級聯)。本 PR 把 Bronze→Silver dirty trigger 接上,讓 dirty queue
真正生效:

  Bronze upsert  →  trigger 自動 mark Silver row is_dirty = TRUE / dirty_at = NOW()

對齊 cores_overview.md §7.5「Bronze 變更 trigger 設此 flag」契約 + §10.0 唯一
計算路徑原則。

------------------------------------------------------------------------------
6 個 trigger function
------------------------------------------------------------------------------

1. trg_mark_silver_dirty(silver_table TEXT)
   通用 3-col PK (market, stock_id, date) Silver 表;TG_ARGV[0] = silver 表名。
   覆蓋 10 張 Bronze→Silver 對映:
     institutional_investors_tw      → institutional_daily_derived
     margin_purchase_short_sale_tw   → margin_daily_derived
     securities_lending_tw           → margin_daily_derived (整合 SBL,二 source 同 target)
     foreign_investor_share_tw       → foreign_holding_derived
     holding_shares_per_tw           → holding_shares_per_derived
     day_trading_tw                  → day_trading_derived
     valuation_per_tw                → valuation_daily_derived
     monthly_revenue_tw              → monthly_revenue_derived
     market_ohlcv_tw                 → taiex_index_derived
     market_index_us                 → us_market_index_derived

2. trg_mark_financial_stmt_dirty()
   financial_statement_tw 4-col Bronze PK (market, stock_id, date, event_type) →
   financial_statement_derived 4-col Silver PK (market, stock_id, date, type)。
   Bronze.event_type ↔ Silver.type 映射(income / balance / cashflow)。

3. trg_mark_exchange_rate_dirty()
   exchange_rate Bronze (market, date, currency) →
   exchange_rate_derived (market, date, currency)。PK 含 currency 不含 stock_id。

4. trg_mark_market_margin_dirty()
   market_margin_maintenance (market, date) →
   market_margin_maintenance_derived (market, date)。2-col PK,無 stock_id。

5. trg_mark_business_indicator_dirty()
   business_indicator_tw Bronze 2-col PK (market, date) →
   business_indicator_derived 3-col Silver PK (market, stock_id, date) 用 sentinel
   stock_id = '_market_' 注入。

6. trg_mark_fwd_silver_dirty()
   price_adjustment_events 寫入 → 該 (market, stock_id) 全段歷史 fwd 4 張表
   (price_daily_fwd / price_weekly_fwd / price_monthly_fwd /
   price_limit_merge_events)整檔 mark is_dirty = TRUE。
   理由:multiplier 倒推設計,新除權息會回頭改全段歷史值,不只標單日。

------------------------------------------------------------------------------
15 個 trigger CREATE
------------------------------------------------------------------------------

每個 Bronze 對應的 trigger 用 `AFTER INSERT OR UPDATE FOR EACH ROW`,在 Bronze
upsert 完成後同 transaction 把 Silver 對應 row 標 dirty。配合 §5.5 範例 +
spec §7.5 contract。

------------------------------------------------------------------------------
向下相容
------------------------------------------------------------------------------

短期補丁路徑(`post_process.invalidate_fwd_cache` + `phase_executor` write
price_adjustment_events 後 reset `stock_sync_status.fwd_adj_valid=0`)由 trigger
完整接管,改 deprecated;後者的 active call site 在本 PR 同 commit 移除,函式
留給 1~2 sprint 相容期(emergency manual ops 仍可呼叫)。

orchestrator `_run_7c` 同 PR 改成 `stock_ids=None` 時從 `price_daily_fwd.is_dirty`
pull 待算清單(走 dirty queue),取代過去走 Rust 內建 `fwd_adj_valid=0` lookup。

==============================================================================
"""

from typing import Sequence, Union

from alembic import op


revision: str = "n3o4p5q6r7s8"
down_revision: Union[str, Sequence[str], None] = "m2n3o4p5q6r7"
branch_labels: Union[str, Sequence[str], None] = None
depends_on: Union[str, Sequence[str], None] = None


# =============================================================================
# Trigger function DDL
# =============================================================================

FN_GENERIC = """
CREATE OR REPLACE FUNCTION trg_mark_silver_dirty()
RETURNS TRIGGER AS $$
DECLARE
    silver_table TEXT := TG_ARGV[0];
BEGIN
    EXECUTE format(
        'INSERT INTO %I (market, stock_id, date, is_dirty, dirty_at)
         VALUES ($1, $2, $3, TRUE, NOW())
         ON CONFLICT (market, stock_id, date) DO UPDATE
            SET is_dirty = TRUE, dirty_at = NOW()',
        silver_table
    ) USING NEW.market, NEW.stock_id, NEW.date;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;
"""

FN_FINANCIAL_STMT = """
CREATE OR REPLACE FUNCTION trg_mark_financial_stmt_dirty()
RETURNS TRIGGER AS $$
BEGIN
    INSERT INTO financial_statement_derived
        (market, stock_id, date, type, is_dirty, dirty_at)
    VALUES
        (NEW.market, NEW.stock_id, NEW.date, NEW.event_type, TRUE, NOW())
    ON CONFLICT (market, stock_id, date, type) DO UPDATE
        SET is_dirty = TRUE, dirty_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;
"""

FN_EXCHANGE_RATE = """
CREATE OR REPLACE FUNCTION trg_mark_exchange_rate_dirty()
RETURNS TRIGGER AS $$
BEGIN
    INSERT INTO exchange_rate_derived
        (market, date, currency, is_dirty, dirty_at)
    VALUES
        (NEW.market, NEW.date, NEW.currency, TRUE, NOW())
    ON CONFLICT (market, date, currency) DO UPDATE
        SET is_dirty = TRUE, dirty_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;
"""

FN_MARKET_MARGIN = """
CREATE OR REPLACE FUNCTION trg_mark_market_margin_dirty()
RETURNS TRIGGER AS $$
BEGIN
    INSERT INTO market_margin_maintenance_derived
        (market, date, is_dirty, dirty_at)
    VALUES
        (NEW.market, NEW.date, TRUE, NOW())
    ON CONFLICT (market, date) DO UPDATE
        SET is_dirty = TRUE, dirty_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;
"""

FN_BUSINESS_INDICATOR = """
CREATE OR REPLACE FUNCTION trg_mark_business_indicator_dirty()
RETURNS TRIGGER AS $$
BEGIN
    INSERT INTO business_indicator_derived
        (market, stock_id, date, is_dirty, dirty_at)
    VALUES
        (NEW.market, '_market_', NEW.date, TRUE, NOW())
    ON CONFLICT (market, stock_id, date) DO UPDATE
        SET is_dirty = TRUE, dirty_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;
"""

FN_FWD = """
CREATE OR REPLACE FUNCTION trg_mark_fwd_silver_dirty()
RETURNS TRIGGER AS $$
BEGIN
    UPDATE price_daily_fwd
       SET is_dirty = TRUE, dirty_at = NOW()
     WHERE market = NEW.market AND stock_id = NEW.stock_id;
    UPDATE price_weekly_fwd
       SET is_dirty = TRUE, dirty_at = NOW()
     WHERE market = NEW.market AND stock_id = NEW.stock_id;
    UPDATE price_monthly_fwd
       SET is_dirty = TRUE, dirty_at = NOW()
     WHERE market = NEW.market AND stock_id = NEW.stock_id;
    UPDATE price_limit_merge_events
       SET is_dirty = TRUE, dirty_at = NOW()
     WHERE market = NEW.market AND stock_id = NEW.stock_id;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;
"""

ALL_FUNCTIONS = [
    FN_GENERIC, FN_FINANCIAL_STMT, FN_EXCHANGE_RATE, FN_MARKET_MARGIN,
    FN_BUSINESS_INDICATOR, FN_FWD,
]


# =============================================================================
# Trigger CREATE 表(15 個)
# (trigger_name, bronze_table, function_name, function_args)
# 用 EXECUTE FUNCTION foo(arg) — args 用 SQL literal 形式拼接
# =============================================================================

TRIGGERS_GENERIC = [
    ("mark_institutional_derived_dirty",       "institutional_investors_tw",     "institutional_daily_derived"),
    ("mark_margin_derived_from_margin_dirty",  "margin_purchase_short_sale_tw",  "margin_daily_derived"),
    ("mark_margin_derived_from_sbl_dirty",     "securities_lending_tw",          "margin_daily_derived"),
    ("mark_foreign_holding_derived_dirty",     "foreign_investor_share_tw",      "foreign_holding_derived"),
    ("mark_holding_shares_per_derived_dirty",  "holding_shares_per_tw",          "holding_shares_per_derived"),
    ("mark_day_trading_derived_dirty",         "day_trading_tw",                 "day_trading_derived"),
    ("mark_valuation_derived_dirty",           "valuation_per_tw",               "valuation_daily_derived"),
    ("mark_monthly_revenue_derived_dirty",     "monthly_revenue_tw",             "monthly_revenue_derived"),
    ("mark_taiex_index_derived_dirty",         "market_ohlcv_tw",                "taiex_index_derived"),
    ("mark_us_market_index_derived_dirty",     "market_index_us",                "us_market_index_derived"),
]

TRIGGERS_SPECIAL = [
    # (trigger_name, bronze_table, function_name)
    ("mark_financial_stmt_derived_dirty",      "financial_statement_tw",         "trg_mark_financial_stmt_dirty"),
    ("mark_exchange_rate_derived_dirty",       "exchange_rate",                  "trg_mark_exchange_rate_dirty"),
    ("mark_market_margin_derived_dirty",       "market_margin_maintenance",      "trg_mark_market_margin_dirty"),
    ("mark_business_indicator_derived_dirty",  "business_indicator_tw",          "trg_mark_business_indicator_dirty"),
    ("mark_fwd_dirty_on_event",                "price_adjustment_events",        "trg_mark_fwd_silver_dirty"),
]


def _drop_trigger_if_exists(name: str, table: str) -> None:
    op.execute(f"DROP TRIGGER IF EXISTS {name} ON {table}")


# =============================================================================
# Upgrade / downgrade
# =============================================================================

def upgrade() -> None:
    """6 functions + 15 triggers。"""

    # A. CREATE OR REPLACE 6 個 trigger function(idempotent)
    for fn_ddl in ALL_FUNCTIONS:
        op.execute(fn_ddl)

    # B. 10 個通用 trigger(走 trg_mark_silver_dirty,silver 表名為 TG_ARGV)
    for trg_name, bronze_table, silver_table in TRIGGERS_GENERIC:
        _drop_trigger_if_exists(trg_name, bronze_table)
        op.execute(
            f"""
            CREATE TRIGGER {trg_name}
            AFTER INSERT OR UPDATE ON {bronze_table}
            FOR EACH ROW EXECUTE FUNCTION trg_mark_silver_dirty('{silver_table}')
            """
        )

    # C. 5 個 special trigger(各自固定 silver target)
    for trg_name, bronze_table, fn_name in TRIGGERS_SPECIAL:
        _drop_trigger_if_exists(trg_name, bronze_table)
        op.execute(
            f"""
            CREATE TRIGGER {trg_name}
            AFTER INSERT OR UPDATE ON {bronze_table}
            FOR EACH ROW EXECUTE FUNCTION {fn_name}()
            """
        )


def downgrade() -> None:
    """DROP 15 triggers + 6 functions(資料安全:本 PR 沒寫資料)。"""

    # 反向 C
    for trg_name, bronze_table, _fn in TRIGGERS_SPECIAL:
        _drop_trigger_if_exists(trg_name, bronze_table)

    # 反向 B
    for trg_name, bronze_table, _silver in TRIGGERS_GENERIC:
        _drop_trigger_if_exists(trg_name, bronze_table)

    # 反向 A
    for fn_name in [
        "trg_mark_silver_dirty()",
        "trg_mark_financial_stmt_dirty()",
        "trg_mark_exchange_rate_dirty()",
        "trg_mark_market_margin_dirty()",
        "trg_mark_business_indicator_dirty()",
        "trg_mark_fwd_silver_dirty()",
    ]:
        op.execute(f"DROP FUNCTION IF EXISTS {fn_name}")
