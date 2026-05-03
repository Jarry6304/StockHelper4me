"""pr18_5_bronze3_refetch

Revision ID: l1m2n3o4p5q6
Revises: k0l1m2n3o4p5
Create Date: 2026-05-02 00:00:00.000000

==============================================================================
m2 collector 重構 PR #18.5(per blueprint v3.2 r1 §八.1 Option A)。

PR #18 reverse-pivot 解了 5 張 v2.0 pivot 表的反推(institutional / margin /
foreign_holding / day_trading / valuation)。剩 3 張因 detail JSONB unpack
不可逆,走 Option A 從 FinMind 全量重抓:

  1. holding_shares_per:HoldingSharesLevel taxonomy 在 v2.0 detail JSONB 內,
     反推不知 level 完整集合
  2. financial_statement:中→英 origin_name 對應在 v2.0 packing 過程丟失,
     pack 後 detail key 是 origin_name(英)無從推回 type(中)
  3. monthly_revenue:FinMind 月營收 1 row/股/月,FinMind 不回更細粒度

本 migration 建 3 張 Bronze raw 表:

  1. holding_shares_per_tw   PK(market, stock_id, date, holding_shares_level)
     1 row per (stock × date × level);無 aggregation,FieldMapper 直拷

  2. financial_statement_tw  PK(market, stock_id, date, event_type, origin_name)
     event_type ∈ {income, balance, cashflow}(reuse collector.toml event_type
     機制,對齊 price_adjustment_events convention),3 個 FinMind dataset
     (TaiwanStockFinancialStatements / BalanceSheet / CashFlowsStatement)
     統一進這張 Bronze

  3. monthly_revenue_tw     PK(market, stock_id, date)
     raw FinMind 欄名(revenue / revenue_year / revenue_month / country /
     create_time);v3.2 Silver builder PR #19c rename 到 revenue_yoy / revenue_mom
     對齊 v2.0 monthly_revenue_derived schema

每張加 idx_<table>_stock_date_desc(stock_id, date DESC)給 PR #19c Silver
builder reads 用。

對應 collector.toml 5 個新 entries(本 PR commit 一起加):
  - holding_shares_per_v3      → holding_shares_per_tw
  - monthly_revenue_v3         → monthly_revenue_tw
  - financial_income_v3        → financial_statement_tw, event_type='income'
  - financial_balance_v3       → financial_statement_tw, event_type='balance'
  - financial_cashflow_v3      → financial_statement_tw, event_type='cashflow'

User 操作流程(沙箱無 FinMind 連線,以下都本機跑):

  1. git pull
  2. alembic upgrade head                        # 建 3 張 Bronze
  3. python src/main.py backfill --phases 5      # ~30-40h,dual-write 同時填 v2.0 + v3.2
  4. # 驗證 row count 對齊 v2.0
     psql $DATABASE_URL -c "SELECT 'holding_shares_per_tw', COUNT(*) FROM holding_shares_per_tw
                            UNION ALL SELECT 'financial_statement_tw', COUNT(*) FROM financial_statement_tw
                            UNION ALL SELECT 'monthly_revenue_tw', COUNT(*) FROM monthly_revenue_tw"

依據:
- m2Spec/collector_rust_restructure_blueprint_v3_2.md §八.1 Option A
- m2Spec/collector_schema_consolidated_spec_v3_2.md §2.3
- 模板對齊 alembic/versions/2026_05_02_j9k0l1m2n3o4_*

Coexist 模式(blueprint §八.2):legacy v2.0 表(holding_shares_per /
financial_statement / monthly_revenue)保留;_legacy_v2 rename + DROP 留
T0+21 / T0+60(後續 PR #21+)。

Rollback:downgrade DROP 3 張 Bronze。資料安全(v2.0 路徑不動,refetch 過的
data 在 Bronze 被 DROP,但 v2.0 還有同步寫的 copy)。
==============================================================================
"""
from typing import Sequence, Union

from alembic import op


revision: str = "l1m2n3o4p5q6"
down_revision: Union[str, Sequence[str], None] = "k0l1m2n3o4p5"
branch_labels: Union[str, Sequence[str], None] = None
depends_on: Union[str, Sequence[str], None] = None


# =============================================================================
# 3 張 Bronze raw 表 DDL
# =============================================================================

DDL_HOLDING_SHARES_PER_TW = """
    CREATE TABLE IF NOT EXISTS holding_shares_per_tw (
        market               TEXT NOT NULL,
        stock_id             TEXT NOT NULL,
        date                 DATE NOT NULL,
        holding_shares_level TEXT NOT NULL,
        people               BIGINT,
        percent              NUMERIC(8, 4),
        unit                 BIGINT,
        PRIMARY KEY (market, stock_id, date, holding_shares_level)
    )
"""

DDL_FINANCIAL_STATEMENT_TW = """
    CREATE TABLE IF NOT EXISTS financial_statement_tw (
        market      TEXT NOT NULL,
        stock_id    TEXT NOT NULL,
        date        DATE NOT NULL,
        event_type  TEXT NOT NULL,
        type        TEXT,
        origin_name TEXT NOT NULL,
        value       NUMERIC(20, 4),
        PRIMARY KEY (market, stock_id, date, event_type, origin_name),
        CONSTRAINT chk_fs_tw_event_type CHECK (event_type IN ('income', 'balance', 'cashflow'))
    )
"""

DDL_MONTHLY_REVENUE_TW = """
    CREATE TABLE IF NOT EXISTS monthly_revenue_tw (
        market         TEXT NOT NULL,
        stock_id       TEXT NOT NULL,
        date           DATE NOT NULL,
        revenue        NUMERIC(20, 2),
        revenue_year   NUMERIC(10, 4),
        revenue_month  NUMERIC(10, 4),
        country        TEXT,
        create_time    TIMESTAMPTZ,
        PRIMARY KEY (market, stock_id, date)
    )
"""

INDEXES = [
    ("idx_holding_shares_per_tw_stock_date_desc", "holding_shares_per_tw"),
    ("idx_financial_statement_tw_stock_date_desc", "financial_statement_tw"),
    ("idx_monthly_revenue_tw_stock_date_desc",     "monthly_revenue_tw"),
]


def upgrade() -> None:
    """建 3 張 Bronze refetch 表 + 索引。"""
    op.execute(DDL_HOLDING_SHARES_PER_TW)
    op.execute(DDL_FINANCIAL_STATEMENT_TW)
    op.execute(DDL_MONTHLY_REVENUE_TW)

    for idx_name, table in INDEXES:
        op.execute(
            f"CREATE INDEX IF NOT EXISTS {idx_name} "
            f"ON {table} (stock_id, date DESC)"
        )


def downgrade() -> None:
    """DROP 3 張 Bronze。legacy v2.0 表不動,資料安全。"""
    op.execute("DROP TABLE IF EXISTS monthly_revenue_tw")
    op.execute("DROP TABLE IF EXISTS financial_statement_tw")
    op.execute("DROP TABLE IF EXISTS holding_shares_per_tw")
