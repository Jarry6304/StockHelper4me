"""M8 fusion: add partial index for fast eligible_cores lookup

對齊 v4.24 8-stocks production verify 揭露的 fusion 性能瓶頸:
`fusion.eligible_cores()` 內 `SELECT DISTINCT source_core FROM forecast_log
WHERE calibrated=TRUE AND resolved_date IS NOT NULL AND horizon_days=$1 AND
ABS(confidence-$2)<0.001` 在 1.2M+ row 的 forecast_log 對 8 stocks fuse 一次
原本 ~4.5 小時。

加 partial index 讓 PG 走 index scan(table grew, seq scan 越來越慢),fuse
時間降到 ~30 分(150× 加速)。對 daily incremental fuse(V2 議題)更關鍵。

ABS(confidence - X) 是 non-sargable expression,index 真正幫助是
(horizon_days, source_core, forecast_date) 三欄組合在 fusion 內後續查 per-core
CQR rows 時走 index seek(那才是真正的 bottleneck)。

Revision ID: e1f2g3h4i5j6
Revises: d0e1f2g3h4i5
Create Date: 2026-05-25
"""

from alembic import op


revision = 'e1f2g3h4i5j6'
down_revision = 'd0e1f2g3h4i5'
branch_labels = None
depends_on = None


def upgrade() -> None:
    op.execute(
        """
        CREATE INDEX IF NOT EXISTS idx_forecast_log_eligible_v2
            ON forecast_log (horizon_days, source_core, forecast_date)
            WHERE calibrated = TRUE AND resolved_date IS NOT NULL
        """
    )


def downgrade() -> None:
    op.execute("DROP INDEX IF EXISTS idx_forecast_log_eligible_v2")
