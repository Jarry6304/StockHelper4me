"""silver14_dirty_scaffolding

Revision ID: k0l1m2n3o4p5
Revises: j9k0l1m2n3o4
Create Date: 2026-05-02 00:00:00.000000

==============================================================================
m2 collector 重構 PR #19a(per blueprint v3.2 r1 §五 §5.5 + §十 PR #8 切片)。

PR #19 完整 scope(blueprint PR #8,3 天 work)被切成 3 段獨立可 verify 的 PR:
  - **PR #19a 本檔**:14 張 Silver `*_derived` 表 schema + 3 張 fwd ALTER 加 dirty
                       欄位 + silver/ 骨架(builders / orchestrator stubs)
  - PR #19b:5 個簡單 builder(institutional / valuation / day_trading / margin /
                                foreign_holding,因 Bronze 已 PR #18 落地有真資料)
  - PR #19c:剩 9 個 builder + orchestrator 真實邏輯 + Phase 7a/7b/7c CLI
  - PR #20:Bronze→Silver trigger DDL CREATE + ENABLE(本 PR 不 enable trigger,
            避免 Bronze 雙寫期間每筆 upsert 都觸發級聯)

本檔做 3 件事:

A. 建 14 張 Silver `*_derived` 表(per spec §2.3 canonical 清單):
     1. price_limit_merge_events            (Rust 計算,PR #20 enable)
     2. monthly_revenue_derived             (mirror monthly_revenue)
     3. valuation_daily_derived             (mirror valuation_daily +market_value_weight)
     4. financial_statement_derived         (mirror financial_statement)
     5. institutional_daily_derived         (mirror institutional_daily +gov_bank_net)
     6. margin_daily_derived                (mirror margin_daily + SBL 6 欄)
     7. foreign_holding_derived             (mirror foreign_holding)
     8. holding_shares_per_derived          (mirror holding_shares_per)
     9. day_trading_derived                 (mirror day_trading)
    10. taiex_index_derived                 (mirror market_ohlcv_tw)
    11. us_market_index_derived             (mirror market_index_us)
    12. exchange_rate_derived               (mirror exchange_rate)
    13. market_margin_maintenance_derived   (mirror market_margin_maintenance + 2 欄)
    14. business_indicator_derived          (NEW per spec §6.3)

B. 每張 Silver 表 + 3 張 fwd 表全加共通 dirty 欄位:
     is_dirty BOOLEAN NOT NULL DEFAULT FALSE
     dirty_at TIMESTAMPTZ
   + partial index ON (...) WHERE is_dirty(orchestrator pull queue 用)

C. ALTER price_daily_fwd / price_weekly_fwd / price_monthly_fwd 加 dirty 欄位
   (這 3 張 PR #17 已建,本檔加欄位讓 §5.5 trigger pattern 在 PR #20 一致 apply)

依據:
- m2Spec/collector_schema_consolidated_spec_v3_2.md §2.3 / §2.5 / §2.6 / §6.3
- m2Spec/collector_rust_restructure_blueprint_v3_2.md §五 §5.5 / §十 PR #8
- 模板對齊 alembic/versions/2026_05_02_j9k0l1m2n3o4_b_pr18_bronze5_reverse_pivot.py

User 操作流程:
  1. git pull
  2. alembic upgrade head           # 14 張 Silver + 3 fwd ALTER 落地
  3. python -c "from silver import orchestrator"   # 確認 import 通
  4. psql $DATABASE_URL -c "\dt *_derived"          # 看 13 張 *_derived
  5. psql $DATABASE_URL -c "\d institutional_daily_derived"  # 確認 dirty 欄位

Rollback:downgrade DROP 14 張 + 3 張 fwd 砍 dirty 欄位。資料安全(本 PR 不寫資料)。
==============================================================================
"""
from typing import Sequence, Union

from alembic import op


revision: str = "k0l1m2n3o4p5"
down_revision: Union[str, Sequence[str], None] = "j9k0l1m2n3o4"
branch_labels: Union[str, Sequence[str], None] = None
depends_on: Union[str, Sequence[str], None] = None


# =============================================================================
# 14 張 Silver *_derived 表 DDL(共通結構:source 表 cols + dirty 欄位 + index)
# =============================================================================

SILVER_TABLES_DDL = [

    # ─── 1. price_limit_merge_events(Rust 計算,PR #20 才有實際 schema)──────
    """
    CREATE TABLE IF NOT EXISTS price_limit_merge_events (
        market         TEXT NOT NULL,
        stock_id       TEXT NOT NULL,
        date           DATE NOT NULL,
        merge_type     TEXT,                            -- TBD per Rust impl
        detail         JSONB,                           -- TBD per Rust impl
        is_dirty       BOOLEAN NOT NULL DEFAULT FALSE,
        dirty_at       TIMESTAMPTZ,
        PRIMARY KEY (market, stock_id, date)
    )
    """,

    # ─── 2. monthly_revenue_derived ───────────────────────────────────────
    """
    CREATE TABLE IF NOT EXISTS monthly_revenue_derived (
        market         TEXT NOT NULL,
        stock_id       TEXT NOT NULL,
        date           DATE NOT NULL,
        revenue        NUMERIC(20, 2),
        revenue_mom    NUMERIC(10, 4),
        revenue_yoy    NUMERIC(10, 4),
        detail         JSONB,
        is_dirty       BOOLEAN NOT NULL DEFAULT FALSE,
        dirty_at       TIMESTAMPTZ,
        PRIMARY KEY (market, stock_id, date)
    )
    """,

    # ─── 3. valuation_daily_derived(+market_value_weight per §2.6.4)──────
    """
    CREATE TABLE IF NOT EXISTS valuation_daily_derived (
        market               TEXT NOT NULL,
        stock_id             TEXT NOT NULL,
        date                 DATE NOT NULL,
        per                  NUMERIC(10, 4),
        pbr                  NUMERIC(10, 4),
        dividend_yield       NUMERIC(8, 4),
        market_value_weight  NUMERIC(10, 6),            -- §2.6.4 個股佔大盤市值比重
        is_dirty             BOOLEAN NOT NULL DEFAULT FALSE,
        dirty_at             TIMESTAMPTZ,
        PRIMARY KEY (market, stock_id, date)
    )
    """,

    # ─── 4. financial_statement_derived ───────────────────────────────────
    """
    CREATE TABLE IF NOT EXISTS financial_statement_derived (
        market         TEXT NOT NULL,
        stock_id       TEXT NOT NULL,
        date           DATE NOT NULL,
        type           TEXT NOT NULL,
        detail         JSONB,
        is_dirty       BOOLEAN NOT NULL DEFAULT FALSE,
        dirty_at       TIMESTAMPTZ,
        PRIMARY KEY (market, stock_id, date, type),
        CONSTRAINT chk_fin_derived_type CHECK (type IN ('income', 'balance', 'cashflow'))
    )
    """,

    # ─── 5. institutional_daily_derived(+gov_bank_net per §2.6.2)─────────
    """
    CREATE TABLE IF NOT EXISTS institutional_daily_derived (
        market                      TEXT NOT NULL,
        stock_id                    TEXT NOT NULL,
        date                        DATE NOT NULL,
        foreign_buy                 BIGINT,
        foreign_sell                BIGINT,
        foreign_dealer_self_buy     BIGINT,
        foreign_dealer_self_sell    BIGINT,
        investment_trust_buy        BIGINT,
        investment_trust_sell       BIGINT,
        dealer_buy                  BIGINT,
        dealer_sell                 BIGINT,
        dealer_hedging_buy          BIGINT,
        dealer_hedging_sell         BIGINT,
        gov_bank_net                BIGINT,             -- §2.6.2 八大行庫淨買賣
        is_dirty                    BOOLEAN NOT NULL DEFAULT FALSE,
        dirty_at                    TIMESTAMPTZ,
        PRIMARY KEY (market, stock_id, date)
    )
    """,

    # ─── 6. margin_daily_derived(+SBL 6 欄 per §2.6.1)────────────────────
    """
    CREATE TABLE IF NOT EXISTS margin_daily_derived (
        market                                  TEXT NOT NULL,
        stock_id                                TEXT NOT NULL,
        date                                    DATE NOT NULL,
        margin_purchase                         BIGINT,
        margin_sell                             BIGINT,
        margin_balance                          BIGINT,
        short_sale                              BIGINT,
        short_cover                             BIGINT,
        short_balance                           BIGINT,
        detail                                  JSONB,
        -- §2.6.1 SBL 6 欄(融券關鍵 3 + 借券關鍵 3)
        margin_short_sales_short_sales          BIGINT,
        margin_short_sales_short_covering       BIGINT,
        margin_short_sales_current_day_balance  BIGINT,
        sbl_short_sales_short_sales             BIGINT,
        sbl_short_sales_returns                 BIGINT,
        sbl_short_sales_current_day_balance     BIGINT,
        is_dirty                                BOOLEAN NOT NULL DEFAULT FALSE,
        dirty_at                                TIMESTAMPTZ,
        PRIMARY KEY (market, stock_id, date)
    )
    """,

    # ─── 7. foreign_holding_derived ───────────────────────────────────────
    """
    CREATE TABLE IF NOT EXISTS foreign_holding_derived (
        market                  TEXT NOT NULL,
        stock_id                TEXT NOT NULL,
        date                    DATE NOT NULL,
        foreign_holding_shares  BIGINT,
        foreign_holding_ratio   NUMERIC(8, 4),
        detail                  JSONB,
        is_dirty                BOOLEAN NOT NULL DEFAULT FALSE,
        dirty_at                TIMESTAMPTZ,
        PRIMARY KEY (market, stock_id, date)
    )
    """,

    # ─── 8. holding_shares_per_derived ────────────────────────────────────
    """
    CREATE TABLE IF NOT EXISTS holding_shares_per_derived (
        market         TEXT NOT NULL,
        stock_id       TEXT NOT NULL,
        date           DATE NOT NULL,
        detail         JSONB,
        is_dirty       BOOLEAN NOT NULL DEFAULT FALSE,
        dirty_at       TIMESTAMPTZ,
        PRIMARY KEY (market, stock_id, date)
    )
    """,

    # ─── 9. day_trading_derived ───────────────────────────────────────────
    """
    CREATE TABLE IF NOT EXISTS day_trading_derived (
        market             TEXT NOT NULL,
        stock_id           TEXT NOT NULL,
        date               DATE NOT NULL,
        day_trading_buy    BIGINT,
        day_trading_sell   BIGINT,
        detail             JSONB,
        is_dirty           BOOLEAN NOT NULL DEFAULT FALSE,
        dirty_at           TIMESTAMPTZ,
        PRIMARY KEY (market, stock_id, date)
    )
    """,

    # ─── 10. taiex_index_derived(對應 market_ohlcv_tw)─────────────────────
    """
    CREATE TABLE IF NOT EXISTS taiex_index_derived (
        market         TEXT NOT NULL,
        stock_id       TEXT NOT NULL,                   -- TAIEX | TPEx
        date           DATE NOT NULL,
        open           NUMERIC(15, 4),
        high           NUMERIC(15, 4),
        low            NUMERIC(15, 4),
        close          NUMERIC(15, 4),
        volume         BIGINT,
        detail         JSONB,
        is_dirty       BOOLEAN NOT NULL DEFAULT FALSE,
        dirty_at       TIMESTAMPTZ,
        PRIMARY KEY (market, stock_id, date)
    )
    """,

    # ─── 11. us_market_index_derived(對應 market_index_us)─────────────────
    """
    CREATE TABLE IF NOT EXISTS us_market_index_derived (
        market         TEXT NOT NULL,
        stock_id       TEXT NOT NULL,                   -- SPY | ^VIX
        date           DATE NOT NULL,
        open           NUMERIC(15, 4),
        high           NUMERIC(15, 4),
        low            NUMERIC(15, 4),
        close          NUMERIC(15, 4),
        volume         BIGINT,
        detail         JSONB,
        is_dirty       BOOLEAN NOT NULL DEFAULT FALSE,
        dirty_at       TIMESTAMPTZ,
        PRIMARY KEY (market, stock_id, date)
    )
    """,

    # ─── 12. exchange_rate_derived(對應 exchange_rate;PK 含 currency)─────
    """
    CREATE TABLE IF NOT EXISTS exchange_rate_derived (
        market         TEXT NOT NULL,
        date           DATE NOT NULL,
        currency       TEXT NOT NULL,                   -- USD | EUR | JPY ...
        rate           NUMERIC(15, 6),
        detail         JSONB,
        is_dirty       BOOLEAN NOT NULL DEFAULT FALSE,
        dirty_at       TIMESTAMPTZ,
        PRIMARY KEY (market, date, currency)
    )
    """,

    # ─── 13. market_margin_maintenance_derived(+2 欄 per §2.6.3)──────────
    """
    CREATE TABLE IF NOT EXISTS market_margin_maintenance_derived (
        market                          TEXT NOT NULL,
        date                            DATE NOT NULL,
        ratio                           NUMERIC(8, 2),
        total_margin_purchase_balance   BIGINT,         -- §2.6.3 整體市場融資餘額
        total_short_sale_balance        BIGINT,         -- §2.6.3 整體市場融券餘額
        is_dirty                        BOOLEAN NOT NULL DEFAULT FALSE,
        dirty_at                        TIMESTAMPTZ,
        PRIMARY KEY (market, date)
    )
    """,

    # ─── 14. business_indicator_derived(NEW per spec §6.3)────────────────
    """
    CREATE TABLE IF NOT EXISTS business_indicator_derived (
        market              TEXT NOT NULL DEFAULT 'tw',
        stock_id            TEXT NOT NULL DEFAULT '_market_',
        date                DATE NOT NULL,
        leading             NUMERIC(10, 4),
        coincident          NUMERIC(10, 4),
        lagging             NUMERIC(10, 4),
        monitoring          INT,
        monitoring_color    TEXT,
        is_dirty            BOOLEAN NOT NULL DEFAULT FALSE,
        dirty_at            TIMESTAMPTZ,
        PRIMARY KEY (market, stock_id, date)
    )
    """,
]

# 14 個對應 partial index(orchestrator 從 dirty queue pull 用)
SILVER_DIRTY_INDEXES = [
    ("idx_plme_dirty",       "price_limit_merge_events"),
    ("idx_mr_dirty",         "monthly_revenue_derived"),
    ("idx_vd_dirty",         "valuation_daily_derived"),
    ("idx_fs_dirty",         "financial_statement_derived"),
    ("idx_id_dirty",         "institutional_daily_derived"),
    ("idx_md_dirty",         "margin_daily_derived"),
    ("idx_fh_dirty",         "foreign_holding_derived"),
    ("idx_hsp_dirty",        "holding_shares_per_derived"),
    ("idx_dt_dirty",         "day_trading_derived"),
    ("idx_tid_dirty",        "taiex_index_derived"),
    ("idx_usmid_dirty",      "us_market_index_derived"),
    ("idx_erd_dirty",        "exchange_rate_derived"),
    ("idx_mmmd_dirty",       "market_margin_maintenance_derived"),
    ("idx_bid_dirty",        "business_indicator_derived"),
]

# 3 張 fwd 表 ALTER(已存在,加 dirty 欄位)
FWD_TABLES = ["price_daily_fwd", "price_weekly_fwd", "price_monthly_fwd"]


def upgrade() -> None:
    """建 14 張 Silver + 3 張 fwd ALTER + 14 個 dirty index。"""

    # A. 建 14 張 Silver
    for ddl in SILVER_TABLES_DDL:
        op.execute(ddl)

    # B. 14 個 dirty partial index
    for idx_name, table in SILVER_DIRTY_INDEXES:
        op.execute(
            f"CREATE INDEX IF NOT EXISTS {idx_name} "
            f"ON {table} (dirty_at) WHERE is_dirty = TRUE"
        )

    # C. ALTER 3 張 fwd 加 dirty 欄位 + index
    for tbl in FWD_TABLES:
        op.execute(
            f"""
            ALTER TABLE {tbl}
                ADD COLUMN IF NOT EXISTS is_dirty BOOLEAN NOT NULL DEFAULT FALSE,
                ADD COLUMN IF NOT EXISTS dirty_at TIMESTAMPTZ
            """
        )
        op.execute(
            f"CREATE INDEX IF NOT EXISTS idx_{tbl}_dirty "
            f"ON {tbl} (dirty_at) WHERE is_dirty = TRUE"
        )


def downgrade() -> None:
    """DROP 14 張 Silver + 砍 fwd dirty 欄位。資料安全(本 PR 沒寫資料)。"""

    # 反向 C:fwd 表砍 dirty 欄位 + index
    for tbl in reversed(FWD_TABLES):
        op.execute(f"DROP INDEX IF EXISTS idx_{tbl}_dirty")
        op.execute(
            f"""
            ALTER TABLE {tbl}
                DROP COLUMN IF EXISTS dirty_at,
                DROP COLUMN IF EXISTS is_dirty
            """
        )

    # 反向 B + A:DROP 14 張 Silver(index 隨 table drop 自動消)
    for idx_name, table in reversed(SILVER_DIRTY_INDEXES):
        op.execute(f"DROP TABLE IF EXISTS {table}")
