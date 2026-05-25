"""Tests for src/fusion/dual_track/track2.py — 軌道二(統計)讀法。

對齊 m3Spec/dual_track_resonance.md §三 + §七 + §十一。
"""

from __future__ import annotations

import sys
from datetime import date
from pathlib import Path

import pytest

_REPO_ROOT = Path(__file__).resolve().parent.parent.parent
_SRC_ROOT = _REPO_ROOT / "src"
for p in (str(_SRC_ROOT), str(_REPO_ROOT)):
    if p not in sys.path:
        sys.path.insert(0, p)

from fusion.dual_track._shared import BAND_WIDTH_THRESHOLD  # noqa: E402
from fusion.dual_track.track2 import fetch_band, read_track2  # noqa: E402


# ─── Mock connection ─────────────────────────────────────────────────────────


class _CursorMock:
    def __init__(self, response_map):
        """response_map: dict mapping source_core → row(or None)。"""
        self.response_map = response_map
        self.calls: list[tuple[str, tuple]] = []
        self._last_src = None

    def __enter__(self):
        return self

    def __exit__(self, *a):
        return False

    def execute(self, sql, params):
        self.calls.append((sql, params))
        # params 最後一個是 source_core(對齊 track2.fetch_band 的 SQL)
        self._last_src = params[-1] if params else None

    def fetchall(self):
        row = self.response_map.get(self._last_src)
        return [row] if row else []


class _ConnMock:
    def __init__(self, response_map: dict):
        self.response_map = response_map
        self.cursor_obj = _CursorMock(response_map)

    def cursor(self, *a, **kw):
        # 新 cursor 每次都用同 response_map(read_track2 多次 fetch_band)
        self.cursor_obj = _CursorMock(self.response_map)
        return self.cursor_obj


# ─── fetch_band ─────────────────────────────────────────────────────────────


class TestFetchBand:
    def test_fusion_prefers(self):
        """fusion source 優先;若有 fusion row 即用,不查 kalman_cqr。"""
        conn = _ConnMock({
            "fusion": {"lower": 95.0, "upper": 105.0, "point": 100.0, "source_core": "fusion"},
        })
        band = fetch_band(
            conn, stock_id="2330", forecast_date=date(2024, 6, 1),
            horizon_days=63, confidence=0.80, current_price=100.0,
        )
        assert band is not None
        assert band.source_core == "fusion"
        assert band.lower == 95.0
        assert band.upper == 105.0
        assert band.point == 100.0

    def test_kalman_fallback(self):
        """fusion 缺則退 kalman_cqr。"""
        conn = _ConnMock({
            "kalman_cqr": {"lower": 90.0, "upper": 110.0, "point": 100.0,
                            "source_core": "kalman_cqr"},
        })
        band = fetch_band(
            conn, stock_id="2330", forecast_date=date(2024, 6, 1),
            horizon_days=63, confidence=0.80, current_price=100.0,
        )
        assert band is not None
        assert band.source_core == "kalman_cqr"

    def test_width_ratio(self):
        conn = _ConnMock({
            "fusion": {"lower": 95.0, "upper": 105.0, "point": 100.0, "source_core": "fusion"},
        })
        band = fetch_band(
            conn, stock_id="2330", forecast_date=date(2024, 6, 1),
            horizon_days=63, confidence=0.80, current_price=100.0,
        )
        assert band.width_ratio == 0.1  # (105-95)/100
        assert band.is_overly_wide is False  # 0.1 < 0.30

    def test_is_overly_wide_triggered(self):
        # width / current = 50/100 = 0.5 > 0.30
        conn = _ConnMock({
            "fusion": {"lower": 75.0, "upper": 125.0, "point": 100.0, "source_core": "fusion"},
        })
        band = fetch_band(
            conn, stock_id="2330", forecast_date=date(2024, 6, 1),
            horizon_days=63, confidence=0.80, current_price=100.0,
        )
        assert band.is_overly_wide is True

    def test_no_current_price_no_overly_wide_judgement(self):
        """current_price 缺 → width_ratio=None,is_overly_wide=False(無法判)。"""
        conn = _ConnMock({
            "fusion": {"lower": 75.0, "upper": 125.0, "point": 100.0, "source_core": "fusion"},
        })
        band = fetch_band(
            conn, stock_id="2330", forecast_date=date(2024, 6, 1),
            horizon_days=63, confidence=0.80, current_price=None,
        )
        assert band.width_ratio is None
        assert band.is_overly_wide is False

    def test_no_row_returns_none(self):
        conn = _ConnMock({})
        band = fetch_band(
            conn, stock_id="2330", forecast_date=date(2024, 6, 1),
            horizon_days=63, confidence=0.80, current_price=100.0,
        )
        assert band is None

    def test_sql_filters_internal_only(self):
        """B-4 機制丙:SQL 必含 internal_only = FALSE 過濾。"""
        conn = _ConnMock({})
        fetch_band(
            conn, stock_id="2330", forecast_date=date(2024, 6, 1),
            horizon_days=63, confidence=0.80, current_price=100.0,
        )
        sql, _ = conn.cursor_obj.calls[0]
        assert "internal_only = FALSE" in sql


# ─── read_track2 ─────────────────────────────────────────────────────────────


class TestReadTrack2:
    def test_multi_horizon(self):
        """21 / 63 / 126 三個 horizon 都有 row → 3 個 band。"""
        conn = _ConnMock({
            "fusion": {"lower": 95.0, "upper": 105.0, "point": 100.0, "source_core": "fusion"},
        })
        t2 = read_track2(
            conn, stock_id="2330", as_of=date(2024, 6, 1), current_price=100.0,
        )
        assert set(t2.horizons.keys()) == {21, 63, 126}
        assert t2.primary_horizon == 63
        assert t2.primary_band is not None
        assert t2.primary_band.horizon_days == 63

    def test_empty_when_no_rows(self):
        conn = _ConnMock({})
        t2 = read_track2(
            conn, stock_id="2330", as_of=date(2024, 6, 1), current_price=100.0,
        )
        assert t2.primary_band is None
        assert t2.horizons == {}
        assert any("no forecast_log rows" in n for n in t2.notes)

    def test_custom_horizons(self):
        conn = _ConnMock({
            "fusion": {"lower": 95.0, "upper": 105.0, "point": 100.0, "source_core": "fusion"},
        })
        t2 = read_track2(
            conn, stock_id="2330", as_of=date(2024, 6, 1), current_price=100.0,
            horizons=(30, 60),
        )
        assert set(t2.horizons.keys()) == {30, 60}
