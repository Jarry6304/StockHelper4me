"""b6_business_indicator_tw

Revision ID: g6h7i8j9k0l1
Revises: f5g6h7i8j9k0
Create Date: 2026-05-02 00:00:00.000000

==============================================================================
m2 collector 重構 PR #6(per blueprint v3.2 r1 §六 #14 B-6)。

新 Bronze 表 business_indicator_tw:景氣指標(月頻)。

降級背景(blueprint §六.1 / §六.2 Q2 答辯):
- v3.1 提案 business_indicator_core,但「不寫入 Core」「僅 Aggregation Layer 比對」
- v3.2 反方審查 Q2 拍板降為 reference 級別 — 不算 Core 但仍持久化(月頻指標
  每次 query FinMind 不適合 20 beta 用戶共享)
- 對應 silver derived 也建(blueprint §6.3 business_indicator_derived,後續 PR)

砍掉 v3.1 提案的:leading_notrend / coincident_notrend / lagging_notrend 三欄
(總經學家用,Beta 不需)。

FinMind dataset:TaiwanBusinessIndicator
==============================================================================
"""
from typing import Sequence, Union

from alembic import op


# revision identifiers, used by Alembic.
revision: str = "g6h7i8j9k0l1"
down_revision: Union[str, Sequence[str], None] = "f5g6h7i8j9k0"
branch_labels: Union[str, Sequence[str], None] = None
depends_on: Union[str, Sequence[str], None] = None


def upgrade() -> None:
    """建 business_indicator_tw 表(月頻,單市場 'tw')。"""
    op.execute(
        """
        CREATE TABLE IF NOT EXISTS business_indicator_tw (
            market              TEXT NOT NULL DEFAULT 'tw',
            date                DATE NOT NULL,           -- 月初
            leading             NUMERIC(10, 4),
            coincident          NUMERIC(10, 4),
            lagging             NUMERIC(10, 4),
            monitoring          INT,                     -- 綜合分數
            monitoring_color    TEXT,                    -- R / YR / G / YB / B
            detail              JSONB,
            PRIMARY KEY (market, date)
        )
        """
    )


def downgrade() -> None:
    """移除 business_indicator_tw 表。"""
    op.execute("DROP TABLE IF EXISTS business_indicator_tw")
