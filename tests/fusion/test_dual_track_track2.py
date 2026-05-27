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
from fusion.dual_track.track2 import (  # noqa: E402
    fetch_band,
    fetch_bands_batch,
    read_track2,
)


# ─── Mock connection(v4.28+ batch-aware)────────────────────────────────────


class _CursorMock:
    """Mock cursor for v4.28+ batch query.

    Batch SQL params shape: (stock_id, forecast_date, horizons_list, sources_list,
    confidence). fetchall() returns rows for (requested_sources × requested_horizons)
    whose source_core appears in `response_map`.
    """

    def __init__(self, response_map):
        """response_map: {source_core: row_dict}(row 不含 horizon_days,由 mock 補)。"""
        self.response_map = response_map
        self.calls: list[tuple[str, tuple]] = []
        self._last_params: tuple | None = None

    def __enter__(self):
        return self

    def __exit__(self, *a):
        return False

    def execute(self, sql, params):
        self.calls.append((sql, params))
        self._last_params = params

    def fetchall(self):
        if not self._last_params or len(self._last_params) < 5:
            return []
        # Batch: (stock_id, forecast_date, horizons_list, sources_list, confidence)
        horizons = self._last_params[2]
        sources = self._last_params[3]
        if not isinstance(horizons, list) or not isinstance(sources, list):
            return []
        out: list[dict] = []
        for src in sources:
            row = self.response_map.get(src)
            if not row:
                continue
            for h in horizons:
                new_row = dict(row)
                new_row["horizon_days"] = h
                out.append(new_row)
        return out


class _ConnMock:
    def __init__(self, response_map: dict):
        self.response_map = response_map
        self.cursor_obj = _CursorMock(response_map)

    def cursor(self, *a, **kw):
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

    def test_uses_single_batch_query(self):
        """v4.28+ N+1 fix:read_track2 對 3 horizons + 6 sources 只發 1 SQL,非 18。"""
        conn = _ConnMock({
            "fusion": {"lower": 95.0, "upper": 105.0, "point": 100.0,
                       "source_core": "fusion"},
        })
        read_track2(conn, stock_id="2330", as_of=date(2024, 6, 1),
                    current_price=100.0)
        assert len(conn.cursor_obj.calls) == 1
        sql, params = conn.cursor_obj.calls[0]
        assert "ANY(%s::int[])" in sql
        assert "ANY(%s::text[])" in sql
        assert isinstance(params[2], list)
        assert sorted(params[2]) == [21, 63, 126]
        assert isinstance(params[3], list)
        assert "fusion" in params[3]


# ─── fetch_bands_batch(v4.28+)───────────────────────────────────────────────


class TestFetchBandsBatch:
    def test_returns_preferred_source(self):
        """同 horizon 多 source row → 取 fusion(pref rank 0 勝過 kalman_cqr)。"""
        conn = _ConnMock({
            "fusion": {"lower": 95.0, "upper": 105.0, "point": 100.0,
                       "source_core": "fusion"},
            "kalman_cqr": {"lower": 90.0, "upper": 110.0, "point": 100.0,
                            "source_core": "kalman_cqr"},
        })
        out = fetch_bands_batch(
            conn, stock_id="2330", forecast_date=date(2024, 6, 1),
            horizons=(63,), confidence=0.80, current_price=100.0,
        )
        assert 63 in out
        assert out[63].source_core == "fusion"  # rank 0 勝
        assert out[63].lower == 95.0

    def test_kalman_fallback_when_fusion_missing(self):
        conn = _ConnMock({
            "kalman_cqr": {"lower": 90.0, "upper": 110.0, "point": 100.0,
                            "source_core": "kalman_cqr"},
        })
        out = fetch_bands_batch(
            conn, stock_id="2330", forecast_date=date(2024, 6, 1),
            horizons=(63,), confidence=0.80, current_price=100.0,
        )
        assert out[63].source_core == "kalman_cqr"

    def test_missing_horizon_returns_partial(self):
        """部分 horizon 無 row → result 只含有 row 的 horizon。"""
        # Mock 全 source 都有 row,但 read 只請求 (21,) → 只回 21
        conn = _ConnMock({
            "fusion": {"lower": 95.0, "upper": 105.0, "point": 100.0,
                       "source_core": "fusion"},
        })
        out = fetch_bands_batch(
            conn, stock_id="2330", forecast_date=date(2024, 6, 1),
            horizons=(21,), confidence=0.80, current_price=100.0,
        )
        assert set(out.keys()) == {21}

    def test_empty_returns_empty_dict(self):
        conn = _ConnMock({})
        out = fetch_bands_batch(
            conn, stock_id="2330", forecast_date=date(2024, 6, 1),
            horizons=(21, 63, 126), confidence=0.80, current_price=100.0,
        )
        assert out == {}

    def test_empty_horizons_returns_empty(self):
        """horizons=() → 0 query,直接回空 dict(無 DB roundtrip)。"""
        conn = _ConnMock({
            "fusion": {"lower": 95.0, "upper": 105.0, "point": 100.0,
                       "source_core": "fusion"},
        })
        out = fetch_bands_batch(
            conn, stock_id="2330", forecast_date=date(2024, 6, 1),
            horizons=(), confidence=0.80, current_price=100.0,
        )
        assert out == {}
        assert conn.cursor_obj.calls == []  # 沒發 query

    def test_sql_contains_internal_only_filter(self):
        """B-4 機制丙(v4.25)+ v4.28+ batch:SQL 必含 internal_only = FALSE。"""
        conn = _ConnMock({})
        fetch_bands_batch(
            conn, stock_id="2330", forecast_date=date(2024, 6, 1),
            horizons=(63,), confidence=0.80, current_price=100.0,
        )
        sql, _ = conn.cursor_obj.calls[0]
        assert "internal_only = FALSE" in sql

    def test_explicit_cast_in_sql(self):
        """v4.28+ Plan agent caveat:psycopg list → SMALLINT[] / TEXT[] 需顯式 cast。"""
        conn = _ConnMock({})
        fetch_bands_batch(
            conn, stock_id="2330", forecast_date=date(2024, 6, 1),
            horizons=(63,), confidence=0.80, current_price=100.0,
        )
        sql, _ = conn.cursor_obj.calls[0]
        assert "ANY(%s::int[])" in sql
        assert "ANY(%s::text[])" in sql
