"""Tests for v3.3 SyncTracker preload cache。

對齊 plan §規格 3:
- __init__ 預先撈 status IN ('completed', 'empty') 進 in-memory set
- is_completed 走 O(1) cache check
- mark_progress 寫 DB 後同步更新 cache
- preload 失敗時 fallback 到 DB query path
"""

from __future__ import annotations

import sys
from datetime import date
from pathlib import Path
from unittest.mock import MagicMock

_REPO_ROOT = Path(__file__).resolve().parent.parent
_SRC_ROOT = _REPO_ROOT / "src"
for p in (str(_SRC_ROOT), str(_REPO_ROOT)):
    if p not in sys.path:
        sys.path.insert(0, p)


class TestSyncTrackerPreload:
    """v3.3:啟動時 preload + is_completed O(1) check。"""

    def test_preload_loads_completed_segments(self):
        from sync_tracker import SyncTracker

        db = MagicMock()
        db.query.return_value = [
            {"api_name": "price_daily", "stock_id": "2330", "segment_start": date(2024, 1, 1)},
            {"api_name": "price_daily", "stock_id": "2330", "segment_start": date(2024, 4, 1)},
            {"api_name": "monthly_revenue_v3", "stock_id": "2317", "segment_start": date(2020, 1, 1)},
        ]

        tracker = SyncTracker(db)
        assert tracker._preload_ok is True
        assert len(tracker._cache) == 3

        # is_completed 走 cache,不打 DB
        db.query_one.reset_mock()
        assert tracker.is_completed("price_daily", "2330", "2024-01-01") is True
        assert tracker.is_completed("price_daily", "2330", "2024-01-02") is False
        assert tracker.is_completed("price_daily", "2317", "2024-01-01") is False
        # is_completed 0 個 DB call(走 cache)
        db.query_one.assert_not_called()

    def test_preload_failure_fallback_to_db(self):
        """preload 失敗(例:schema 未落)→ fallback DB query path。"""
        from sync_tracker import SyncTracker

        db = MagicMock()
        db.query.side_effect = Exception("relation api_sync_progress does not exist")
        db.query_one.return_value = {"status": "completed"}

        tracker = SyncTracker(db)
        assert tracker._preload_ok is False
        # is_completed 應 fallback 走 db.query_one
        assert tracker.is_completed("price_daily", "2330", "2024-01-01") is True
        db.query_one.assert_called_once()

    def test_mark_progress_updates_cache(self):
        """mark_progress 寫 DB 成功後同步更新 cache。"""
        from sync_tracker import SyncTracker

        db = MagicMock()
        db.query.return_value = []  # preload 空
        db._table_pks.return_value = ["api_name", "stock_id", "segment_start"]

        tracker = SyncTracker(db)
        assert tracker.is_completed("price_daily", "2330", "2024-01-01") is False

        tracker.mark_progress(
            "price_daily", "2330", "2024-01-01", "2024-03-31",
            status="completed", record_count=100,
        )
        # 寫完 cache 應立刻 visible(不需重新 reload)
        assert tracker.is_completed("price_daily", "2330", "2024-01-01") is True

    def test_mark_failed_does_not_add_to_cache(self):
        """status=failed 不該加進 cache(SKIP_STATUSES 不含 failed)。"""
        from sync_tracker import SyncTracker

        db = MagicMock()
        db.query.return_value = []
        db._table_pks.return_value = ["api_name", "stock_id", "segment_start"]

        tracker = SyncTracker(db)
        tracker.mark_failed("price_daily", "2330", "2024-01-01", "2024-03-31", "boom")
        assert tracker.is_completed("price_daily", "2330", "2024-01-01") is False

    def test_norm_date_accepts_str_and_date(self):
        """_norm_date 對 str / date / datetime 都正常 normalize。"""
        from datetime import datetime
        from sync_tracker import SyncTracker

        assert SyncTracker._norm_date("2024-01-01") == date(2024, 1, 1)
        assert SyncTracker._norm_date(date(2024, 1, 1)) == date(2024, 1, 1)
        assert SyncTracker._norm_date(datetime(2024, 1, 1, 10, 30)) == date(2024, 1, 1)
