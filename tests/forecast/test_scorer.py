"""Tests for src/forecast/scorer.py — pinball / sharpness / reliability."""

from __future__ import annotations

import sys
from pathlib import Path

import pytest

_REPO_ROOT = Path(__file__).resolve().parent.parent.parent
_SRC_ROOT = _REPO_ROOT / "src"
for p in (str(_SRC_ROOT), str(_REPO_ROOT)):
    if p not in sys.path:
        sys.path.insert(0, p)

from forecast.scorer import score, interval_pinball, quantile_pinball  # noqa: E402


def _row(*, realized, lower, upper, confidence, source_core="baseline", horizon=21,
         regime=None):
    pl = interval_pinball(realized, lower, upper, confidence)
    return {
        "realized_price": realized,
        "lower": lower,
        "upper": upper,
        "confidence": confidence,
        "hit": lower <= realized <= upper,
        "pinball_loss": pl,
        "source_core": source_core,
        "horizon_days": horizon,
        "regime_tag": regime,
    }


class TestPinballFormula:
    def test_quantile_pinball_above(self):
        # y > q, tau=0.05 → loss = (y - q) * 0.05
        assert quantile_pinball(realized=10, quantile_value=5, tau=0.05) == pytest.approx(0.25)

    def test_quantile_pinball_below(self):
        # y < q, tau=0.95 → loss = (q - y) * 0.05
        assert quantile_pinball(realized=5, quantile_value=10, tau=0.95) == pytest.approx(0.25)

    def test_interval_pinball_in_range_is_small(self):
        # realized inside [L, U] → both tails are "y above L" + "y below U"
        # 80% interval, alpha = 0.2, tau_lo = 0.1, tau_hi = 0.9
        loss = interval_pinball(realized=100, lower=90, upper=110, confidence=0.80)
        # lo: y > q (100 > 90), loss = (100-90)*0.1 = 1.0
        # hi: y < q (100 < 110), loss = (110-100)*0.1 = 1.0
        # total = 0.5 * (1 + 1) = 1.0
        assert loss == pytest.approx(1.0)


class TestScoreNoGroup:
    def test_empty_returns_zeros(self):
        out = score([])
        assert out["n"] == 0
        assert out["mean_pinball_loss"] is None
        assert out["sharpness"] is None
        assert out["reliability"] == []

    def test_all_hits_reliability_perfect(self):
        rows = [_row(realized=100, lower=90, upper=110, confidence=0.80) for _ in range(5)]
        out = score(rows)
        assert out["n"] == 5
        assert out["sharpness"] == pytest.approx(20.0)
        assert out["reliability"] == [(0.80, 1.0)]

    def test_all_misses_reliability_zero(self):
        rows = [_row(realized=200, lower=90, upper=110, confidence=0.80) for _ in range(5)]
        out = score(rows)
        assert out["reliability"] == [(0.80, 0.0)]

    def test_multi_confidence_grouped_reliability(self):
        rows = [
            _row(realized=100, lower=90, upper=110, confidence=0.80),     # hit
            _row(realized=100, lower=99, upper=101, confidence=0.50),     # hit
            _row(realized=200, lower=99, upper=101, confidence=0.50),     # miss
        ]
        out = score(rows)
        rel = dict(out["reliability"])
        assert rel[0.80] == pytest.approx(1.0)
        assert rel[0.50] == pytest.approx(0.5)


class TestScoreGrouped:
    def test_group_by_source_core(self):
        rows = [
            _row(realized=100, lower=90, upper=110, confidence=0.80, source_core="baseline"),
            _row(realized=100, lower=99, upper=101, confidence=0.80, source_core="baseline"),
            _row(realized=200, lower=99, upper=101, confidence=0.80, source_core="kalman_cqr"),
        ]
        out = score(rows, group_by="source_core")
        assert set(out.keys()) == {"baseline", "kalman_cqr"}
        assert out["baseline"]["n"] == 2
        assert out["kalman_cqr"]["n"] == 1
        # baseline: both hit → empirical_coverage = 1.0
        assert out["baseline"]["reliability"] == [(0.80, 1.0)]
        # kalman_cqr: 1 miss → 0.0
        assert out["kalman_cqr"]["reliability"] == [(0.80, 0.0)]

    def test_group_by_regime_tag_handles_none(self):
        rows = [
            _row(realized=100, lower=90, upper=110, confidence=0.80, regime="5wave"),
            _row(realized=100, lower=99, upper=101, confidence=0.80, regime=None),
        ]
        out = score(rows, group_by="regime_tag")
        assert "5wave" in out
        assert None in out

    def test_invalid_group_by_raises(self):
        with pytest.raises(ValueError):
            score([], group_by="nope")
