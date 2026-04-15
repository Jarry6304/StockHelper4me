"""
sync_tracker.py
----------------
斷點續傳追蹤模組。

以 api_sync_progress 資料表記錄每個 (api_name, stock_id, segment_start)
的執行狀態，實現 backfill 的精細斷點續傳。

Status 說明：
  pending         尚未開始
  completed       成功入庫
  failed          失敗（含錯誤訊息）
  empty           API 回傳空資料（正常，某些股票無此類事件）
  schema_mismatch API 回傳欄位與預期不符，已入庫但需人工檢查
"""

import logging
from datetime import date, datetime

from db import DBWriter

logger = logging.getLogger("collector.sync_tracker")

# 可跳過的狀態（completed 或 empty 不需重新執行）
SKIP_STATUSES = {"completed", "empty"}


class SyncTracker:
    """
    API 層級斷點續傳追蹤器。

    以 (api_name, stock_id, segment_start) 三元組為主鍵追蹤進度。

    使用方式：
        tracker = SyncTracker(db)
        if tracker.is_completed(api_name, stock_id, seg_start):
            continue
        # ... 執行任務 ...
        tracker.mark_progress(api_name, stock_id, seg_start, seg_end, "completed", count)
    """

    def __init__(self, db: DBWriter):
        """
        Args:
            db: DBWriter 連線（api_sync_progress 表應已初始化）
        """
        self.db = db

    # =========================================================================
    # 查詢
    # =========================================================================

    def is_completed(
        self,
        api_name: str,
        stock_id: str,
        segment_start: str,
    ) -> bool:
        """
        判斷指定段落是否已完成（completed 或 empty），可安全跳過。

        Args:
            api_name:      API 名稱（collector.toml 中的 name 欄位）
            stock_id:      股票代碼
            segment_start: 日期段起始（YYYY-MM-DD）

        Returns:
            True 表示已完成，可跳過；False 表示需要執行
        """
        row = self.db.query_one(
            """
            SELECT status FROM api_sync_progress
            WHERE api_name = ? AND stock_id = ? AND segment_start = ?
            """,
            [api_name, stock_id, segment_start],
        )
        if row is None:
            return False
        return row["status"] in SKIP_STATUSES

    def get_last_sync(self, api_name: str, stock_id: str) -> date | None:
        """
        取得指定 API + 股票的最後成功同步日期（segment_end）。
        用於 incremental 模式計算起始日期。

        Args:
            api_name: API 名稱
            stock_id: 股票代碼

        Returns:
            最後成功 segment 的結束日期；無記錄時回傳 None
        """
        row = self.db.query_one(
            """
            SELECT MAX(segment_end) AS last_end
            FROM api_sync_progress
            WHERE api_name = ? AND stock_id = ?
              AND status IN ('completed', 'empty')
            """,
            [api_name, stock_id],
        )
        if row is None or row["last_end"] is None:
            return None

        return date.fromisoformat(row["last_end"])

    # =========================================================================
    # 更新
    # =========================================================================

    def mark_progress(
        self,
        api_name: str,
        stock_id: str,
        segment_start: str,
        segment_end: str,
        status: str = "completed",
        record_count: int = 0,
        error_message: str | None = None,
    ) -> None:
        """
        更新或插入 api_sync_progress 進度記錄。

        Args:
            api_name:      API 名稱
            stock_id:      股票代碼
            segment_start: 日期段起始（YYYY-MM-DD）
            segment_end:   日期段結束（YYYY-MM-DD）
            status:        執行狀態（pending/completed/failed/empty/schema_mismatch）
            record_count:  本段入庫筆數
            error_message: 錯誤訊息（status=failed 時填入）
        """
        self.db.upsert(
            "api_sync_progress",
            [{
                "api_name":      api_name,
                "stock_id":      stock_id,
                "segment_start": segment_start,
                "segment_end":   segment_end,
                "status":        status,
                "record_count":  record_count,
                "error_message": error_message,
                # 使用 Python 端時間字串，避免 SQLite 字串字面值問題
                "updated_at":    datetime.now().isoformat(timespec="seconds"),
            }],
        )

        if status == "failed":
            logger.warning(
                f"Progress: api={api_name}, stock={stock_id}, "
                f"segment={segment_start}~{segment_end}, "
                f"status={status}, error={error_message}"
            )
        else:
            logger.info(
                f"Progress: api={api_name}, stock={stock_id}, "
                f"segment={segment_start}~{segment_end}, "
                f"status={status}, records={record_count}"
            )

    def mark_failed(
        self,
        api_name: str,
        stock_id: str,
        segment_start: str,
        segment_end: str,
        error_message: str,
    ) -> None:
        """
        標記指定段落為失敗。

        Args:
            api_name:      API 名稱
            stock_id:      股票代碼
            segment_start: 日期段起始
            segment_end:   日期段結束
            error_message: 錯誤訊息
        """
        self.mark_progress(
            api_name, stock_id, segment_start, segment_end,
            status="failed",
            error_message=error_message,
        )

    def mark_schema_mismatch(
        self,
        api_name: str,
        stock_id: str,
        segment_start: str,
        segment_end: str,
        record_count: int = 0,
    ) -> None:
        """
        標記指定段落為 schema 欄位不符（已入庫但需人工確認）。

        Args:
            api_name:      API 名稱
            stock_id:      股票代碼
            segment_start: 日期段起始
            segment_end:   日期段結束
            record_count:  入庫筆數
        """
        self.mark_progress(
            api_name, stock_id, segment_start, segment_end,
            status="schema_mismatch",
            record_count=record_count,
            error_message="API 回傳欄位與 field_rename 定義不符，請確認欄位映射",
        )

    # =========================================================================
    # 進度摘要
    # =========================================================================

    def summary(self) -> dict[str, int]:
        """
        回傳各 status 的累計筆數，用於 CLI status 指令。

        Returns:
            {"completed": N, "failed": N, "pending": N, ...}
        """
        rows = self.db.query(
            """
            SELECT status, COUNT(*) AS cnt
            FROM api_sync_progress
            GROUP BY status
            """
        )
        return {row["status"]: row["cnt"] for row in rows}

    def update_stock_sync_status(
        self,
        market: str,
        stock_id: str,
        sync_type: str = "full",
    ) -> None:
        """
        更新 stock_sync_status 表的同步時間戳記。

        Args:
            market:    市場代碼（"TW"）
            stock_id:  股票代碼
            sync_type: "full"（全量）| "incr"（增量）
        """
        today = date.today().isoformat()

        if sync_type == "full":
            sql    = """
                INSERT OR REPLACE INTO stock_sync_status
                    (market, stock_id, last_full_sync, fwd_adj_valid)
                VALUES (?, ?, ?, 0)
            """
            params = [market, stock_id, today]
        else:
            sql    = """
                INSERT INTO stock_sync_status (market, stock_id, last_incr_sync)
                VALUES (?, ?, ?)
                ON CONFLICT(market, stock_id)
                DO UPDATE SET last_incr_sync = excluded.last_incr_sync
            """
            params = [market, stock_id, today]

        self.db.update(sql, params)
