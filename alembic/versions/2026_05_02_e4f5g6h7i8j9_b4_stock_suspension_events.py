"""b4_stock_suspension_events

Revision ID: e4f5g6h7i8j9
Revises: d3e4f5g6h7i8
Create Date: 2026-05-02 00:00:00.000000

==============================================================================
m2 collector 重構 PR #4(per blueprint v3.2 r1 §六 #4 B-4)。

新 Bronze 表 stock_suspension_events:個股暫停交易事件。

用途(blueprint §四):
- 模組 prev_trading_day(stock_id, date) 精確計算個股級「前一交易日」
  → 影響 institutional aggregator 等需要逐日對齊的計算
- tw_market_core 識別個股級交易缺口 → 影響 OHLC 連續性判斷
- 取代 v3.1 提案的 tw_market_event_log(只保留個股暫停,砍 DayTradingSuspension
  + DispositionSecurities,blueprint §四 §四.1 拍板)

FinMind dataset:TaiwanStockSuspended

Schema(blueprint §附錄 B):
- PK (market, stock_id, suspension_date) — 同股同日只 1 筆
- 不需 source 欄位(blueprint §五 設計簡潔)
==============================================================================
"""
from typing import Sequence, Union

from alembic import op


# revision identifiers, used by Alembic.
revision: str = "e4f5g6h7i8j9"
down_revision: Union[str, Sequence[str], None] = "d3e4f5g6h7i8"
branch_labels: Union[str, Sequence[str], None] = None
depends_on: Union[str, Sequence[str], None] = None


def upgrade() -> None:
    """建 stock_suspension_events 表。"""
    op.execute(
        """
        CREATE TABLE IF NOT EXISTS stock_suspension_events (
            market              TEXT NOT NULL,
            stock_id            TEXT NOT NULL,
            suspension_date     DATE NOT NULL,
            suspension_time     TEXT,
            resumption_date     DATE,
            resumption_time     TEXT,
            reason              TEXT,
            detail              JSONB,
            PRIMARY KEY (market, stock_id, suspension_date)
        )
        """
    )
    # 給 prev_trading_day(stock_id, date) 模組用的索引:常見查詢「某股某日前後的暫停事件」
    op.execute(
        "CREATE INDEX IF NOT EXISTS idx_sse_stock_date "
        "ON stock_suspension_events(market, stock_id, suspension_date DESC)"
    )


def downgrade() -> None:
    """移除 stock_suspension_events 表。"""
    op.execute("DROP INDEX IF EXISTS idx_sse_stock_date")
    op.execute("DROP TABLE IF EXISTS stock_suspension_events")
