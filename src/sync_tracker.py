"""
sync_tracker.py
----------------
斷點續傳追蹤模組(v3.3 升 preload cache)。

以 api_sync_progress 資料表記錄每個 (api_name, stock_id, segment_start)
的執行狀態,實現 backfill 的精細斷點續傳。

v3.3 改動(對齊 plan §規格 3):
- `__init__` 預先從 DB 撈所有 status IN ('completed', 'empty') 的 row →
  in-memory set
- `is_completed` 改為 O(1) tuple key 查詢,不再每次 DB query
- 對 20+ 萬次 hot loop(全市場 backfill 跑 backfill --phases 1-6 時跑得到)
  從 O(N) DB I/O 降到 O(1) memory check
- `mark_progress` 寫 DB 後同步更新 cache

Status 說明:
  pending         尚未開始
  completed       成功入庫
  failed          失敗(含錯誤訊息)
  empty           API 回傳空資料(正常,某些股票無此類事件)
  schema_mismatch API 回傳欄位與預期不符,已入庫但需人工檢查
"""

import logging
from datetime import date, datetime

from db import DBWriter

logger = logging.getLogger("collector.sync_tracker")

# 可跳過的狀態(completed 或 empty 不需重新執行)
SKIP_STATUSES = frozenset({"completed", "empty"})


class SyncTracker:
    """
    API 層級斷點續傳追蹤器。

    以 (api_name, stock_id, segment_start) 三元組為主鍵追蹤進度。

    使用方式:
        tracker = SyncTracker(db)
        if tracker.is_completed(api_name, stock_id, seg_start):
            continue
        # ... 執行任務 ...
        tracker.mark_progress(api_name, stock_id, seg_start, seg_end, "completed", count)
    """

    def __init__(self, db: DBWriter):
        """
        Args:
            db: DBWriter 連線(api_sync_progress 表應已初始化)

        v3.3:啟動時 preload 所有 SKIP_STATUSES row 到 in-memory set。
        對全量 backfill 後 ~340K rows(api × stock × segments)記憶體 ~50 MB
        可接受,換來 hot loop 不打 DB。

        Defensive:preload 失敗(例如 schema 還沒落地)不 abort,fallback
        到 DB query path(舊行為)。
        """
        self.db = db
        self._cache: set[tuple[str, str, date]] = set()
        self._preload_ok = False

        try:
            rows = self.db.query(
                """
                SELECT api_name, stock_id, segment_start
                  FROM api_sync_progress
                 WHERE status IN ('completed', 'empty')
                """
            )
            self._cache = {
                (r["api_name"], r["stock_id"], self._norm_date(r["segment_start"]))
                for r in rows
            }
            self._preload_ok = True
            logger.info(f"SyncTracker preload: {len(self._cache)} completed/empty segments")
        except Exception as e:
            logger.warning(
                f"SyncTracker preload 失敗,fallback 到 DB query path: {e}"
            )
            self._cache.clear()
            self._preload_ok = False

    @staticmethod
    def _norm_date(value) -> date:
        """統一 segment_start 為 date 物件(支援 date / datetime / ISO 字串)。"""
        if isinstance(value, datetime):
            return value.date()
        if isinstance(value, date):
            return value
        if isinstance(value, str):
            return date.fromisoformat(value.split(" ")[0].split("T")[0])
        raise TypeError(f"_norm_date: unsupported type {type(value)} value={value!r}")

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
        判斷指定段落是否已完成(completed 或 empty),可安全跳過。

        v3.3:走 in-memory cache O(1) check;preload 失敗時 fallback 到 DB query。

        Args:
            api_name:      API 名稱(collector.toml 中的 name 欄位)
            stock_id:      股票代碼
            segment_start: 日期段起始(YYYY-MM-DD)

        Returns:
            True 表示已完成,可跳過;False 表示需要執行
        """
        if self._preload_ok:
            key = (api_name, stock_id, self._norm_date(segment_start))
            return key in self._cache

        # Fallback:preload 失敗 → 走 DB query
        row = self.db.query_one(
            """
            SELECT status FROM api_sync_progress
            WHERE api_name = %s AND stock_id = %s AND segment_start = %s
            """,
            [api_name, stock_id, segment_start],
        )
        if row is None:
            return False
        return row["status"] in SKIP_STATUSES

    def get_last_sync(self, api_name: str, stock_id: str) -> date | None:
        """
        取得指定 API + 股票的最後成功同步日期(segment_end)。
        用於 incremental 模式計算起始日期。

        Args:
            api_name: API 名稱
            stock_id: 股票代碼

        Returns:
            最後成功 segment 的結束日期;無記錄時回傳 None
        """
        row = self.db.query_one(
            """
            SELECT MAX(segment_end) AS last_end
            FROM api_sync_progress
            WHERE api_name = %s AND stock_id = %s
              AND status IN ('completed', 'empty')
            """,
            [api_name, stock_id],
        )
        if row is None or row["last_end"] is None:
            return None

        last_end = row["last_end"]
        if isinstance(last_end, date):
            return last_end
        return date.fromisoformat(last_end)

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

        v3.3:DB 寫入成功後同步更新 in-memory cache(若 status 在 SKIP_STATUSES)。
        失敗時 cache 不更新,確保 cache 與 DB 一致(寫入異常 caller 會自行處理)。

        Args:
            api_name:      API 名稱
            stock_id:      股票代碼
            segment_start: 日期段起始(YYYY-MM-DD)
            segment_end:   日期段結束(YYYY-MM-DD)
            status:        執行狀態(pending/completed/failed/empty/schema_mismatch)
            record_count:  本段入庫筆數
            error_message: 錯誤訊息(status=failed 時填入)
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
                "updated_at":    datetime.now().isoformat(timespec="seconds"),
            }],
            primary_keys=self.db._table_pks("api_sync_progress"),
        )

        # v3.3:DB 寫入成功才更新 cache。preload 失敗時 cache 為空 set,
        # 添加無害(下次 is_completed fallback 路徑仍走 DB)。
        if self._preload_ok and status in SKIP_STATUSES:
            self._cache.add(
                (api_name, stock_id, self._norm_date(segment_start))
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
        """標記指定段落為失敗。"""
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
        """標記指定段落為 schema 欄位不符(已入庫但需人工確認)。"""
        self.mark_progress(
            api_name, stock_id, segment_start, segment_end,
            status="schema_mismatch",
            record_count=record_count,
            error_message="API 回傳欄位與 field_rename 定義不符,請確認欄位映射",
        )

    # =========================================================================
    # 進度摘要
    # =========================================================================

    def summary(self) -> dict[str, int]:
        """回傳各 status 的累計筆數,用於 CLI status 指令。"""
        rows = self.db.query(
            """
            SELECT status, COUNT(*) AS cnt
            FROM api_sync_progress
            GROUP BY status
            """
        )
        return {row["status"]: row["cnt"] for row in rows}
