"""Tests for src/forecast/fundamental_forecast.py — revenue YoY drift core."""

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

from forecast.fundamental_forecast import (  # noqa: E402
    _compute_realized_vol,
    _compute_yoy_3m_avg,
    make_fundamental_forecast,
)


def _flat_series(n: int, start_close=100.0, daily_drift=0.0):
    """Synthetic OHLCV-like series."""
    return [
        {"date": date(2024, 1, 1), "asof_adj_close": start_close + i * daily_drift}
        for i in range(n)
    ]


def _revenue_rows_with_yoy(yoy_pct: float, n_months: int = 24,
                            base_revenue: float = 1_000_000.0):
    """Synthetic 24-month revenue rows where last 3 months show specified YoY.

    For each month in last 12, revenue = base * (1 + yoy_pct).
    For prior 12 months, revenue = base.
    """
    rows = []
    # Earlier 12 months (months 1..12 of year 2022)
    for m in range(1, 13):
        rows.append({
            "revenue_year": 2022,
            "revenue_month": m,
            "revenue": base_revenue,
            "date": date(2022, m, 1),
        })
    # Latest 12 months (months 1..12 of year 2023) with YoY uplift
    for m in range(1, 13):
        rows.append({
            "revenue_year": 2023,
            "revenue_month": m,
            "revenue": base_revenue * (1 + yoy_pct),
            "date": date(2023, m, 1),
        })
    return rows[:n_months * 2 if n_months < 24 else len(rows)]


class TestComputeYoy:
    def test_yoy_zero_when_revenue_flat(self):
        rows = _revenue_rows_with_yoy(0.0)
        yoy = _compute_yoy_3m_avg(rows)
        assert yoy == pytest.approx(0.0, abs=1e-9)

    def test_yoy_positive_when_revenue_grows(self):
        rows = _revenue_rows_with_yoy(0.20)  # +20% YoY
        yoy = _compute_yoy_3m_avg(rows)
        assert yoy == pytest.approx(0.20, rel=1e-6)

    def test_yoy_negative_when_revenue_drops(self):
        rows = _revenue_rows_with_yoy(-0.15)  # -15% YoY
        yoy = _compute_yoy_3m_avg(rows)
        assert yoy == pytest.approx(-0.15, rel=1e-6)

    def test_yoy_none_when_no_base_year(self):
        # Only latest 12 months, no base year for YoY
        rows = [
            {"revenue_year": 2023, "revenue_month": m, "revenue": 1_000_000.0,
             "date": date(2023, m, 1)}
            for m in range(1, 13)
        ]
        yoy = _compute_yoy_3m_avg(rows)
        assert yoy is None

    def test_yoy_skips_invalid_revenue(self):
        rows = [
            {"revenue_year": 2022, "revenue_month": 12, "revenue": None,
             "date": date(2022, 12, 1)},
            {"revenue_year": 2022, "revenue_month": 11, "revenue": 1_000_000.0,
             "date": date(2022, 11, 1)},
            {"revenue_year": 2023, "revenue_month": 11, "revenue": 1_100_000.0,
             "date": date(2023, 11, 1)},
            # Need at least one matching pair
        ]
        yoy = _compute_yoy_3m_avg(rows)
        assert yoy == pytest.approx(0.10, rel=1e-6)

    def test_yoy_empty_returns_none(self):
        assert _compute_yoy_3m_avg([]) is None


class TestComputeVol:
    def test_zero_vol_flat_series(self):
        sigma = _compute_realized_vol([100.0] * 100, lookback=60)
        # Flat series → sigma = 0 → returns None (degenerate)
        assert sigma is None

    def test_positive_vol_real_series(self):
        # Geometric brownian-ish: alternating 1% up/down
        import math
        closes = [100.0 * math.exp(0.01 * ((-1) ** i)) for i in range(100)]
        # cumulate
        closes = []
        p = 100.0
        for i in range(100):
            p *= math.exp(0.01 * ((-1) ** i))
            closes.append(p)
        sigma = _compute_realized_vol(closes, lookback=60)
        assert sigma is not None
        assert 0.005 < sigma < 0.05

    def test_too_short_returns_none(self):
        assert _compute_realized_vol([100.0] * 30, lookback=60) is None


class TestMakeFundamentalForecast:
    def _setup(self, yoy_pct: float, price_drift: float = 0.001):
        rows = _revenue_rows_with_yoy(yoy_pct)
        # 120 days with small daily drift to give meaningful vol
        import math
        closes_series = []
        p = 100.0
        for i in range(120):
            # small noisy walk
            p *= math.exp(price_drift * ((-1) ** i))
            closes_series.append({
                "date": date(2024, 1, 1),
                "asof_adj_close": p,
            })
        return rows, closes_series

    def test_returns_none_without_revenue_data(self):
        out = make_fundamental_forecast(
            _flat_series(120, 100.0, 0.5),
            forecast_date=date(2024, 6, 1),
            horizon=63,
            confidence=0.80,
            stock_id="2330",
            revenue_rows=[],  # no revenue
        )
        assert out is None

    def test_returns_none_without_price_series(self):
        rows = _revenue_rows_with_yoy(0.10)
        out = make_fundamental_forecast(
            [],
            forecast_date=date(2024, 6, 1),
            horizon=63,
            confidence=0.80,
            stock_id="2330",
            revenue_rows=rows,
        )
        assert out is None

    def test_returns_none_without_conn_or_rows(self):
        out = make_fundamental_forecast(
            _flat_series(120, 100.0, 0.5),
            forecast_date=date(2024, 6, 1),
            horizon=63,
            confidence=0.80,
            stock_id="2330",
        )
        assert out is None

    def test_returns_shape(self):
        rows, closes = self._setup(yoy_pct=0.10)
        out = make_fundamental_forecast(
            closes,
            forecast_date=date(2024, 6, 1),
            horizon=63,
            confidence=0.80,
            stock_id="2330",
            revenue_rows=rows,
        )
        assert out is not None
        for k in ["stock_id", "forecast_date", "horizon_days", "lower",
                  "upper", "point", "confidence", "calibrated", "source_core",
                  "regime_tag", "params_hash"]:
            assert k in out, f"missing key {k}"
        assert out["source_core"] == "fundamental_forecast_core"
        assert out["calibrated"] is False
        assert out["lower"] < out["upper"]
        assert out["lower"] <= out["point"] <= out["upper"]

    def test_drift_positive_for_positive_yoy(self):
        rows, closes = self._setup(yoy_pct=0.20)
        out = make_fundamental_forecast(
            closes,
            forecast_date=date(2024, 6, 1),
            horizon=63,
            confidence=0.80,
            stock_id="2330",
            revenue_rows=rows,
        )
        current_price = closes[-1]["asof_adj_close"]
        assert out["point"] > current_price  # drift up

    def test_drift_negative_for_negative_yoy(self):
        rows, closes = self._setup(yoy_pct=-0.20)
        out = make_fundamental_forecast(
            closes,
            forecast_date=date(2024, 6, 1),
            horizon=63,
            confidence=0.80,
            stock_id="2330",
            revenue_rows=rows,
        )
        current_price = closes[-1]["asof_adj_close"]
        assert out["point"] < current_price  # drift down

    def test_drift_capped_at_drift_cap(self):
        rows, closes = self._setup(yoy_pct=2.0)  # +200% YoY extreme
        out = make_fundamental_forecast(
            closes,
            forecast_date=date(2024, 6, 1),
            horizon=126,
            confidence=0.80,
            stock_id="2330",
            revenue_rows=rows,
            drift_cap=0.20,
        )
        current_price = closes[-1]["asof_adj_close"]
        # drift_pct should be capped at +20%; point should not exceed 1.20 * current
        max_point = current_price * 1.20 * (1 + 1e-6)
        assert out["point"] <= max_point

    def test_regime_tag_contains_yoy(self):
        rows, closes = self._setup(yoy_pct=0.15)
        out = make_fundamental_forecast(
            closes,
            forecast_date=date(2024, 6, 1),
            horizon=63,
            confidence=0.80,
            stock_id="2330",
            revenue_rows=rows,
        )
        assert "yoy3m=" in out["regime_tag"]
        assert "+0.150" in out["regime_tag"]

    def test_params_hash_deterministic(self):
        rows, closes = self._setup(yoy_pct=0.10)
        out1 = make_fundamental_forecast(
            closes, date(2024, 6, 1), 63, 0.80,
            stock_id="2330", revenue_rows=rows,
        )
        out2 = make_fundamental_forecast(
            closes, date(2024, 6, 1), 63, 0.80,
            stock_id="2330", revenue_rows=rows,
        )
        assert out1["params_hash"] == out2["params_hash"]

    def test_horizon_scales_drift(self):
        rows, closes = self._setup(yoy_pct=0.20)
        out21 = make_fundamental_forecast(
            closes, date(2024, 6, 1), 21, 0.80,
            stock_id="2330", revenue_rows=rows,
        )
        out126 = make_fundamental_forecast(
            closes, date(2024, 6, 1), 126, 0.80,
            stock_id="2330", revenue_rows=rows,
        )
        cp = closes[-1]["asof_adj_close"]
        drift_21 = (out21["point"] - cp) / cp
        drift_126 = (out126["point"] - cp) / cp
        # Longer horizon → bigger drift (positive YoY)
        assert drift_126 > drift_21
