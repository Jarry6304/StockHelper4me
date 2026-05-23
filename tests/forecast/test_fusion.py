"""Tests for src/forecast/fusion.py — zero-parameter fusion."""

from __future__ import annotations

import sys
from datetime import date
from pathlib import Path
from unittest.mock import patch

import pytest

_REPO_ROOT = Path(__file__).resolve().parent.parent.parent
_SRC_ROOT = _REPO_ROOT / "src"
for p in (str(_SRC_ROOT), str(_REPO_ROOT)):
    if p not in sys.path:
        sys.path.insert(0, p)

from forecast.fusion import _fuse_intervals, fuse_one  # noqa: E402


class TestFuseIntervals:
    def test_empty_raises(self):
        with pytest.raises(ValueError):
            _fuse_intervals([])

    def test_single_interval_passes_through(self):
        lo, up, pt = _fuse_intervals([(90.0, 110.0)])
        assert lo == 90.0
        assert up == 110.0
        assert pt == 100.0

    def test_overlapping_intersection(self):
        # Three intervals: [90, 110], [95, 115], [85, 105]
        # intersection: max(90,95,85)=95, min(110,115,105)=105 → [95, 105]
        lo, up, pt = _fuse_intervals([(90.0, 110.0), (95.0, 115.0), (85.0, 105.0)])
        assert lo == 95.0
        assert up == 105.0
        assert pt == 100.0

    def test_disjoint_falls_back_to_divergence(self):
        # Two disjoint intervals: [80, 90] and [110, 120]
        # midpoints: 85, 115 → centroid = 100
        # half_widths: 5, 5 → max = 5
        # std of midpoints: sqrt(((85-100)^2 + (115-100)^2)/1) = sqrt(450) ≈ 21.21
        # half_w = 5 + 21.21 = 26.21
        # lower = 100 - 26.21 = 73.79;upper = 100 + 26.21 = 126.21
        lo, up, pt = _fuse_intervals([(80.0, 90.0), (110.0, 120.0)])
        assert pt == 100.0
        # Width 大致對齊
        import math
        expected_half = 5.0 + math.sqrt(450.0)
        assert abs((up - lo) / 2 - expected_half) < 1e-6

    def test_boundary_touching_intervals(self):
        # [90, 100] and [100, 110] — intersection at single point 100
        # max(90, 100) = 100, min(100, 110) = 100 → [100, 100]
        lo, up, pt = _fuse_intervals([(90.0, 100.0), (100.0, 110.0)])
        assert lo == 100.0
        assert up == 100.0
        assert pt == 100.0


# ─── fuse_one DB integration tests ───────────────────────────────────────────


class TestFuseOne:
    def test_no_eligible_cores_returns_status(self):
        with patch("forecast.fusion.eligible_cores", return_value=[]), \
             patch("forecast.fusion.upsert_forecast") as upd:
            res = fuse_one(
                None,
                stock_id="2330",
                forecast_date=date(2024, 6, 1),
                horizon_days=21,
                confidence=0.80,
            )
        assert res["status"] == "no_eligible_cores"
        upd.assert_not_called()

    def test_no_inputs_for_date(self):
        # eligible_cores returns names but no rows for the date
        with patch("forecast.fusion.eligible_cores", return_value=["kalman_cqr"]), \
             patch("forecast.fusion._fetch_eligible_forecasts", return_value=[]), \
             patch("forecast.fusion.upsert_forecast") as upd:
            res = fuse_one(
                None,
                stock_id="2330",
                forecast_date=date(2024, 6, 1),
                horizon_days=21,
                confidence=0.80,
            )
        assert res["status"] == "no_calibrated_inputs_for_date"
        upd.assert_not_called()

    def test_writes_fused_row(self):
        rows = [
            {"source_core": "kalman_cqr", "lower": 95.0, "upper": 105.0, "point": 100.0},
            {"source_core": "log_channel_cqr", "lower": 90.0, "upper": 102.0, "point": 96.0},
        ]
        written = []
        with patch("forecast.fusion.eligible_cores",
                   return_value=["kalman_cqr", "log_channel_cqr"]), \
             patch("forecast.fusion._fetch_eligible_forecasts", return_value=rows), \
             patch("forecast.fusion.upsert_forecast",
                   side_effect=lambda conn, row: written.append(row)):
            res = fuse_one(
                None,
                stock_id="2330",
                forecast_date=date(2024, 6, 1),
                horizon_days=21,
                confidence=0.80,
            )
        assert res["status"] == "written"
        # intersection [max(95,90), min(105,102)] = [95, 102]
        assert res["interval"] == (95.0, 102.0)
        assert res["intersection_valid"] is True
        row = written[0]
        assert row["source_core"] == "fusion"
        assert row["calibrated"] is True
        assert row["lower"] == 95.0
        assert row["upper"] == 102.0
        # params_hash records contributing cores sorted
        assert "kalman_cqr" in row["params_hash"]
        assert "log_channel_cqr" in row["params_hash"]

    def test_disjoint_falls_back_to_divergence(self):
        rows = [
            {"source_core": "kalman_cqr", "lower": 80.0, "upper": 90.0, "point": 85.0},
            {"source_core": "other_cqr", "lower": 110.0, "upper": 120.0, "point": 115.0},
        ]
        written = []
        with patch("forecast.fusion.eligible_cores",
                   return_value=["kalman_cqr", "other_cqr"]), \
             patch("forecast.fusion._fetch_eligible_forecasts", return_value=rows), \
             patch("forecast.fusion.upsert_forecast",
                   side_effect=lambda conn, row: written.append(row)):
            res = fuse_one(
                None,
                stock_id="2330",
                forecast_date=date(2024, 6, 1),
                horizon_days=21,
                confidence=0.80,
            )
        assert res["status"] == "written"
        assert res["intersection_valid"] is False
        # divergence fallback widens
        assert res["interval"][1] - res["interval"][0] > 20  # wider than either input
        assert res["point"] == 100.0  # centroid of midpoints (85, 115)

    def test_excludes_baseline_and_fusion_via_eligible(self):
        # The exclusion is enforced in eligible_cores SQL; verify here that
        # if eligible_cores is empty (because only baseline / fusion exist),
        # fuse_one bails out cleanly.
        with patch("forecast.fusion.eligible_cores", return_value=[]):
            res = fuse_one(
                None,
                stock_id="2330",
                forecast_date=date(2024, 6, 1),
                horizon_days=21,
                confidence=0.80,
            )
        assert res["status"] == "no_eligible_cores"
