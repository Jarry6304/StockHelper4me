"""Tests for src/forecast/neely_emitter.py — Neely fib zone emitter."""

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

from forecast.neely_emitter import (  # noqa: E402
    emit_neely_fib,
    _effective_degree,
    _scenario_horizon_days,
    _pick_primary,
)


def _make_scenario(
    *,
    pattern="Impulse",
    power="Strong",
    rules_passed=5,
    span_days=200,
    fib_zones=None,
):
    return {
        "pattern_type": pattern,
        "power_rating": power,
        "rules_passed_count": rules_passed,
        "wave_tree": {
            "start": str(date(2024, 1, 1)),
            "end": str(date(2024, 1, 1) + __import__("datetime").timedelta(days=span_days)),
        },
        "expected_fib_zones": fib_zones or [],
    }


class TestEffectiveDegree:
    def test_short_span_is_subminuette(self):
        s = _make_scenario(span_days=30)  # ~0.08 year
        assert _effective_degree(s) == "Subminuette"

    def test_year_span_is_minute(self):
        s = _make_scenario(span_days=400)  # ~1.1 year
        assert _effective_degree(s) == "Minute"

    def test_decade_span_is_primary(self):
        s = _make_scenario(span_days=4000)  # ~11 years
        assert _effective_degree(s) == "Primary"

    def test_no_wave_tree_returns_none(self):
        assert _effective_degree({}) is None


class TestHorizonMapping:
    def test_subminuette_maps_to_21(self):
        s = _make_scenario(span_days=50)
        assert _scenario_horizon_days(s) == 21

    def test_minute_maps_to_63(self):
        s = _make_scenario(span_days=400)
        assert _scenario_horizon_days(s) == 63

    def test_minor_maps_to_126(self):
        s = _make_scenario(span_days=2000)  # ~5.5 years
        assert _scenario_horizon_days(s) == 126


class TestPicker:
    def test_higher_degree_wins(self):
        short = _make_scenario(span_days=30, power="Strong")    # Subminuette
        long_ = _make_scenario(span_days=2000, power="Moderate")  # Minor
        primary = _pick_primary([short, long_])
        # Long-span wins on degree, even though short has higher power
        assert primary is long_

    def test_same_degree_power_wins(self):
        a = _make_scenario(span_days=2000, power="Strong")
        b = _make_scenario(span_days=2000, power="Weak")
        assert _pick_primary([a, b]) is a

    def test_empty_forest_returns_none(self):
        assert _pick_primary([]) is None


class TestEmitNeelyFib:
    def test_no_snapshot_returns_status(self):
        with patch("forecast.neely_emitter._fetch_latest_neely_snapshot", return_value=None):
            res = emit_neely_fib(None, "2330", date(2024, 6, 1))
        assert res["status"] == "no_snapshot"
        assert res["zones_emitted"] == 0

    def test_empty_forest_returns_status(self):
        snap = {
            "snapshot_date": date(2024, 5, 30),
            "snapshot": {"scenario_forest": []},
        }
        with patch("forecast.neely_emitter._fetch_latest_neely_snapshot", return_value=snap), \
             patch("forecast.neely_emitter.upsert_forecast") as upd:
            res = emit_neely_fib(None, "2330", date(2024, 6, 1))
        assert res["status"] == "empty_forest"
        upd.assert_not_called()

    def test_emits_envelope_row(self):
        # Primary scenario with 3 fib zones
        primary = _make_scenario(
            span_days=400,  # Minute → horizon 63
            fib_zones=[
                {"label": "0.382", "low": 90.0, "high": 95.0},
                {"label": "0.618", "low": 92.0, "high": 100.0},
                {"label": "1.000", "low": 105.0, "high": 115.0},
            ],
        )
        snap = {
            "snapshot_date": date(2024, 5, 30),
            "snapshot": {"scenario_forest": [primary]},
        }
        written: list[dict] = []
        with patch("forecast.neely_emitter._fetch_latest_neely_snapshot", return_value=snap), \
             patch("forecast.neely_emitter.upsert_forecast",
                   side_effect=lambda conn, row: written.append(row)):
            res = emit_neely_fib(None, "2330", date(2024, 6, 1))
        assert res["status"] == "written"
        assert res["horizon_days"] == 63
        assert res["envelope"] == (90.0, 115.0)  # min/max across all zones
        assert res["primary_pattern"] == "Impulse"
        # One row, envelope encloses all zones
        assert len(written) == 1
        row = written[0]
        assert row["source_core"] == "neely_fib"
        assert row["calibrated"] is False
        # v1.0 dual_track (alembic f2g3h4i5j6k7): neely_fib emit 必標 internal_only=True
        # 對齊 m3Spec/dual_track_resonance.md §六 + §七.2(B-4 機制丙)
        assert row["internal_only"] is True
        assert row["regime_tag"] == "Impulse"
        assert row["lower"] == 90.0
        assert row["upper"] == 115.0
        assert "neely_fib" in row["params_hash"]

    def test_overwrite_horizon_param(self):
        primary = _make_scenario(
            span_days=400,
            fib_zones=[{"label": "0.5", "low": 90.0, "high": 100.0}],
        )
        snap = {"snapshot_date": date(2024, 5, 30),
                "snapshot": {"scenario_forest": [primary]}}
        written = []
        with patch("forecast.neely_emitter._fetch_latest_neely_snapshot", return_value=snap), \
             patch("forecast.neely_emitter.upsert_forecast",
                   side_effect=lambda conn, row: written.append(row)):
            res = emit_neely_fib(None, "2330", date(2024, 6, 1), overwrite_horizon=126)
        assert res["horizon_days"] == 126
        assert written[0]["horizon_days"] == 126

    def test_fallback_to_flat_union_when_primary_empty(self):
        # primary scenario has no fib zones, but top-level flat_fib_zones populated
        # (v4.11+ neely_core output structure)
        primary = _make_scenario(span_days=400, fib_zones=[])
        snap = {
            "snapshot_date": date(2024, 5, 30),
            "snapshot": {
                "scenario_forest": [primary],
                "flat_fib_zones": [
                    {"label": "u_0.382", "low": 88.0, "high": 92.0},
                    {"label": "u_0.618", "low": 95.0, "high": 105.0},
                ],
            },
        }
        written = []
        with patch("forecast.neely_emitter._fetch_latest_neely_snapshot", return_value=snap), \
             patch("forecast.neely_emitter.upsert_forecast",
                   side_effect=lambda conn, row: written.append(row)):
            res = emit_neely_fib(None, "2330", date(2024, 6, 1))
        assert res["status"] == "written"
        assert res["fallback_to_flat_union"] is True
        assert res["envelope"] == (88.0, 105.0)
        assert "source=flat_union" in written[0]["params_hash"]

    def test_no_fib_zones_when_both_primary_and_flat_empty(self):
        primary = _make_scenario(span_days=400, fib_zones=[])
        snap = {
            "snapshot_date": date(2024, 5, 30),
            "snapshot": {"scenario_forest": [primary], "flat_fib_zones": []},
        }
        with patch("forecast.neely_emitter._fetch_latest_neely_snapshot", return_value=snap), \
             patch("forecast.neely_emitter.upsert_forecast") as upd:
            res = emit_neely_fib(None, "2330", date(2024, 6, 1))
        assert res["status"] == "no_fib_zones"
        upd.assert_not_called()
