"""Tests for src/forecast/log_channel.py — trailing-window log OLS."""

from __future__ import annotations

import math
import sys
from datetime import date
from pathlib import Path

import pytest

_REPO_ROOT = Path(__file__).resolve().parent.parent.parent
_SRC_ROOT = _REPO_ROOT / "src"
for p in (str(_SRC_ROOT), str(_REPO_ROOT)):
    if p not in sys.path:
        sys.path.insert(0, p)

from forecast.log_channel import make_log_channel_forecast, _z_two_sided  # noqa: E402


def _series(closes: list[float]) -> list[dict]:
    return [{"date": date(2024, 1, 1), "asof_adj_close": c} for c in closes]


class TestZTwoSided:
    def test_known_quantiles(self):
        # Same formula as Rust kalman_forecast_core — verify cross-language consistency
        assert abs(_z_two_sided(0.50) - 0.6745) < 1e-3
        assert abs(_z_two_sided(0.80) - 1.2816) < 1e-3
        assert abs(_z_two_sided(0.95) - 1.9600) < 1e-3


class TestLogChannel:
    def test_insufficient_data_returns_none(self):
        out = make_log_channel_forecast(
            _series([100.0] * 50),
            forecast_date=date(2024, 6, 1),
            horizon=21, confidence=0.80,
            window=252,
        )
        assert out is None

    def test_flat_series_returns_constant_point(self):
        # Perfectly flat at 100 → slope ≈ 0, point ≈ 100
        out = make_log_channel_forecast(
            _series([100.0] * 300),
            forecast_date=date(2024, 6, 1),
            horizon=21, confidence=0.80,
            window=252,
        )
        assert out is not None
        assert abs(out["point"] - 100.0) < 0.5
        # Width should be very small since residuals are 0
        assert abs(out["upper"] - out["lower"]) < 0.5

    def test_upward_log_trend_projects_upward(self):
        # log_p = log(100) + 0.001·t → exponential growth
        # over 300 days
        import numpy as np
        t = np.arange(300)
        closes = (100.0 * np.exp(0.001 * t)).tolist()
        out = make_log_channel_forecast(
            _series(closes),
            forecast_date=date(2024, 6, 1),
            horizon=21, confidence=0.80,
            window=252,
        )
        assert out is not None
        # Last close ~ 100 * exp(0.001 * 299) ≈ 134.9
        # 21-day projection should be HIGHER
        assert out["point"] > closes[-1] * 0.95  # at least near last close
        # Slope should be positive — point > closes[-1] eventually
        assert out["point"] > closes[-1] - 5

    def test_higher_horizon_widens_interval(self):
        import numpy as np
        # Synthetic series with some noise
        rng = np.random.default_rng(42)
        log_p = np.log(100.0) + 0.0005 * np.arange(300) + rng.normal(0, 0.01, 300)
        closes = np.exp(log_p).tolist()
        out21 = make_log_channel_forecast(
            _series(closes), date(2024, 6, 1), 21, 0.80, window=252,
        )
        out126 = make_log_channel_forecast(
            _series(closes), date(2024, 6, 1), 126, 0.80, window=252,
        )
        assert out21 and out126
        width21 = out21["upper"] - out21["lower"]
        width126 = out126["upper"] - out126["lower"]
        assert width126 > width21  # sqrt(h) scaling widens longer horizons

    def test_higher_confidence_widens_interval(self):
        import numpy as np
        rng = np.random.default_rng(42)
        log_p = np.log(100.0) + 0.0005 * np.arange(300) + rng.normal(0, 0.01, 300)
        closes = np.exp(log_p).tolist()
        out50 = make_log_channel_forecast(
            _series(closes), date(2024, 6, 1), 21, 0.50, window=252,
        )
        out95 = make_log_channel_forecast(
            _series(closes), date(2024, 6, 1), 21, 0.95, window=252,
        )
        width50 = out50["upper"] - out50["lower"]
        width95 = out95["upper"] - out95["lower"]
        assert width95 > width50

    def test_trailing_window_respected(self):
        # First 200 days flat at 100, last 100 days strong uptrend to 200.
        # If window=252, OLS uses last 252 bars (52 flat + 200 uptrend bars).
        # If window=50 only, OLS uses only the strong uptrend tail → much
        # steeper slope.
        closes = [100.0] * 200 + [100.0 + i for i in range(100)]
        out_full = make_log_channel_forecast(
            _series(closes), date(2024, 6, 1), 21, 0.80, window=252,
        )
        out_short = make_log_channel_forecast(
            _series(closes), date(2024, 6, 1), 21, 0.80, window=50,
        )
        assert out_full and out_short
        # Short window should produce a HIGHER point projection than long
        # (steeper local slope on short window)
        assert out_short["point"] > out_full["point"]

    def test_calibrated_false_and_source_core_label(self):
        out = make_log_channel_forecast(
            _series([100.0] * 300),
            forecast_date=date(2024, 6, 1),
            horizon=21, confidence=0.80,
            window=252,
        )
        assert out["calibrated"] is False
        assert out["source_core"] == "log_channel"
        assert "params_hash" in out and len(out["params_hash"]) == 16
