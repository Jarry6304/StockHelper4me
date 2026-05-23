"""Interval-forecast spine: forecast_log table (v0.3 spec, Phase 1)

對齊 user v0.3 區間預測 spine spec(2026-05-23 session)+ plan 文件
`/root/.claude/plans/stockhelper4me-serene-thacker.md` Phase 1。

`forecast_log` 為機械軌 backtest 與裁量軌 forward log 共用 sink,與 `facts` 表
並列(同 PG schema,職責不同)。一個 source_core 對同一 (stock, T, horizon)
只能有一筆;重跑經 ON CONFLICT UPDATE 覆寫。

`chk_calibrated_or_unsigned` 鎖住「source_core 不在 known-uncalibrated 名單
(baseline / log_channel / fib / manual / kalman_raw / neely_fib /
kalman_forecast_core)時,calibrated 必為 TRUE」— 擋未來新增 core 忘記宣告
校準狀態(spec rule:未校準者不得宣稱覆蓋率)。

Revision ID: a7b8c9d0e1f2
Revises: f1g2h3i4j5k6
Create Date: 2026-05-23
"""

from alembic import op


revision = 'a7b8c9d0e1f2'
down_revision = 'f1g2h3i4j5k6'
branch_labels = None
depends_on = None


def upgrade() -> None:
    op.execute(
        """
        CREATE TABLE IF NOT EXISTS forecast_log (
            id              BIGSERIAL PRIMARY KEY,
            stock_id        TEXT NOT NULL,
            forecast_date   DATE NOT NULL,
            horizon_days    SMALLINT NOT NULL,
            lower           NUMERIC(15, 4),
            upper           NUMERIC(15, 4),
            point           NUMERIC(15, 4),
            confidence      NUMERIC(5, 4) NOT NULL,
            calibrated      BOOLEAN NOT NULL DEFAULT FALSE,
            source_core     TEXT NOT NULL,
            regime_tag      TEXT,
            params_hash     TEXT,
            resolved_date   DATE,
            realized_price  NUMERIC(15, 4),
            hit             BOOLEAN,
            pinball_loss    NUMERIC(15, 6),
            created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            CONSTRAINT uq_forecast_log_lookup
                UNIQUE (stock_id, forecast_date, horizon_days, source_core),
            CONSTRAINT chk_forecast_confidence
                CHECK (confidence > 0 AND confidence < 1),
            CONSTRAINT chk_forecast_horizon
                CHECK (horizon_days > 0),
            CONSTRAINT chk_forecast_calibrated_or_unsigned CHECK (
                calibrated = TRUE
                OR source_core IN ('baseline', 'log_channel', 'fib', 'manual',
                                   'kalman_raw', 'neely_fib', 'kalman_forecast_core')
            )
        )
        """
    )
    op.execute(
        """
        CREATE INDEX IF NOT EXISTS idx_forecast_log_pending
            ON forecast_log (forecast_date, horizon_days)
            WHERE resolved_date IS NULL
        """
    )
    op.execute(
        """
        CREATE INDEX IF NOT EXISTS idx_forecast_log_scoring
            ON forecast_log (source_core, forecast_date)
            WHERE resolved_date IS NOT NULL
        """
    )


def downgrade() -> None:
    op.execute("DROP TABLE IF EXISTS forecast_log")
