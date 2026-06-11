"""Tests for date_segmenter incremental segment_days 切段。

all_market dataset(segment_days>0)在 incremental 模式必須把多日 gap 切成
多段 — FinMind all_market 端口單請求只回 1 日。segment_days=0 的 per_stock
低頻 dataset 維持單段。
"""

from __future__ import annotations

import sys
from datetime import date, datetime
from pathlib import Path
from unittest.mock import MagicMock

import pytest

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


# segments() 段尾凍結在固定日 — 隔離 15:30 cutoff 的時刻相依(effective_today
# 本身另測 TestEffectiveToday)
FROZEN_TODAY = date(2026, 6, 11)


class TestIncrementalSegmenting:
    @pytest.fixture(autouse=True)
    def _freeze_effective_today(self, monkeypatch):
        import date_segmenter

        monkeypatch.setattr(
            date_segmenter, "effective_today", lambda now=None: FROZEN_TODAY
        )

    def test_segment_days_1_splits_multi_day_gap_into_daily(self):
        """all_market segment_days=1:多日 gap → 每日一段(否則只抓到 1 天)。"""
        seg = _segmenter(last_sync=date(2026, 5, 18))
        segs = seg.segments(_api(segment_days=1), "incremental", "__ALL__")
        # last_sync 05-18 → start 05-19,每段單日,最後一段結束於 effective today
        assert segs, "should produce at least one segment"
        assert all(s == e for s, e in segs), f"每段應為單日:{segs}"
        assert segs[0] == ("2026-05-19", "2026-05-19")
        assert date.fromisoformat(segs[-1][1]) == FROZEN_TODAY

    def test_segment_days_0_keeps_single_segment(self):
        """per_stock segment_days=0:維持單段(per_stock 多日 range 正常)。"""
        seg = _segmenter(last_sync=date(2026, 5, 18))
        segs = seg.segments(_api(segment_days=0), "incremental", "2330")
        assert len(segs) == 1
        assert segs[0][0] == "2026-05-19"
        assert date.fromisoformat(segs[0][1]) == FROZEN_TODAY

    def test_synced_today_yields_no_segments(self):
        """last_sync 已是 today → start>today → segment_days>0 回空(無多餘請求)。"""
        seg = _segmenter(last_sync=FROZEN_TODAY)
        segs = seg.segments(_api(segment_days=1), "incremental", "__ALL__")
        assert segs == []

    def test_segment_days_0_synced_today_yields_no_segments(self):
        """segment_days=0 已同步到 today → 回空,不寫出 start>end 的 backwards segment。"""
        seg = _segmenter(last_sync=FROZEN_TODAY)
        segs = seg.segments(_api(segment_days=0), "incremental", "2330")
        assert segs == []

    def test_no_sync_record_splits_from_backfill_start(self):
        """無同步紀錄 → 從 backfill_start 切段(本 case 證實會切很多段)。"""
        seg = _segmenter(last_sync=None)
        segs = seg.segments(_api(segment_days=1), "incremental", "__ALL__")
        assert segs[0] == ("2019-01-01", "2019-01-01")
        assert len(segs) > 365, "整段歷史應切成數千個單日段"


class TestEffectiveToday:
    """15:30 cutoff:EOD 發布前跑 collector 不抓今日段。

    empty 會推進水位線(sync_tracker SKIP_STATUSES 含 empty)→ 發布前抓到的
    「今日空段」會讓該日永久跳過(2026-06-09 實踩破洞)。cutoff 前段尾 cap 昨日。
    """

    def test_before_cutoff_returns_yesterday(self):
        from date_segmenter import effective_today

        assert effective_today(datetime(2026, 6, 11, 9, 0)) == date(2026, 6, 10)
        assert effective_today(datetime(2026, 6, 11, 15, 29, 59)) == date(2026, 6, 10)

    def test_at_or_after_cutoff_returns_today(self):
        from date_segmenter import effective_today

        assert effective_today(datetime(2026, 6, 11, 15, 30)) == date(2026, 6, 11)
        assert effective_today(datetime(2026, 6, 11, 19, 30)) == date(2026, 6, 11)

    def test_env_override(self, monkeypatch):
        from date_segmenter import effective_today

        monkeypatch.setenv("COLLECTOR_TODAY_CUTOFF", "00:00")  # 等效停用
        assert effective_today(datetime(2026, 6, 11, 0, 0)) == date(2026, 6, 11)
        monkeypatch.setenv("COLLECTOR_TODAY_CUTOFF", "21:00")
        assert effective_today(datetime(2026, 6, 11, 20, 59)) == date(2026, 6, 10)

    def test_bad_env_falls_back_to_default(self, monkeypatch):
        from date_segmenter import effective_today

        monkeypatch.setenv("COLLECTOR_TODAY_CUTOFF", "not-a-time")
        assert effective_today(datetime(2026, 6, 11, 9, 0)) == date(2026, 6, 10)
        assert effective_today(datetime(2026, 6, 11, 16, 0)) == date(2026, 6, 11)
