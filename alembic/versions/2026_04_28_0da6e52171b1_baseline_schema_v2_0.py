"""baseline_schema_v2_0

Revision ID: 0da6e52171b1
Revises:
Create Date: 2026-04-28 09:28:28.724047

==============================================================================
Baseline schema for tw-stock-collector v2.0 (Postgres 17 baseline).

設計要點:
1. 此 migration 是「基礎建立(baseline)」,從 0 建到 SCHEMA_VERSION='2.0'
2. 直接執行 src/schema_pg.sql,不重複維護 DDL 兩份
3. downgrade 整個 drop schema(只在開發 reset 時使用,production 禁用)
4. 後續所有 schema 變動都在此 baseline 之上產生 incremental migration
==============================================================================
"""
from pathlib import Path
from typing import Sequence, Union

from alembic import op
import sqlalchemy as sa


# revision identifiers, used by Alembic.
revision: str = '0da6e52171b1'
down_revision: Union[str, Sequence[str], None] = None
branch_labels: Union[str, Sequence[str], None] = None
depends_on: Union[str, Sequence[str], None] = None


def _read_schema_sql() -> str:
    """讀取 src/schema_pg.sql 全文。"""
    project_root = Path(__file__).resolve().parent.parent.parent
    schema_path = project_root / "src" / "schema_pg.sql"
    if not schema_path.exists():
        raise RuntimeError(
            f"schema_pg.sql 不存在於 {schema_path}。"
            "Baseline migration 需要這個檔案。"
        )
    return schema_path.read_text(encoding="utf-8")


def upgrade() -> None:
    """建立 v2.0 完整 schema(對齊 src/schema_pg.sql)。"""
    sql = _read_schema_sql()
    op.execute(sa.text(sql))


def downgrade() -> None:
    """
    Downgrade 到 baseline 之前 = 完全清空 schema。

    生產環境禁用!此 downgrade 會刪除全部表。
    僅供開發環境 reset 用。
    """
    tables_to_drop = [
        # Phase 6 macro
        "fear_greed_index",
        "market_margin_maintenance",
        "institutional_market_daily",
        "exchange_rate",
        "market_index_us",
        # Phase 5 chip / fundamental
        "financial_statement",
        "monthly_revenue",
        "index_weight_daily",
        "day_trading",
        "valuation_daily",
        "holding_shares_per",
        "foreign_holding",
        "margin_daily",
        "institutional_daily",
        # Phase 4 fwd
        "price_monthly_fwd",
        "price_weekly_fwd",
        "price_daily_fwd",
        # Phase 3 raw
        "price_limit",
        "price_daily",
        # Phase 2 events
        "_dividend_policy_staging",
        "price_adjustment_events",
        # Phase 1 meta
        "market_index_tw",
        "trading_calendar",
        "stock_info",
        # System tables
        "api_sync_progress",
        "stock_sync_status",
        "schema_metadata",
    ]
    for table in tables_to_drop:
        op.execute(sa.text(f'DROP TABLE IF EXISTS "{table}" CASCADE'))
