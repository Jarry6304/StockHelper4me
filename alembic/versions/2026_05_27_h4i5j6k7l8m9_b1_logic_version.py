"""B1:forecast_log.logic_version — backtest segmentation

對齊 b1-degree-consolidation skill 塊 4:讓 X/Y 決策證據不被 logic 斷點污染。

新增 `logic_version TEXT NOT NULL DEFAULT 'pre_b1'` 欄:
- DEFAULT 'pre_b1' 讓既有 row(本 migration upgrade 時)自動 backfill 為 'pre_b1'
- 新寫入由 src/forecast/_db.upsert_forecast 帶 'b1'(預設)/ caller 可覆寫
- ON CONFLICT SET 子句對 logic_version 加 CASE guard:已 settle row(resolved_date
  IS NOT NULL)永不被覆寫 — backtest 證據在 settle 時凍結

**不進 ON CONFLICT 唯一鍵**:維持既有 5-tuple
`(stock_id, forecast_date, horizon_days, source_core, confidence)`。

Revision ID: h4i5j6k7l8m9
Revises: g3h4i5j6k7l8
Create Date: 2026-05-27
"""

from alembic import op


revision = 'h4i5j6k7l8m9'
down_revision = 'g3h4i5j6k7l8'
branch_labels = None
depends_on = None


def upgrade() -> None:
    # 1. 加 logic_version 欄,server_default='pre_b1' 讓既有 row 自動 backfill
    op.execute(
        """
        ALTER TABLE forecast_log
            ADD COLUMN IF NOT EXISTS logic_version TEXT NOT NULL DEFAULT 'pre_b1'
        """
    )

    # 2. 移除 server_default,讓新寫入必須 explicit 帶 logic_version
    #    (upsert_forecast 預設帶 'b1';caller 可覆寫)
    op.execute(
        """
        ALTER TABLE forecast_log
            ALTER COLUMN logic_version DROP DEFAULT
        """
    )


def downgrade() -> None:
    # destructive — 對齊 PR #R6 / v4.17 destructive 先例
    op.execute("ALTER TABLE forecast_log DROP COLUMN IF EXISTS logic_version")
