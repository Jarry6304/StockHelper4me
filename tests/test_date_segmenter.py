"""Tests for date_segmenter incremental segment_days 切段。

all_market dataset(segment_days>0)在 incremental 模式必須把多日 gap 切成
多段 — FinMind all_market 端口單請求只回 1 日。segment_days=0 的 per_stock
低頻 dataset 維持單段。
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


def _segmenter(last_sync):
    from date_segmenter import DateSegmenter

    config = MagicMock()
    config.global_cfg.backfill_start_date = "2019-01-01"
    config.execution.start_date = "2019-01-01"
    tracker = MagicMock()
    tracker.get_last_sync.return_value = last_sync
    return DateSegmenter(config, tracker)


def _api(segment_days):
    from config_loader import ApiConfig

    return ApiConfig(
        name="price_daily",
        dataset="TaiwanStockPrice",
        param_mode="all_market" if segment_days else "per_stock",
        target_table="price_daily",
        phase=3,
        enabled=True,
        is_backer=True,
        segment_days=segment_days,
    )


class TestIncrementalSegmenting:
    def test_segment_days_1_splits_multi_day_gap_into_daily(self):
        """all_market segment_days=1:多日 gap → 每日一段(否則只抓到 1 天)。"""
        seg = _segmenter(last_sync=date(2026, 5, 18))
        segs = seg.segments(_api(segment_days=1), "incremental", "__ALL__")
        # last_sync 05-18 → start 05-19,每段單日,最後一段結束於 today
        assert segs, "should produce at least one segment"
        assert all(s == e for s, e in segs), f"每段應為單日:{segs}"
        assert segs[0] == ("2026-05-19", "2026-05-19")
        assert date.fromisoformat(segs[-1][1]) == date.today()

    def test_segment_days_0_keeps_single_segment(self):
        """per_stock segment_days=0:維持單段(per_stock 多日 range 正常)。"""
        seg = _segmenter(last_sync=date(2026, 5, 18))
        segs = seg.segments(_api(segment_days=0), "incremental", "2330")
        assert len(segs) == 1
        assert segs[0][0] == "2026-05-19"
        assert date.fromisoformat(segs[0][1]) == date.today()

    def test_synced_today_yields_no_segments(self):
        """last_sync 已是 today → start>today → segment_days>0 回空(無多餘請求)。"""
        seg = _segmenter(last_sync=date.today())
        segs = seg.segments(_api(segment_days=1), "incremental", "__ALL__")
        assert segs == []

    def test_no_sync_record_splits_from_backfill_start(self):
        """無同步紀錄 → 從 backfill_start 切段(本 case 證實會切很多段)。"""
        seg = _segmenter(last_sync=None)
        segs = seg.segments(_api(segment_days=1), "incremental", "__ALL__")
        assert segs[0] == ("2019-01-01", "2019-01-01")
        assert len(segs) > 365, "整段歷史應切成數千個單日段"
