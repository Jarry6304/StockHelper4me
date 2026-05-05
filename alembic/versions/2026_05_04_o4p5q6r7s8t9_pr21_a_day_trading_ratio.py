"""pr21_a_day_trading_ratio

Revision ID: o4p5q6r7s8t9
Revises: n3o4p5q6r7s8
Create Date: 2026-05-04 18:00:00.000000

==============================================================================
PR #21-A — day_trading_derived 加 day_trading_ratio 衍生欄(per spec §7.4)。

day_trading_ratio = (day_trading_buy + day_trading_sell) × 100 / volume
單位 %(NUMERIC(10, 4))。

PR #19a 落 day_trading_derived schema 時把這個衍生欄漏了(其他 4 個衍生欄
gov_bank_net / market_value_weight / total_*_balance / SBL 6 都有先放佔位
column,只是 builder 寫 NULL)。本 migration 補加 column,builder 同 PR
填 ratio 邏輯。

對齊 chip_cores.md §7.4 DayTradingPoint.day_trade_ratio。
==============================================================================
"""

from typing import Sequence, Union

from alembic import op


revision: str = "o4p5q6r7s8t9"
down_revision: Union[str, Sequence[str], None] = "n3o4p5q6r7s8"
branch_labels: Union[str, Sequence[str], None] = None
depends_on: Union[str, Sequence[str], None] = None


def upgrade() -> None:
    op.execute(
        "ALTER TABLE day_trading_derived "
        "ADD COLUMN IF NOT EXISTS day_trading_ratio NUMERIC(10, 4)"
    )


def downgrade() -> None:
    op.execute(
        "ALTER TABLE day_trading_derived DROP COLUMN IF EXISTS day_trading_ratio"
    )
