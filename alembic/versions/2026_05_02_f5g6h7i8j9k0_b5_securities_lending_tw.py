"""b5_securities_lending_tw

Revision ID: f5g6h7i8j9k0
Revises: e4f5g6h7i8j9
Create Date: 2026-05-02 00:00:00.000000

==============================================================================
m2 collector 重構 PR #5(per blueprint v3.2 r1 §六 #13 B-5)。

新 Bronze 表 securities_lending_tw:借券成交明細(SBL)。

用途:
- margin_core 借券關鍵欄位(blueprint §2.6.1 加 SBL 6 欄到 margin_daily_derived)
- 個股賣空壓力分析(實戰用例:3363 上詮 4/27 限跌停案例,SBL 借券賣出
  量 = 拉積盤效應實證指標)

FinMind dataset:TaiwanStockSecuritiesLending

Schema(blueprint §附錄 B):
- PK (market, stock_id, date, transaction_type, fee_rate)
  → 同股同日有「議借」+「競價」兩種 transaction_type,各自有不同 fee_rate
==============================================================================
"""
from typing import Sequence, Union

from alembic import op


# revision identifiers, used by Alembic.
revision: str = "f5g6h7i8j9k0"
down_revision: Union[str, Sequence[str], None] = "e4f5g6h7i8j9"
branch_labels: Union[str, Sequence[str], None] = None
depends_on: Union[str, Sequence[str], None] = None


def upgrade() -> None:
    """建 securities_lending_tw 表。"""
    op.execute(
        """
        CREATE TABLE IF NOT EXISTS securities_lending_tw (
            market                  TEXT NOT NULL,
            stock_id                TEXT NOT NULL,
            date                    DATE NOT NULL,
            transaction_type        TEXT NOT NULL,        -- 議借 / 競價
            volume                  BIGINT,
            fee_rate                NUMERIC(8, 4),
            close                   NUMERIC(15, 4),
            original_return_date    DATE,
            original_lending_period INT,
            detail                  JSONB,
            PRIMARY KEY (market, stock_id, date, transaction_type, fee_rate)
        )
        """
    )
    # margin_core 常見查詢「某股某日聚合借券資料」
    op.execute(
        "CREATE INDEX IF NOT EXISTS idx_sbl_stock_date "
        "ON securities_lending_tw(market, stock_id, date DESC)"
    )


def downgrade() -> None:
    """移除 securities_lending_tw 表。"""
    op.execute("DROP INDEX IF EXISTS idx_sbl_stock_date")
    op.execute("DROP TABLE IF EXISTS securities_lending_tw")
