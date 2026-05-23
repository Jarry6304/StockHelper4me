"""Tests for src/forecast/calibration.py — CQR / ACI math + I/O."""

from __future__ import annotations

import math
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

from forecast.calibration import (  # noqa: E402
    nonconformity_score,
    cqr_quantile,
    conformalize_one,
)


class TestNonconformityScore:
    def test_inside_interval_is_negative(self):
        # realized=100 ∈ [90, 110] → score = max(90-100, 100-110) = max(-10, -10) = -10
        assert nonconformity_score(100.0, 90.0, 110.0) == -10.0

    def test_above_interval_is_positive(self):
        # realized=120, U=110 → score = max(90-120, 120-110) = max(-30, 10) = 10
        assert nonconformity_score(120.0, 90.0, 110.0) == 10.0

    def test_below_interval_is_positive(self):
        # realized=80, L=90 → score = max(90-80, 80-110) = max(10, -30) = 10
        assert nonconformity_score(80.0, 90.0, 110.0) == 10.0

    def test_on_boundary_is_zero(self):
        # realized=L
        assert nonconformity_score(90.0, 90.0, 110.0) == 0.0
        # realized=U
        assert nonconformity_score(110.0, 90.0, 110.0) == 0.0


class TestCqrQuantile:
    def test_empty_returns_inf(self):
        assert math.isinf(cqr_quantile([], 0.80))

    def test_all_negative_scores_returns_negative_q(self):
        # All settled forecasts had realized inside interval → q < 0
        scores = [-1.0, -2.0, -3.0, -4.0, -5.0] * 20  # n=100
        q = cqr_quantile(scores, 0.80)
        assert q < 0
        # Width adjustment with q<0 SHRINKS the interval — coverage was higher
        # than confidence, calibration tightens.

    def test_all_positive_scores_returns_positive_q(self):
        # All settled forecasts had realized OUTSIDE interval → q > 0
        scores = [1.0, 2.0, 3.0, 4.0, 5.0] * 20  # n=100
        q = cqr_quantile(scores, 0.80)
        assert q > 0
        # Width adjustment with q>0 WIDENS the interval — coverage was lower
        # than confidence, calibration loosens.

    def test_finite_sample_correction_picks_correct_index(self):
        # n=10, α=0.2, k = ceil(11 * 0.8) = 9, so 9-th smallest score (1-indexed)
        scores = list(range(10))  # 0..9
        # 9-th smallest = 8
        assert cqr_quantile(scores, 0.80) == 8.0

    def test_small_sample_returns_inf_when_k_exceeds_n(self):
        # n=4, α=0.05, k = ceil(5 * 0.95) = 5 > 4 → +inf
        assert math.isinf(cqr_quantile([1.0, 2.0, 3.0, 4.0], 0.95))


# ─── conformalize_one DB integration tests ───────────────────────────────────


def _raw_row(lower=90.0, upper=110.0, point=100.0):
    return {"lower": lower, "upper": upper, "point": point,
            "confidence": 0.80, "params_hash": "raw123"}


def _settled_row(lower, upper, realized, fdate):
    return {"lower": lower, "upper": upper, "realized_price": realized,
            "forecast_date": fdate}


class TestConformalizeOne:
    def test_no_raw_returns_no_raw(self):
        with patch("forecast.calibration._fetch_raw_forecast", return_value=None), \
             patch("forecast.calibration._fetch_calibration_set", return_value=[]), \
             patch("forecast.calibration.upsert_forecast") as upd:
            res = conformalize_one(
                None, stock_id="2330", asof=date(2024, 6, 1),
                horizon_days=21, confidence=0.80,
            )
        assert res["status"] == "no_raw"
        upd.assert_not_called()

    def test_insufficient_calibration_size(self):
        cal = [_settled_row(90, 110, 100.0, date(2024, 1, i + 1)) for i in range(10)]
        with patch("forecast.calibration._fetch_raw_forecast", return_value=_raw_row()), \
             patch("forecast.calibration._fetch_calibration_set", return_value=cal), \
             patch("forecast.calibration.upsert_forecast") as upd:
            res = conformalize_one(
                None, stock_id="2330", asof=date(2024, 6, 1),
                horizon_days=21, confidence=0.80,
                min_calibration_size=30,
            )
        assert res["status"] == "insufficient_calibration"
        assert res["n"] == 10
        upd.assert_not_called()

    def test_all_hits_shrinks_interval(self):
        # 100 settled rows, all realized perfectly in middle of [90, 110]
        cal = [_settled_row(90, 110, 100.0, date(2024, 1, 1)) for _ in range(100)]
        written: list[dict] = []
        with patch("forecast.calibration._fetch_raw_forecast", return_value=_raw_row()), \
             patch("forecast.calibration._fetch_calibration_set", return_value=cal), \
             patch("forecast.calibration.upsert_forecast",
                   side_effect=lambda conn, row: written.append(row)):
            res = conformalize_one(
                None, stock_id="2330", asof=date(2024, 6, 1),
                horizon_days=21, confidence=0.80,
                min_calibration_size=30,
            )
        assert res["status"] == "written"
        # scores all -10 (realized=100, L=90, U=110)
        # q = quantile at level depending on n=100, α=0.2 → k=ceil(101*0.8)=81
        # 81st-smallest of 100 copies of -10 is still -10
        assert res["q"] == -10.0
        # Width: original 20, new lower = 90 - (-10) = 100, new upper = 110 + (-10) = 100
        # → calibrated interval is collapsed (point estimate-like)
        assert len(written) == 1
        row = written[0]
        assert row["source_core"] == "kalman_cqr"
        assert row["calibrated"] is True
        assert row["lower"] == 100.0  # 90 - (-10)
        assert row["upper"] == 100.0  # 110 + (-10)

    def test_all_misses_widens_interval(self):
        # 100 settled rows, realized always 50 above upper bound
        cal = [_settled_row(90, 110, 160.0, date(2024, 1, 1)) for _ in range(100)]
        # nonconformity = max(90-160, 160-110) = max(-70, 50) = 50
        written: list[dict] = []
        with patch("forecast.calibration._fetch_raw_forecast", return_value=_raw_row()), \
             patch("forecast.calibration._fetch_calibration_set", return_value=cal), \
             patch("forecast.calibration.upsert_forecast",
                   side_effect=lambda conn, row: written.append(row)):
            res = conformalize_one(
                None, stock_id="2330", asof=date(2024, 6, 1),
                horizon_days=21, confidence=0.80,
                min_calibration_size=30,
            )
        assert res["q"] == 50.0
        row = written[0]
        assert row["lower"] == 40.0   # 90 - 50
        assert row["upper"] == 160.0  # 110 + 50
        assert row["calibrated"] is True

    def test_params_hash_includes_cqr_n_marker(self):
        cal = [_settled_row(90, 110, 100.0, date(2024, 1, 1)) for _ in range(50)]
        written = []
        with patch("forecast.calibration._fetch_raw_forecast", return_value=_raw_row()), \
             patch("forecast.calibration._fetch_calibration_set", return_value=cal), \
             patch("forecast.calibration.upsert_forecast",
                   side_effect=lambda conn, row: written.append(row)):
            conformalize_one(
                None, stock_id="2330", asof=date(2024, 6, 1),
                horizon_days=21, confidence=0.80, min_calibration_size=30,
            )
        # Hash should encode calibration size for traceability
        assert "cqr_n=50" in written[0]["params_hash"]
