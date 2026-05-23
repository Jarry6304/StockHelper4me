"""Tests for src/forecast/baseline.py — RW + vol cone + trend decomp."""

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

from forecast.baseline import make_baseline_forecast  # noqa: E402


def _series(n: int, start_close=100.0, drift=0.0):
    """Generate a synthetic series of n days with optional linear drift."""
    return [
        {
            "date": date(2024, 1, 1),  # date irrelevant for baseline math
            "asof_adj_close": start_close + i * drift,
        }
        for i in range(n)
    ]


class TestBaseline:
    def test_returns_none_if_series_too_short(self):
        out = make_baseline_forecast(
            _series(20),
            forecast_date=date(2024, 6, 1),
            horizon=21,
            confidence=0.80,
        )
        assert out is None

    def test_returns_shape(self):
        # 300 days flat at 100 → returns ≈ 0, interval ≈ trend
        out = make_baseline_forecast(
            _series(300, start_close=100.0, drift=0.0),
            forecast_date=date(2024, 6, 1),
            horizon=21,
            confidence=0.80,
            stock_id="2330",
        )
        assert out is not None
        # Required keys
        for k in ("lower", "upper", "point", "confidence", "calibrated",
                  "source_core", "params_hash", "forecast_date", "horizon_days"):
            assert k in out, f"missing key: {k}"
        assert out["source_core"] == "baseline"
        assert out["calibrated"] is False
        assert out["confidence"] == 0.80
        # Trend on flat series = 100
        assert out["point"] == pytest.approx(100.0)
        # Bounds bracket trend
        assert out["lower"] <= out["point"] <= out["upper"]

    def test_flat_series_tight_band(self):
        # Pure-flat returns → quantile band = (0, 0) → lower==upper==trend
        out = make_baseline_forecast(
            _series(300, start_close=100.0, drift=0.0),
            forecast_date=date(2024, 6, 1),
            horizon=21,
            confidence=0.95,
        )
        assert out["lower"] == pytest.approx(out["upper"])
        assert out["lower"] == pytest.approx(100.0)

    def test_wider_confidence_wider_band(self):
        # Synthetic noisy series so quantiles differ
        import math
        # Alternating up/down 1% pattern → returns sequence has variance
        closes = [100.0 * (1 + 0.01 * math.sin(i)) for i in range(400)]
        series = [{"date": date(2024, 1, 1), "asof_adj_close": c} for c in closes]
        out80 = make_baseline_forecast(
            series, forecast_date=date(2024, 6, 1), horizon=21, confidence=0.80
        )
        out95 = make_baseline_forecast(
            series, forecast_date=date(2024, 6, 1), horizon=21, confidence=0.95
        )
        assert out95 is not None and out80 is not None
        width80 = out80["upper"] - out80["lower"]
        width95 = out95["upper"] - out95["lower"]
        assert width95 >= width80  # higher confidence ⇒ wider (or equal)

    def test_params_hash_stable(self):
        out1 = make_baseline_forecast(
            _series(300), forecast_date=date(2024, 6, 1), horizon=21, confidence=0.80
        )
        out2 = make_baseline_forecast(
            _series(300), forecast_date=date(2024, 6, 2), horizon=21, confidence=0.80
        )
        # params (decompose_window, cone_lookback_days) identical → same hash
        assert out1["params_hash"] == out2["params_hash"]
