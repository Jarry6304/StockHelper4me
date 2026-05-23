"""Add confidence to forecast_log UNIQUE key (v0.3 spine hotfix)

User production verify(2026-05-23)揭露 batch INSERT 噴錯:
    ON CONFLICT DO UPDATE command cannot affect row a second time

Root cause:原 UNIQUE key `(stock_id, forecast_date, horizon_days, source_core)`
沒含 confidence。Kalman 對同 (stock, T, horizon) 輸出 3 個不同 confidence
(0.50/0.80/0.95)的 row,batch INSERT 時 3 行共享同 UNIQUE key → PG 拒。
Python upsert per-row 路徑也有問題(三 row 互相覆蓋,silent overwrite)。

設計拍版:三個 confidence 是「不同 width / 不同 coverage 宣稱」的不同預測,
本來就該各自占一 row。confidence 進 UNIQUE key 是 spec 原意忽略的細節。

v0.3 spec 原文 (本 conversation):
    `confidence` numeric  - 宣稱覆蓋率 0–1
    唯一鍵:`(stock_id, forecast_date, horizon_days, source_core)`

修法:加 confidence 到 UNIQUE key,變 5-tuple。

Revision ID: c9d0e1f2g3h4
Revises: b8c9d0e1f2g3
Create Date: 2026-05-23
"""

from alembic import op


revision = 'c9d0e1f2g3h4'
down_revision = 'b8c9d0e1f2g3'
branch_labels = None
depends_on = None


def upgrade() -> None:
    # Drop old UNIQUE constraint
    op.execute("ALTER TABLE forecast_log DROP CONSTRAINT IF EXISTS uq_forecast_log_lookup")

    # Add new UNIQUE constraint with confidence
    op.execute(
        """
        ALTER TABLE forecast_log
            ADD CONSTRAINT uq_forecast_log_lookup
            UNIQUE (stock_id, forecast_date, horizon_days, source_core, confidence)
        """
    )


def downgrade() -> None:
    op.execute("ALTER TABLE forecast_log DROP CONSTRAINT IF EXISTS uq_forecast_log_lookup")
    op.execute(
        """
        ALTER TABLE forecast_log
            ADD CONSTRAINT uq_forecast_log_lookup
            UNIQUE (stock_id, forecast_date, horizon_days, source_core)
        """
    )
