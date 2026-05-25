"""forecast_log.internal_only — B-4 機制丙(雙軌共振決策層)

對齊 m3Spec/dual_track_resonance.md §七「事實層唯一改動」。

加 `internal_only BOOLEAN NOT NULL DEFAULT FALSE` 一欄,讓 forecast_log 區分:
- `internal_only = FALSE`(預設):對外可見,進 settlement / scorer / fusion /
  calibration / UI / MCP / dual_track track2 等所有對外路徑。
- `internal_only = TRUE`:audit / 對齊影子,**禁止上畫面與 MCP 輸出**,只
  internal 對齊用。

首批 internal_only=TRUE:`source_core = 'neely_fib'`(對齊 §六 失真處理 — neely
fib 帶非統計帶,且一行外包絡壓掉了離散 fib 線資訊;dual_track 軌道一直接讀
structural_snapshots 完整資料,forecast_log neely_fib 行降級為對齊影子)。

backfill:既有 `source_core = 'neely_fib'` 全部 UPDATE internal_only=TRUE,讓
schema invariant 立即生效(後續查詢預設過濾 internal_only=FALSE,不會 leak)。

Revision ID: f2g3h4i5j6k7
Revises: e1f2g3h4i5j6
Create Date: 2026-05-25
"""

from alembic import op


revision = 'f2g3h4i5j6k7'
down_revision = 'e1f2g3h4i5j6'
branch_labels = None
depends_on = None


def upgrade() -> None:
    # 1. 加欄(IF NOT EXISTS 對齊 PG 15+;若 PG 14 則需先 drop / 不建議 production
    #    回退到 14)。本機 production 是 PG 17,IF NOT EXISTS 可用。
    op.execute(
        """
        ALTER TABLE forecast_log
            ADD COLUMN IF NOT EXISTS internal_only BOOLEAN NOT NULL DEFAULT FALSE
        """
    )

    # 2. 既有 neely_fib 行 backfill 為 internal_only=TRUE
    #    對齊 §七「emit `neely_fib` 時標 True」— 既有資料一次性對齊
    op.execute(
        """
        UPDATE forecast_log
           SET internal_only = TRUE
         WHERE source_core = 'neely_fib'
           AND internal_only = FALSE
        """
    )

    # 3. 加 partial index 加速「對外查詢」常見路徑(over 99% 查詢走這條)
    #    既有 idx_forecast_log_eligible_v2 已含 calibrated=TRUE 過濾,本 index
    #    補上「calibrated=FALSE 但非 internal」對外路徑(e.g. baseline / kalman_raw)
    op.execute(
        """
        CREATE INDEX IF NOT EXISTS idx_forecast_log_external
            ON forecast_log (stock_id, forecast_date, horizon_days, source_core)
            WHERE internal_only = FALSE
        """
    )


def downgrade() -> None:
    # destructive — 對齊 PR #R6 / v4.17 destructive 先例
    op.execute("DROP INDEX IF EXISTS idx_forecast_log_external")
    op.execute("ALTER TABLE forecast_log DROP COLUMN IF EXISTS internal_only")
