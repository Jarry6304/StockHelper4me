"""b1_b2_market_ohlcv_tw

Revision ID: h7i8j9k0l1m2
Revises: g6h7i8j9k0l1
Create Date: 2026-05-02 00:00:00.000000

==============================================================================
m2 collector 重構 PR #16(per blueprint v3.2 r1 §六 #8 B-1 / B-2)。

新 Bronze 表 market_ohlcv_tw:TAIEX / TPEx 大盤 OHLCV(日頻)。

設計背景(blueprint §一表級別 / §二 Bronze layer):
- 既有 market_index_tw 來源 TaiwanStockTotalReturnIndex,只給 price(報酬指數
  收盤),不含 OHLC 與成交量
- v3.2 規範 tw_market_core / taiex_core 需要 TAIEX 大盤 OHLCV,故新增
  market_ohlcv_tw 表並存(blueprint §1.1:「保留 + 新增」)
- 來源:TotalReturnIndex(close)+ VariousIndicators5Seconds(intraday 5-sec
  aggregate to daily OHLCV)

Schema(對齊既有 market_index_us OHLCV pattern):
- PK (market, stock_id, date) — 同股同日只 1 筆
- stock_id ∈ {TAIEX, TPEx}
- detail JSONB 給後續可能加的欄位用(如 turnover / change_pct)

本 PR 範圍:
- ✅ alembic migration(本檔)
- ✅ src/schema_pg.sql baseline 同步
- ❌ 不加 collector.toml [[api]] entries:multi-source merge(兩支 API → 一張表)
  邏輯 v2.0 framework 不支援,留給 PR #17 重構 phase_executor 時一併做。
  schema 先建可 unblock 後續 PR 開發/驗證。
==============================================================================
"""
from typing import Sequence, Union

from alembic import op


# revision identifiers, used by Alembic.
revision: str = "h7i8j9k0l1m2"
down_revision: Union[str, Sequence[str], None] = "g6h7i8j9k0l1"
branch_labels: Union[str, Sequence[str], None] = None
depends_on: Union[str, Sequence[str], None] = None


def upgrade() -> None:
    """建 market_ohlcv_tw 表(TAIEX / TPEx 日 OHLCV)。"""
    op.execute(
        """
        CREATE TABLE IF NOT EXISTS market_ohlcv_tw (
            market          TEXT NOT NULL,
            stock_id        TEXT NOT NULL,         -- TAIEX | TPEx
            date            DATE NOT NULL,
            open            NUMERIC(15, 4),
            high            NUMERIC(15, 4),
            low             NUMERIC(15, 4),
            close           NUMERIC(15, 4),
            volume          BIGINT,
            detail          JSONB,
            source          TEXT NOT NULL DEFAULT 'finmind',
            PRIMARY KEY (market, stock_id, date)
        )
        """
    )
    op.execute(
        "CREATE INDEX IF NOT EXISTS idx_market_ohlcv_tw_id_date "
        "ON market_ohlcv_tw (stock_id, date DESC)"
    )


def downgrade() -> None:
    """移除 market_ohlcv_tw 表。"""
    op.execute("DROP INDEX IF EXISTS idx_market_ohlcv_tw_id_date")
    op.execute("DROP TABLE IF EXISTS market_ohlcv_tw")
