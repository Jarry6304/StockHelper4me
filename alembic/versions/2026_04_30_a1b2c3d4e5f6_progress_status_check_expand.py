"""progress_status_check_expand

Revision ID: a1b2c3d4e5f6
Revises: 0da6e52171b1
Create Date: 2026-04-30 00:00:00.000000

==============================================================================
擴充 api_sync_progress.chk_progress_status，補上 'empty' 與 'schema_mismatch'。

背景：
    baseline (v2.0) 的 CHECK constraint 只允許 ('pending', 'completed', 'failed')，
    但 src/sync_tracker.py 與 src/phase_executor.py 實際會寫入兩種額外狀態：
      - empty           ：API 回空陣列（很多 dataset/股票 沒有資料時的正常結果）
      - schema_mismatch ：FieldMapper 偵測到 API 回傳欄位與 field_rename 不符

    沒擴充 CHECK 的話，凡是 API 回空（dividend_result 對沒發過股利的股票、
    incremental 已無新資料等）的 segment upsert 都會被 PG 拒絕，
    導致進度永遠標不起來、斷點續傳卡死。

修法：
    DROP 舊 CHECK，重建涵蓋 5 種 status 的 CHECK。
==============================================================================
"""
from typing import Sequence, Union

from alembic import op


# revision identifiers, used by Alembic.
revision: str = "a1b2c3d4e5f6"
down_revision: Union[str, Sequence[str], None] = "0da6e52171b1"
branch_labels: Union[str, Sequence[str], None] = None
depends_on: Union[str, Sequence[str], None] = None


def upgrade() -> None:
    """擴充 chk_progress_status 至 5 種 status。"""
    op.execute("ALTER TABLE api_sync_progress DROP CONSTRAINT IF EXISTS chk_progress_status")
    op.execute(
        "ALTER TABLE api_sync_progress "
        "ADD CONSTRAINT chk_progress_status CHECK ("
        "status IN ('pending', 'completed', 'failed', 'empty', 'schema_mismatch')"
        ")"
    )


def downgrade() -> None:
    """退回 baseline 的 3 種 status。

    ⚠️ 若資料表已有 status='empty' 或 'schema_mismatch' 的 row，downgrade 會失敗
    （CHECK 加回去當下會驗證所有現存資料）。生產環境執行 downgrade 前須先：
      DELETE FROM api_sync_progress WHERE status IN ('empty', 'schema_mismatch');
    """
    op.execute("ALTER TABLE api_sync_progress DROP CONSTRAINT IF EXISTS chk_progress_status")
    op.execute(
        "ALTER TABLE api_sync_progress "
        "ADD CONSTRAINT chk_progress_status CHECK ("
        "status IN ('pending', 'completed', 'failed')"
        ")"
    )
