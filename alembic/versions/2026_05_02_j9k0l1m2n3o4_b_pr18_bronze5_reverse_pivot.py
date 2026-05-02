"""b_pr18_bronze5_reverse_pivot

Revision ID: j9k0l1m2n3o4
Revises: i8j9k0l1m2n3
Create Date: 2026-05-02 00:00:00.000000

==============================================================================
m2 collector 重構 PR #18(per blueprint v3.2 r1 §六 #11 / §十 PR #5)。

把 v2.0 pivot/pack 表中 5 張可逆表反推到 v3.2 Bronze raw,單一 migration 落
5 張新表(原子性,對齊 PR #17 慣例)。Coexist 模式:legacy v2.0 表保留,
_legacy_v2 rename 留到 T0+21(blueprint §八.2,後續 PR #21+)。

5 張 Bronze:

  1. institutional_investors_tw   PK(market, stock_id, date, investor_type)
     1 row per investor_type per stock-date(最多 5 行/日:Foreign_Investor /
     Foreign_Dealer_Self / Investment_Trust / Dealer / Dealer_Hedging)
     反推自 institutional_daily 的 10 個 buy/sell 寬欄。

  2. margin_purchase_short_sale_tw  PK(market, stock_id, date)
     14 raw fields(6 stored + 8 detail unpack):融資三 + 融券三 + 融資/融券
     現金償還 + 限額 + 昨日餘額 + 沖抵 + note。
     反推自 margin_daily(6 stored + detail JSONB 8 keys)。

  3. foreign_investor_share_tw     PK(market, stock_id, date)
     11 raw fields(2 stored + 9 detail unpack):外資持股 + 持股比 + 剩餘股
     數 + 上限比 + 陸資上限 + 已發行股數 + 申報日 + 國際代碼 + 公司名 + note。
     反推自 foreign_holding(2 stored + detail JSONB 9 keys)。

  4. day_trading_tw                PK(market, stock_id, date)
     4 raw fields(2 stored + 2 detail unpack):當沖 buy/sell 金額 + flag +
     volume。反推自 day_trading。

  5. valuation_per_tw              PK(market, stock_id, date)
     3 raw fields(per / pbr / dividend_yield)。反推自 valuation_daily,
     最簡單(無 detail)。

每張表加 idx_<table>_stock_date_desc ON (stock_id, date DESC) 給 PR #19 Silver
builder reads 用。

依據:
- m2Spec/collector_rust_restructure_blueprint_v3_2.md §六 #11 / §八.1 / §十 PR #5
- m2Spec/collector_schema_consolidated_spec_v3_2.md §二 / §附錄 B
- scripts/_reverse_pivot_lib.py(SPECS dict 是 single source of truth,
  本 migration 的 DDL 與 lib SPECS 必須對齊欄名)

User 操作流程:
  1. git pull
  2. alembic upgrade head
  3. python scripts/reverse_pivot_institutional.py --stocks 2330 --dry-run
  4. python scripts/reverse_pivot_institutional.py
     python scripts/reverse_pivot_valuation.py
     python scripts/reverse_pivot_day_trading.py
     python scripts/reverse_pivot_margin.py
     python scripts/reverse_pivot_foreign_holding.py
  5. python scripts/verify_pr18_bronze.py    # 5/5 OK

Rollback:downgrade 把 5 張 Bronze 全 DROP。legacy v2.0 表不動,資料安全。
==============================================================================
"""
from typing import Sequence, Union

from alembic import op


revision: str = "j9k0l1m2n3o4"
down_revision: Union[str, Sequence[str], None] = "i8j9k0l1m2n3"
branch_labels: Union[str, Sequence[str], None] = None
depends_on: Union[str, Sequence[str], None] = None


def upgrade() -> None:
    """建 5 張 Bronze 反推目的表 + 索引。"""

    # 1. institutional_investors_tw
    op.execute(
        """
        CREATE TABLE IF NOT EXISTS institutional_investors_tw (
            market         TEXT NOT NULL,
            stock_id       TEXT NOT NULL,
            date           DATE NOT NULL,
            investor_type  TEXT NOT NULL,
            buy            BIGINT,
            sell           BIGINT,
            name           TEXT,
            PRIMARY KEY (market, stock_id, date, investor_type)
        )
        """
    )
    op.execute(
        "CREATE INDEX IF NOT EXISTS idx_institutional_investors_tw_stock_date_desc "
        "ON institutional_investors_tw (stock_id, date DESC)"
    )

    # 2. margin_purchase_short_sale_tw
    op.execute(
        """
        CREATE TABLE IF NOT EXISTS margin_purchase_short_sale_tw (
            market               TEXT NOT NULL,
            stock_id             TEXT NOT NULL,
            date                 DATE NOT NULL,
            margin_purchase      BIGINT,
            margin_sell          BIGINT,
            margin_balance       BIGINT,
            short_sale           BIGINT,
            short_cover          BIGINT,
            short_balance        BIGINT,
            margin_cash_repay    BIGINT,
            margin_prev_balance  BIGINT,
            margin_limit         BIGINT,
            short_cash_repay     BIGINT,
            short_prev_balance   BIGINT,
            short_limit          BIGINT,
            offset_loan_short    BIGINT,
            note                 TEXT,
            PRIMARY KEY (market, stock_id, date)
        )
        """
    )
    op.execute(
        "CREATE INDEX IF NOT EXISTS idx_margin_purchase_short_sale_tw_stock_date_desc "
        "ON margin_purchase_short_sale_tw (stock_id, date DESC)"
    )

    # 3. foreign_investor_share_tw
    op.execute(
        """
        CREATE TABLE IF NOT EXISTS foreign_investor_share_tw (
            market                  TEXT NOT NULL,
            stock_id                TEXT NOT NULL,
            date                    DATE NOT NULL,
            foreign_holding_shares  BIGINT,
            foreign_holding_ratio   NUMERIC(8, 4),
            remaining_shares        BIGINT,
            remain_ratio            NUMERIC(8, 4),
            upper_limit_ratio       NUMERIC(8, 4),
            cn_upper_limit          NUMERIC(8, 4),
            total_issued            BIGINT,
            declare_date            DATE,
            intl_code               TEXT,
            stock_name              TEXT,
            note                    TEXT,
            PRIMARY KEY (market, stock_id, date)
        )
        """
    )
    op.execute(
        "CREATE INDEX IF NOT EXISTS idx_foreign_investor_share_tw_stock_date_desc "
        "ON foreign_investor_share_tw (stock_id, date DESC)"
    )

    # 4. day_trading_tw
    op.execute(
        """
        CREATE TABLE IF NOT EXISTS day_trading_tw (
            market             TEXT NOT NULL,
            stock_id           TEXT NOT NULL,
            date               DATE NOT NULL,
            day_trading_buy    BIGINT,
            day_trading_sell   BIGINT,
            day_trading_flag   TEXT,
            volume             BIGINT,
            PRIMARY KEY (market, stock_id, date)
        )
        """
    )
    op.execute(
        "CREATE INDEX IF NOT EXISTS idx_day_trading_tw_stock_date_desc "
        "ON day_trading_tw (stock_id, date DESC)"
    )

    # 5. valuation_per_tw
    op.execute(
        """
        CREATE TABLE IF NOT EXISTS valuation_per_tw (
            market          TEXT NOT NULL,
            stock_id        TEXT NOT NULL,
            date            DATE NOT NULL,
            per             NUMERIC(10, 4),
            pbr             NUMERIC(10, 4),
            dividend_yield  NUMERIC(8, 4),
            PRIMARY KEY (market, stock_id, date)
        )
        """
    )
    op.execute(
        "CREATE INDEX IF NOT EXISTS idx_valuation_per_tw_stock_date_desc "
        "ON valuation_per_tw (stock_id, date DESC)"
    )


def downgrade() -> None:
    """DROP 5 張 Bronze。legacy v2.0 表不動,資料安全。"""
    op.execute("DROP TABLE IF EXISTS valuation_per_tw")
    op.execute("DROP TABLE IF EXISTS day_trading_tw")
    op.execute("DROP TABLE IF EXISTS foreign_investor_share_tw")
    op.execute("DROP TABLE IF EXISTS margin_purchase_short_sale_tw")
    op.execute("DROP TABLE IF EXISTS institutional_investors_tw")
