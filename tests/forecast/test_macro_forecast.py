"""Tests for src/forecast/macro_forecast.py — FX + business indicator drift core."""

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

from forecast.macro_forecast import (  # noqa: E402
    _compute_business_indicator_score,
    _compute_twd_momentum_score,
    _parse_color_score,
    make_macro_forecast,
)


def _build_fx_rows(start_rate: float, end_rate: float, n_days: int = 22):
    """Linear rate change between start and end, n_days entries (oldest first)."""
    base = date(2024, 1, 1)
    if n_days <= 1:
        return [{"date": base, "rate": start_rate}]
    return [
        {
            "date": base + timedelta(days=i),
            "rate": start_rate + (end_rate - start_rate) * (i / (n_days - 1)),
        }
        for i in range(n_days)
    ]


def _build_business_rows(color_seq: list[str]):
    base = date(2023, 1, 1)
    return [
        {
            "date": base + timedelta(days=30 * i),
            "monitoring_color": c,
            "leading_indicator": None,
            "coincident_indicator": None,
            "lagging_indicator": None,
            "monitoring": None,
            "report_date": None,
            "detail": None,
        }
        for i, c in enumerate(color_seq)
    ]


def _flat_price_series(n: int, start_close=100.0, vol_factor=0.002):
    """Synthetic series with small alternating noise for non-zero vol."""
    closes = []
    p = start_close
    for i in range(n):
        p *= math.exp(vol_factor * ((-1) ** i))
        closes.append({"date": date(2024, 1, 1), "asof_adj_close": p})
    return closes


class TestParseColorScore:
    @pytest.mark.parametrize("label,expected", [
        ("blue", -2), ("yellow_blue", -1), ("green", 0),
        ("yellow_red", 1), ("red", 2),
        ("B", -2), ("YB", -1), ("G", 0), ("YR", 1), ("R", 2),
        ("藍", -2), ("黃藍", -1), ("綠", 0), ("黃紅", 1), ("紅", 2),
        ("藍燈", -2), ("紅燈", 2),
        ("  Green  ", 0),
        ("invalid", None), ("", None), (None, None),
    ])
    def test_parse(self, label, expected):
        assert _parse_color_score(label) == expected


class TestTwdMomentumScore:
    def test_zero_score_when_flat(self):
        rows = _build_fx_rows(31.0, 31.0, n_days=22)
        score = _compute_twd_momentum_score(rows)
        assert score == pytest.approx(0.0, abs=1e-6)

    def test_positive_score_when_twd_weakens(self):
        # TWD/USD rate up 2% over 21d → saturate to +1
        rows = _build_fx_rows(31.0, 31.62, n_days=22)
        score = _compute_twd_momentum_score(rows)
        assert score == pytest.approx(1.0, abs=0.01)

    def test_negative_score_when_twd_strengthens(self):
        rows = _build_fx_rows(31.0, 30.38, n_days=22)  # -2%
        score = _compute_twd_momentum_score(rows)
        assert score == pytest.approx(-1.0, abs=0.01)

    def test_proportional_in_middle(self):
        rows = _build_fx_rows(31.0, 31.31, n_days=22)  # +1% ≈ 0.5 saturation
        score = _compute_twd_momentum_score(rows)
        assert score == pytest.approx(0.5, abs=0.02)

    def test_too_few_rows_returns_none(self):
        rows = _build_fx_rows(31.0, 31.5, n_days=10)
        assert _compute_twd_momentum_score(rows) is None

    def test_empty_returns_none(self):
        assert _compute_twd_momentum_score([]) is None


class TestBusinessIndicatorScore:
    def test_green_is_zero(self):
        rows = _build_business_rows(["B", "YB", "G"])
        assert _compute_business_indicator_score(rows) == pytest.approx(0.0)

    def test_red_is_plus_one(self):
        rows = _build_business_rows(["G", "YR", "R"])
        assert _compute_business_indicator_score(rows) == pytest.approx(1.0)

    def test_blue_is_minus_one(self):
        rows = _build_business_rows(["G", "YB", "B"])
        assert _compute_business_indicator_score(rows) == pytest.approx(-1.0)

    def test_yellow_blue_is_minus_half(self):
        rows = _build_business_rows(["G", "G", "YB"])
        assert _compute_business_indicator_score(rows) == pytest.approx(-0.5)

    def test_skips_invalid_uses_prior(self):
        rows = _build_business_rows(["G", "R", "unknown"])
        # Latest unparseable → fall back to "R" (+1)
        assert _compute_business_indicator_score(rows) == pytest.approx(1.0)

    def test_all_invalid_returns_none(self):
        rows = _build_business_rows(["foo", "bar"])
        assert _compute_business_indicator_score(rows) is None

    def test_empty_returns_none(self):
        assert _compute_business_indicator_score([]) is None


class TestMakeMacroForecast:
    def test_returns_none_no_signal(self):
        out = make_macro_forecast(
            _flat_price_series(120),
            forecast_date=date(2024, 6, 1),
            horizon=63,
            confidence=0.80,
            stock_id="2330",
            fx_rows=[],
            business_rows=[],
        )
        assert out is None

    def test_returns_none_without_price(self):
        out = make_macro_forecast(
            [],
            forecast_date=date(2024, 6, 1),
            horizon=63,
            confidence=0.80,
            stock_id="2330",
            fx_rows=_build_fx_rows(31.0, 31.31),
            business_rows=_build_business_rows(["G"]),
        )
        assert out is None

    def test_returns_shape_with_both_signals(self):
        out = make_macro_forecast(
            _flat_price_series(120),
            forecast_date=date(2024, 6, 1),
            horizon=63,
            confidence=0.80,
            stock_id="2330",
            fx_rows=_build_fx_rows(31.0, 31.31),  # +1% → twd=+0.5
            business_rows=_build_business_rows(["G", "YR"]),  # YR → biz=+0.5
        )
        assert out is not None
        for k in ["stock_id", "forecast_date", "horizon_days", "lower",
                  "upper", "point", "confidence", "calibrated", "source_core",
                  "regime_tag", "params_hash"]:
            assert k in out
        assert out["source_core"] == "macro_forecast_core"
        assert out["calibrated"] is False
        assert out["lower"] < out["upper"]
        assert "twd=" in out["regime_tag"]
        assert "biz=" in out["regime_tag"]

    def test_positive_macro_drives_drift_up(self):
        series = _flat_price_series(120)
        out = make_macro_forecast(
            series, date(2024, 6, 1), 63, 0.80,
            stock_id="2330",
            fx_rows=_build_fx_rows(31.0, 31.62),  # +1 twd
            business_rows=_build_business_rows(["R"]),  # +1 biz
        )
        cp = series[-1]["asof_adj_close"]
        assert out["point"] > cp

    def test_negative_macro_drives_drift_down(self):
        series = _flat_price_series(120)
        out = make_macro_forecast(
            series, date(2024, 6, 1), 63, 0.80,
            stock_id="2330",
            fx_rows=_build_fx_rows(31.0, 30.38),  # -1 twd
            business_rows=_build_business_rows(["B"]),  # -1 biz
        )
        cp = series[-1]["asof_adj_close"]
        assert out["point"] < cp

    def test_uses_only_fx_when_business_missing(self):
        series = _flat_price_series(120)
        out = make_macro_forecast(
            series, date(2024, 6, 1), 63, 0.80,
            stock_id="2330",
            fx_rows=_build_fx_rows(31.0, 31.62),  # +1 twd
            business_rows=[],
        )
        assert out is not None
        assert "twd=" in out["regime_tag"]
        assert "biz=" not in out["regime_tag"]

    def test_uses_only_business_when_fx_missing(self):
        series = _flat_price_series(120)
        out = make_macro_forecast(
            series, date(2024, 6, 1), 63, 0.80,
            stock_id="2330",
            fx_rows=[],
            business_rows=_build_business_rows(["R"]),
        )
        assert out is not None
        assert "biz=" in out["regime_tag"]
        assert "twd=" not in out["regime_tag"]

    def test_drift_capped(self):
        series = _flat_price_series(120)
        out = make_macro_forecast(
            series, date(2024, 6, 1), 252, 0.80,  # long horizon × max signal
            stock_id="2330",
            fx_rows=_build_fx_rows(31.0, 35.0),  # > saturate (>+1)
            business_rows=_build_business_rows(["R"]),
            drift_cap=0.10,
        )
        cp = series[-1]["asof_adj_close"]
        max_point = cp * 1.10 * (1 + 1e-6)
        assert out["point"] <= max_point

    def test_params_hash_deterministic(self):
        series = _flat_price_series(120)
        kwargs = dict(
            forecast_date=date(2024, 6, 1), horizon=63, confidence=0.80,
            stock_id="2330",
            fx_rows=_build_fx_rows(31.0, 31.31),
            business_rows=_build_business_rows(["G"]),
        )
        out1 = make_macro_forecast(series, **kwargs)
        out2 = make_macro_forecast(series, **kwargs)
        assert out1["params_hash"] == out2["params_hash"]
