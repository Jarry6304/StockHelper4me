"""Tests for src/forecast/chip_forecast.py — institutional flow + margin core."""

from __future__ import annotations

import math
import sys
from datetime import date, timedelta
from pathlib import Path

import pytest

_REPO_ROOT = Path(__file__).resolve().parent.parent.parent
_SRC_ROOT = _REPO_ROOT / "src"
for p in (str(_SRC_ROOT), str(_REPO_ROOT)):
    if p not in sys.path:
        sys.path.insert(0, p)

from forecast.chip_forecast import (  # noqa: E402
    _compute_inst_score,
    _compute_margin_score,
    _net_flow,
    make_chip_forecast,
)


def _inst_row(d: date, fb=0, fs=0, fdsb=0, fdss=0, itb=0, its=0,
              db=0, ds=0, dhb=0, dhs=0):
    return {
        "date": d,
        "foreign_buy": fb, "foreign_sell": fs,
        "foreign_dealer_self_buy": fdsb, "foreign_dealer_self_sell": fdss,
        "investment_trust_buy": itb, "investment_trust_sell": its,
        "dealer_buy": db, "dealer_sell": ds,
        "dealer_hedging_buy": dhb, "dealer_hedging_sell": dhs,
    }


def _inst_rows_constant_flow(n_days: int, daily_net: float):
    """Build n_days of institutional rows with a constant net flow.

    We put all flow on foreign_buy/sell for simplicity.
    """
    base = date(2024, 1, 1)
    out = []
    for i in range(n_days):
        d = base + timedelta(days=i)
        if daily_net >= 0:
            out.append(_inst_row(d, fb=int(daily_net), fs=0))
        else:
            out.append(_inst_row(d, fb=0, fs=int(-daily_net)))
    return out


def _inst_rows_baseline_then_recent(
    baseline_days: int, baseline_flow: float,
    recent_days: int, recent_flow: float,
):
    """baseline_days rows of baseline_flow, then recent_days of recent_flow."""
    base = date(2024, 1, 1)
    rows = []
    for i in range(baseline_days):
        d = base + timedelta(days=i)
        if baseline_flow >= 0:
            rows.append(_inst_row(d, fb=int(baseline_flow), fs=0))
        else:
            rows.append(_inst_row(d, fb=0, fs=int(-baseline_flow)))
    for i in range(recent_days):
        d = base + timedelta(days=baseline_days + i)
        if recent_flow >= 0:
            rows.append(_inst_row(d, fb=int(recent_flow), fs=0))
        else:
            rows.append(_inst_row(d, fb=0, fs=int(-recent_flow)))
    return rows


def _margin_rows_linear(start_balance: float, end_balance: float, n_days: int):
    base = date(2024, 1, 1)
    if n_days <= 1:
        return [{"date": base, "margin_balance": start_balance}]
    return [
        {
            "date": base + timedelta(days=i),
            "margin_balance": start_balance
                + (end_balance - start_balance) * (i / (n_days - 1)),
        }
        for i in range(n_days)
    ]


def _flat_price_series(n: int, start_close=100.0, vol_factor=0.002):
    closes = []
    p = start_close
    for i in range(n):
        p *= math.exp(vol_factor * ((-1) ** i))
        closes.append({"date": date(2024, 1, 1), "asof_adj_close": p})
    return closes


class TestNetFlow:
    def test_all_zero(self):
        assert _net_flow(_inst_row(date(2024, 1, 1))) == 0.0

    def test_foreign_buy_only(self):
        row = _inst_row(date(2024, 1, 1), fb=1000)
        assert _net_flow(row) == 1000.0

    def test_all_five_categories_sum(self):
        row = _inst_row(date(2024, 1, 1),
                        fb=1000, fs=200,
                        fdsb=500, fdss=100,
                        itb=300, its=50,
                        db=200, ds=80,
                        dhb=100, dhs=20)
        # (1000-200) + (500-100) + (300-50) + (200-80) + (100-20)
        # = 800 + 400 + 250 + 120 + 80 = 1650
        assert _net_flow(row) == 1650.0

    def test_negative_net(self):
        row = _inst_row(date(2024, 1, 1), fs=500, ds=200)
        assert _net_flow(row) == -700.0

    def test_all_null_returns_none(self):
        row = {"date": date(2024, 1, 1)}
        # All buy/sell missing
        assert _net_flow(row) is None

    def test_empty_returns_none(self):
        assert _net_flow({}) is None
        assert _net_flow(None) is None


class TestInstScore:
    def test_zero_score_when_flow_unchanged(self):
        # Insufficient std → returns None (constant baseline has zero std)
        rows = _inst_rows_constant_flow(80, 1000)
        score = _compute_inst_score(rows)
        # When std=0, function returns None
        assert score is None

    def test_positive_score_when_recent_above_baseline(self):
        # Baseline: noisy flow centered at 0; Recent: positive spike
        import random
        random.seed(42)
        rows = []
        base = date(2024, 1, 1)
        for i in range(60):
            d = base + timedelta(days=i)
            net = random.gauss(0, 1000)
            if net >= 0:
                rows.append(_inst_row(d, fb=int(net), fs=0))
            else:
                rows.append(_inst_row(d, fb=0, fs=int(-net)))
        for i in range(20):
            d = base + timedelta(days=60 + i)
            rows.append(_inst_row(d, fb=3000, fs=0))  # 3 sigma positive
        score = _compute_inst_score(rows)
        assert score is not None
        assert score > 0.5

    def test_negative_score_when_recent_below_baseline(self):
        import random
        random.seed(42)
        rows = []
        base = date(2024, 1, 1)
        for i in range(60):
            d = base + timedelta(days=i)
            net = random.gauss(0, 1000)
            if net >= 0:
                rows.append(_inst_row(d, fb=int(net), fs=0))
            else:
                rows.append(_inst_row(d, fb=0, fs=int(-net)))
        for i in range(20):
            d = base + timedelta(days=60 + i)
            rows.append(_inst_row(d, fb=0, fs=3000))  # 3 sigma negative
        score = _compute_inst_score(rows)
        assert score is not None
        assert score < -0.5

    def test_too_few_rows_returns_none(self):
        rows = _inst_rows_constant_flow(30, 1000)
        assert _compute_inst_score(rows) is None

    def test_empty_returns_none(self):
        assert _compute_inst_score([]) is None


class TestMarginScore:
    def test_zero_when_flat(self):
        rows = _margin_rows_linear(100_000, 100_000, 22)
        score = _compute_margin_score(rows)
        assert score == pytest.approx(0.0)

    def test_negative_when_margin_grows(self):
        # +~10% margin over 20-day lookback → ~-1 (contrarian: bearish)
        # 22-row linear from 100k→110k means balance[-21]≈100.48k, end=110k
        # roc = (110k-100.48k)/100.48k ≈ 0.0948 → score ≈ -0.948
        rows = _margin_rows_linear(100_000, 110_000, 22)
        score = _compute_margin_score(rows)
        assert score == pytest.approx(-0.95, abs=0.05)
        assert score < -0.5  # directionally strongly negative

    def test_positive_when_margin_shrinks(self):
        rows = _margin_rows_linear(100_000, 90_000, 22)
        score = _compute_margin_score(rows)
        assert score == pytest.approx(0.95, abs=0.05)
        assert score > 0.5

    def test_saturated_when_extreme(self):
        rows = _margin_rows_linear(100_000, 150_000, 22)  # +50%
        score = _compute_margin_score(rows)
        assert score == pytest.approx(-1.0, abs=1e-6)  # capped at -1

    def test_too_few_rows_returns_none(self):
        assert _compute_margin_score(_margin_rows_linear(100, 100, 10)) is None

    def test_empty_returns_none(self):
        assert _compute_margin_score([]) is None


class TestMakeChipForecast:
    def _build_strong_pos_inst(self):
        import random
        random.seed(42)
        rows = []
        base = date(2024, 1, 1)
        for i in range(60):
            d = base + timedelta(days=i)
            net = random.gauss(0, 1000)
            if net >= 0:
                rows.append(_inst_row(d, fb=int(net), fs=0))
            else:
                rows.append(_inst_row(d, fb=0, fs=int(-net)))
        for i in range(20):
            d = base + timedelta(days=60 + i)
            rows.append(_inst_row(d, fb=3000, fs=0))
        return rows

    def _build_strong_neg_inst(self):
        import random
        random.seed(42)
        rows = []
        base = date(2024, 1, 1)
        for i in range(60):
            d = base + timedelta(days=i)
            net = random.gauss(0, 1000)
            if net >= 0:
                rows.append(_inst_row(d, fb=int(net), fs=0))
            else:
                rows.append(_inst_row(d, fb=0, fs=int(-net)))
        for i in range(20):
            d = base + timedelta(days=60 + i)
            rows.append(_inst_row(d, fb=0, fs=3000))
        return rows

    def test_returns_none_no_signals(self):
        out = make_chip_forecast(
            _flat_price_series(120),
            forecast_date=date(2024, 6, 1),
            horizon=63,
            confidence=0.80,
            stock_id="2330",
            inst_rows=[],
            margin_rows=[],
        )
        assert out is None

    def test_returns_none_without_price(self):
        out = make_chip_forecast(
            [],
            forecast_date=date(2024, 6, 1),
            horizon=63,
            confidence=0.80,
            stock_id="2330",
            inst_rows=self._build_strong_pos_inst(),
            margin_rows=_margin_rows_linear(100_000, 100_000, 22),
        )
        assert out is None

    def test_returns_shape_with_both_signals(self):
        out = make_chip_forecast(
            _flat_price_series(120),
            forecast_date=date(2024, 6, 1),
            horizon=63,
            confidence=0.80,
            stock_id="2330",
            inst_rows=self._build_strong_pos_inst(),
            margin_rows=_margin_rows_linear(100_000, 95_000, 22),  # margin -5%
        )
        assert out is not None
        for k in ["stock_id", "forecast_date", "horizon_days", "lower",
                  "upper", "point", "confidence", "calibrated", "source_core",
                  "regime_tag", "params_hash"]:
            assert k in out
        assert out["source_core"] == "chip_forecast_core"
        assert out["calibrated"] is False
        assert out["lower"] < out["upper"]
        assert "inst=" in out["regime_tag"]
        assert "margin=" in out["regime_tag"]

    def test_strong_inst_buy_drives_drift_up(self):
        series = _flat_price_series(120)
        out = make_chip_forecast(
            series, date(2024, 6, 1), 63, 0.80,
            stock_id="2330",
            inst_rows=self._build_strong_pos_inst(),
            margin_rows=_margin_rows_linear(100_000, 100_000, 22),
        )
        cp = series[-1]["asof_adj_close"]
        assert out["point"] > cp

    def test_strong_inst_sell_drives_drift_down(self):
        series = _flat_price_series(120)
        out = make_chip_forecast(
            series, date(2024, 6, 1), 63, 0.80,
            stock_id="2330",
            inst_rows=self._build_strong_neg_inst(),
            margin_rows=_margin_rows_linear(100_000, 100_000, 22),
        )
        cp = series[-1]["asof_adj_close"]
        assert out["point"] < cp

    def test_margin_only_works(self):
        series = _flat_price_series(120)
        out = make_chip_forecast(
            series, date(2024, 6, 1), 63, 0.80,
            stock_id="2330",
            inst_rows=[],  # no inst signal
            margin_rows=_margin_rows_linear(100_000, 90_000, 22),  # contrarian +
        )
        assert out is not None
        cp = series[-1]["asof_adj_close"]
        assert "margin=" in out["regime_tag"]
        assert "inst=" not in out["regime_tag"]
        assert out["point"] > cp  # contrarian: margin shrink → bullish

    def test_inst_only_works(self):
        series = _flat_price_series(120)
        out = make_chip_forecast(
            series, date(2024, 6, 1), 63, 0.80,
            stock_id="2330",
            inst_rows=self._build_strong_pos_inst(),
            margin_rows=[],
        )
        assert out is not None
        assert "inst=" in out["regime_tag"]
        assert "margin=" not in out["regime_tag"]

    def test_drift_capped(self):
        series = _flat_price_series(120)
        out = make_chip_forecast(
            series, date(2024, 6, 1), 252, 0.80,
            stock_id="2330",
            inst_rows=self._build_strong_pos_inst(),
            margin_rows=_margin_rows_linear(100_000, 80_000, 22),
            drift_cap=0.10,
        )
        cp = series[-1]["asof_adj_close"]
        assert out["point"] <= cp * 1.10 * (1 + 1e-6)

    def test_params_hash_deterministic(self):
        series = _flat_price_series(120)
        kwargs = dict(
            forecast_date=date(2024, 6, 1), horizon=63, confidence=0.80,
            stock_id="2330",
            inst_rows=self._build_strong_pos_inst(),
            margin_rows=_margin_rows_linear(100_000, 95_000, 22),
        )
        out1 = make_chip_forecast(series, **kwargs)
        out2 = make_chip_forecast(series, **kwargs)
        assert out1["params_hash"] == out2["params_hash"]
