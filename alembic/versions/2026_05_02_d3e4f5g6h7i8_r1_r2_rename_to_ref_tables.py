"""r1_r2_rename_to_ref_tables

Revision ID: d3e4f5g6h7i8
Revises: c2d3e4f5g6h7
Create Date: 2026-05-02 00:00:00.000000

==============================================================================
m2 collector 重構 PR #3:R-1 + R-2 改名 + 部份欄位 rename。
對應 blueprint v3.2 §六 動工順序 #2。

R-1 (trading_calendar → trading_date_ref):
    blueprint §5.1 設計理由「row 存在 = 交易日」,砍 source 欄位(僅 'finmind'
    無區別性);保留 (market, date) PK。

R-2 (stock_info → stock_info_ref):
    blueprint §5.2 spec
    - 表名 rename:stock_info → stock_info_ref
    - 欄位 rename:
        market_type → type
        industry → industry_category
        delist_date → delisting_date
    - 加索引(blueprint 規定):
        idx_sir_active   ON (market, stock_id) WHERE delisting_date IS NULL
        idx_sir_industry ON (industry_category) WHERE industry_category IS NOT NULL

🟡 DEFER(本 PR 不動,留後續 PR 處理):
    保留以下欄位以維持 collector 既有功能:
      - listing_date     ← stock_resolver `min_listing_days` 過濾依賴
      - par_value        ← 未來面額相關計算可能要(也許 P3 後評估砍)
      - detail (JSONB)   ← v1.7 已用來 pack data_update_date
      - source           ← v1.6 加,僅 'finmind' default
      - updated_at       ← ETL 內部追蹤,blueprint §0.1 標「降為 ETL 內部」
    後續 PR 視 stock_resolver 重構決定砍除哪些。

協同改動(同 PR):
    - src/schema_pg.sql baseline 兩個 CREATE TABLE 改名 + 加索引
    - config/collector.toml target_table 跟 field_rename 對齊
    - src/stock_resolver.py SQL 改用新表名 + 新欄位名
    - src/phase_executor.py SQL 同上
==============================================================================
"""
from typing import Sequence, Union

from alembic import op


# revision identifiers, used by Alembic.
revision: str = "d3e4f5g6h7i8"
down_revision: Union[str, Sequence[str], None] = "c2d3e4f5g6h7"
branch_labels: Union[str, Sequence[str], None] = None
depends_on: Union[str, Sequence[str], None] = None


def upgrade() -> None:
    """R-1 + R-2 改名 + 部份欄位 rename。"""
    # ── R-1: trading_calendar → trading_date_ref ──
    op.execute("ALTER TABLE trading_calendar RENAME TO trading_date_ref")
    op.execute("ALTER TABLE trading_date_ref DROP COLUMN IF EXISTS source")

    # ── R-2: stock_info → stock_info_ref ──
    op.execute("ALTER TABLE stock_info RENAME TO stock_info_ref")
    op.execute("ALTER TABLE stock_info_ref RENAME COLUMN market_type TO type")
    op.execute("ALTER TABLE stock_info_ref RENAME COLUMN industry TO industry_category")
    op.execute("ALTER TABLE stock_info_ref RENAME COLUMN delist_date TO delisting_date")

    # blueprint §5.2 規定的兩個 partial indexes
    op.execute(
        "CREATE INDEX IF NOT EXISTS idx_sir_active "
        "ON stock_info_ref(market, stock_id) "
        "WHERE delisting_date IS NULL"
    )
    op.execute(
        "CREATE INDEX IF NOT EXISTS idx_sir_industry "
        "ON stock_info_ref(industry_category) "
        "WHERE industry_category IS NOT NULL"
    )


def downgrade() -> None:
    """退回 v2.0 表名 + 欄位名。

    ⚠️ 警告:downgrade 過程
      1. DROP 兩個新加 index
      2. 欄位 rename 倒推:delisting_date → delist_date / industry_category → industry / type → market_type
      3. 表名倒推:stock_info_ref → stock_info / trading_date_ref → trading_calendar
      4. trading_date_ref 砍掉的 source 欄位**不會自動補回**(無歷史資料),
         若 v2.0 collector 期望此欄位,須手動 ALTER ADD COLUMN source TEXT NOT NULL DEFAULT 'finmind'。
    """
    # 補回 trading_calendar.source(若 downgrade 環境真的需要)
    op.execute("ALTER TABLE trading_date_ref ADD COLUMN IF NOT EXISTS source TEXT NOT NULL DEFAULT 'finmind'")

    # 砍掉新 indexes
    op.execute("DROP INDEX IF EXISTS idx_sir_active")
    op.execute("DROP INDEX IF EXISTS idx_sir_industry")

    # R-2 欄位 + 表名退回
    op.execute("ALTER TABLE stock_info_ref RENAME COLUMN delisting_date TO delist_date")
    op.execute("ALTER TABLE stock_info_ref RENAME COLUMN industry_category TO industry")
    op.execute("ALTER TABLE stock_info_ref RENAME COLUMN type TO market_type")
    op.execute("ALTER TABLE stock_info_ref RENAME TO stock_info")

    # R-1 表名退回
    op.execute("ALTER TABLE trading_date_ref RENAME TO trading_calendar")
