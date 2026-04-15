"""
date_segmenter.py
------------------
歷史回補日期分段模組。

職責：依 api_config.segment_days 與執行模式，
回傳要拉取的 (start_date, end_date) 日期段列表。

分段策略：
  segment_days = 0   → 單段 [(backfill_start, today)]
  segment_days = N   → 按 N 天切段（如 365 表示每次拉一年）
  incremental 模式   → 單段 [(last_sync+1, today)]
"""

import logging
from datetime import date, timedelta

from config_loader import ApiConfig, CollectorConfig
from sync_tracker import SyncTracker

logger = logging.getLogger("collector.date_segmenter")


class DateSegmenter:
    """
    日期分段計算器。

    使用方式：
        segmenter = DateSegmenter(config, sync_tracker)
        segs = segmenter.segments(api_config, "backfill", "2330")
        # 回傳 [("2019-01-01", "2019-12-31"), ("2020-01-01", "2020-12-31"), ...]
    """

    def __init__(self, config: CollectorConfig, sync_tracker: SyncTracker | None = None):
        """
        Args:
            config:       整合後的 Collector 設定（用於取得 backfill_start_date）
            sync_tracker: 斷點續傳追蹤器（incremental 模式需要）
        """
        self.config       = config
        self.sync_tracker = sync_tracker

    def segments(
        self,
        api_config: ApiConfig,
        mode: str,
        stock_id: str,
    ) -> list[tuple[str, str]]:
        """
        計算並回傳 (start_date, end_date) 日期段列表。

        Args:
            api_config: API 設定（含 segment_days）
            mode:       "backfill" | "incremental"
            stock_id:   股票代碼（incremental 模式需要查上次同步日期）

        Returns:
            [(start_date, end_date), ...] 格式的日期段列表
            日期格式：YYYY-MM-DD 字串
        """
        today = date.today()

        # ── incremental 模式：從上次同步後一天起算，拉到今天
        if mode == "incremental":
            last_sync = None
            if self.sync_tracker:
                last_sync = self.sync_tracker.get_last_sync(api_config.name, stock_id)

            if last_sync:
                start = last_sync + timedelta(days=1)
            else:
                # 無同步記錄 → 從設定的 backfill 起始日開始
                start = date.fromisoformat(self.config.global_cfg.backfill_start_date)

            return [(start.isoformat(), today.isoformat())]

        # ── backfill 模式
        backfill_start = date.fromisoformat(
            self.config.execution.start_date or self.config.global_cfg.backfill_start_date
        )

        # segment_days = 0：不分段，一次拉全部
        if api_config.segment_days == 0:
            return [(backfill_start.isoformat(), today.isoformat())]

        # segment_days = N：按 N 天切段
        return self._split_segments(backfill_start, today, api_config.segment_days)

    @staticmethod
    def _split_segments(
        start: date,
        end: date,
        segment_days: int,
    ) -> list[tuple[str, str]]:
        """
        將 [start, end] 日期範圍切分為每段最多 segment_days 天的列表。

        例如 start=2019-01-01, end=2026-12-31, segment_days=365：
          → [("2019-01-01", "2019-12-31"),
             ("2020-01-01", "2020-12-31"),
             ...,
             ("2026-01-01", "2026-12-31")]

        Args:
            start:        起始日期
            end:          結束日期（含）
            segment_days: 每段最大天數

        Returns:
            [(start_str, end_str), ...] 日期段列表
        """
        segments: list[tuple[str, str]] = []
        cursor = start

        while cursor <= end:
            seg_end = min(cursor + timedelta(days=segment_days - 1), end)
            segments.append((cursor.isoformat(), seg_end.isoformat()))
            cursor = seg_end + timedelta(days=1)

        return segments
