"""M4: forecast_log whitelist 加 'judgment'(wave_judgment_loop §8 S3)

forward log 切到 judgment:`neely_emitter.emit_judgment_forecast` 依 active
judgment 的 accepted[preferred] 候選發 `source_core='judgment'` 列(uncalibrated
→ 需入 whitelist);舊 `neely_fib`(picker 序列)凍結唯讀 — 值留在 whitelist
(歷史列合法),寫路徑不再被呼叫。

CHECK constraint:DROP + RE-ADD(同 d0e1f2g3h4i5 先例;PG 無法 mutate IN 列表)。
`src/schema_pg.sql` 同步。

Revision ID: l8m9n0o1p2q3
Revises: k7l8m9n0o1p2
Create Date: 2026-08-31
"""

from alembic import op

revision = 'l8m9n0o1p2q3'
down_revision = 'k7l8m9n0o1p2'
branch_labels = None
depends_on = None


_UNCALIBRATED_V3 = (
    "'baseline', 'log_channel', 'fib', 'manual', "
    "'kalman_raw', 'neely_fib', 'kalman_forecast_core', "
    "'chip_forecast_core', 'macro_forecast_core', 'fundamental_forecast_core', "
    "'judgment'"
)

_UNCALIBRATED_V2 = (
    "'baseline', 'log_channel', 'fib', 'manual', "
    "'kalman_raw', 'neely_fib', 'kalman_forecast_core', "
    "'chip_forecast_core', 'macro_forecast_core', 'fundamental_forecast_core'"
)


def upgrade() -> None:
    op.execute(
        "ALTER TABLE forecast_log "
        "DROP CONSTRAINT IF EXISTS chk_forecast_calibrated_or_unsigned"
    )
    op.execute(
        f"ALTER TABLE forecast_log "
        f"ADD CONSTRAINT chk_forecast_calibrated_or_unsigned CHECK ("
        f"  calibrated = TRUE OR source_core IN ({_UNCALIBRATED_V3})"
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
        f"  calibrated = TRUE OR source_core IN ({_UNCALIBRATED_V2})"
        f")"
    )
