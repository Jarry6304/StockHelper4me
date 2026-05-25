"""wave_impulse_screen_derived — Wave Impulse Cross-Stock Screen

對齊 plan `/root/.claude/plans/wave-impulse-cross-stock-virtual-papert.md` §6 Schema。

cross_cores Phase 8 第 12 個 builder。讀 `structural_snapshots`(neely_core forest)
→ 套既有 picker → 雙軸驗證浪位 → emit cross-stock ranking 表。

Schema 特性(對齊 cross_cores `_BASE_TAIL` 慣例,**PK 加 timeframe**):
- PK (market, stock_id, date, timeframe) — user wedge §Q4 拍版 per-tf 獨立 row
- W2/W3/W4/W5 phase + confidence(strict/loose 雙軸驗證 fallback)
- R/R 計算 + cross_tf_aligned 軟對齊 hint
- emit row 即使 is_candidate=False(W5 observe / W1 too_early / OTHER)

Revision ID: g3h4i5j6k7l8
Revises: f2g3h4i5j6k7
Create Date: 2026-05-26
"""

from alembic import op


revision = 'g3h4i5j6k7l8'
down_revision = 'f2g3h4i5j6k7'
branch_labels = None
depends_on = None


def upgrade() -> None:
    op.execute(
        """
        CREATE TABLE IF NOT EXISTS wave_impulse_screen_derived (
            market             TEXT      NOT NULL,
            stock_id           TEXT      NOT NULL,
            date               DATE      NOT NULL,
            timeframe          TEXT      NOT NULL,
            -- Wave position(雙軸驗證結果)
            phase              TEXT,
            wave_number        INTEGER,
            pattern_kind       TEXT,
            direction          TEXT,
            effective_degree   TEXT,
            structure_label    TEXT,
            confidence_level   TEXT      NOT NULL,
            -- R/R metrics
            entry_price        NUMERIC(18, 4),
            target_price       NUMERIC(18, 4),
            invalidation_price NUMERIC(18, 4),
            rr_ratio           NUMERIC(10, 4),
            -- Cross-TF 軟對齊 hint
            cross_tf_aligned   BOOLEAN   NOT NULL DEFAULT FALSE,
            -- Ranking
            impulse_rank       INTEGER,
            universe_size      INTEGER,
            is_candidate       BOOLEAN   NOT NULL DEFAULT FALSE,
            -- Base tail(對齊 _BASE_TAIL,PK 含 timeframe)
            is_top_n           BOOLEAN   NOT NULL DEFAULT FALSE,
            excluded_reason    TEXT,
            detail             JSONB,
            is_dirty           BOOLEAN   NOT NULL DEFAULT FALSE,
            dirty_at           TIMESTAMPTZ,
            PRIMARY KEY (market, stock_id, date, timeframe)
        )
        """
    )
    op.execute(
        """
        CREATE INDEX IF NOT EXISTS idx_wave_impulse_top
            ON wave_impulse_screen_derived (market, date, timeframe, impulse_rank)
            WHERE is_top_n = TRUE
        """
    )


def downgrade() -> None:
    # destructive — 對齊 PR #R6 / v4.17 destructive 先例
    op.execute("DROP INDEX IF EXISTS idx_wave_impulse_top")
    op.execute("DROP TABLE IF EXISTS wave_impulse_screen_derived")
