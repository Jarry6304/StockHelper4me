"""M8: extend forecast_log uncalibrated whitelist for 3 non-price cores

對齊 CLAUDE.md v4.23(2026-05-24)結論:既有 3 cores(baseline / log_channel /
kalman_forecast_core)全部 price-only,fusion 因誤差高度相關(違反 Bates-Granger
1969 前提)無法獲得變異數縮減。本 migration 為 M8 sprint 第一個 commit,
擴充 `chk_forecast_calibrated_or_unsigned` whitelist 收容 3 個新 non-price
forecast core:

- `chip_forecast_core`        institutional flow / margin / loan collateral
- `macro_forecast_core`       FX / business indicator
- `fundamental_forecast_core` monthly revenue / financial statement

CHECK constraint:DROP + RE-ADD(PG 14+ 沒 ALTER CONSTRAINT 可 mutate IN 列表)。

Revision ID: d0e1f2g3h4i5
Revises: c9d0e1f2g3h4
Create Date: 2026-05-24
"""

from alembic import op


revision = 'd0e1f2g3h4i5'
down_revision = 'c9d0e1f2g3h4'
branch_labels = None
depends_on = None


_UNCALIBRATED_V2 = (
    "'baseline', 'log_channel', 'fib', 'manual', "
    "'kalman_raw', 'neely_fib', 'kalman_forecast_core', "
    "'chip_forecast_core', 'macro_forecast_core', 'fundamental_forecast_core'"
)

_UNCALIBRATED_V1 = (
    "'baseline', 'log_channel', 'fib', 'manual', "
    "'kalman_raw', 'neely_fib', 'kalman_forecast_core'"
)


def upgrade() -> None:
    op.execute(
        "ALTER TABLE forecast_log "
        "DROP CONSTRAINT IF EXISTS chk_forecast_calibrated_or_unsigned"
    )
    op.execute(
        f"ALTER TABLE forecast_log "
        f"ADD CONSTRAINT chk_forecast_calibrated_or_unsigned CHECK ("
        f"  calibrated = TRUE OR source_core IN ({_UNCALIBRATED_V2})"
        f")"
    )


def downgrade() -> None:
    op.execute(
        "ALTER TABLE forecast_log "
        "DROP CONSTRAINT IF EXISTS chk_forecast_calibrated_or_unsigned"
    )
    op.execute(
        f"ALTER TABLE forecast_log "
        f"ADD CONSTRAINT chk_forecast_calibrated_or_unsigned CHECK ("
        f"  calibrated = TRUE OR source_core IN ({_UNCALIBRATED_V1})"
        f")"
    )
