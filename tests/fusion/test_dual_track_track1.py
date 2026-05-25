"""Tests for src/fusion/dual_track/track1.py — 軌道一(結構)讀法。

對齊 m3Spec/dual_track_resonance.md §三 + §六。
"""

from __future__ import annotations

import sys
from datetime import date, timedelta
from pathlib import Path
from unittest.mock import patch

import pytest

_REPO_ROOT = Path(__file__).resolve().parent.parent.parent
_SRC_ROOT = _REPO_ROOT / "src"
for p in (str(_SRC_ROOT), str(_REPO_ROOT)):
    if p not in sys.path:
        sys.path.insert(0, p)

from fusion.dual_track._shared import FibLine, Track1View  # noqa: E402
from fusion.dual_track.track1 import (  # noqa: E402
    read_track1,
    scenario_is_invalidated,
    _extract_invalidation_price,
    _zone_to_fib_line,
    _pick_primary,
    _effective_degree,
    _direction_from_power,
)


def _make_scenario(
    *,
    pattern_type="Impulse",
    power="StrongBullish",
    rules_passed=5,
    span_days=200,
    structure_label="5-wave from mw1 to mw5",
    fib_zones=None,
    invalidation_triggers=None,
):
    return {
        "pattern_type": pattern_type,
        "power_rating": power,
        "rules_passed_count": rules_passed,
        "structure_label": structure_label,
        "wave_tree": {
            "start": str(date(2024, 1, 1)),
            "end": str(date(2024, 1, 1) + timedelta(days=span_days)),
        },
        "expected_fib_zones": fib_zones or [],
        "invalidation_triggers": invalidation_triggers or [],
    }


def _make_snapshot(scenarios, *, flat=None):
    return {
        "snapshot_date": date(2024, 6, 1),
        "snapshot": {
            "scenario_forest": scenarios,
            "flat_fib_zones": flat or [],
        },
        "timeframe": "daily",
        "core_name": "neely_core",
    }


# ─── Direction / Degree ──────────────────────────────────────────────────────


class TestDirectionFromPower:
    def test_strong_bullish(self):
        assert _direction_from_power("StrongBullish") == "bullish"

    def test_bearish(self):
        assert _direction_from_power("Bearish") == "bearish"

    def test_slight_bullish(self):
        assert _direction_from_power("SlightBullish") == "bullish"

    def test_neutral(self):
        assert _direction_from_power("Neutral") == "neutral"

    def test_serde_tagged_dict(self):
        # serde tagged enum 可能是 {"StrongBullish": ...}
        assert _direction_from_power({"StrongBearish": {}}) == "bearish"

    def test_none(self):
        assert _direction_from_power(None) == "neutral"


class TestEffectiveDegree:
    def test_short_span(self):
        s = _make_scenario(span_days=30)
        assert _effective_degree(s) == "Subminuette"

    def test_year_span(self):
        s = _make_scenario(span_days=400)
        assert _effective_degree(s) == "Minute"

    def test_no_wave_tree(self):
        assert _effective_degree({}) is None


# ─── Picker ──────────────────────────────────────────────────────────────────


class TestPicker:
    def test_higher_degree_wins(self):
        short = _make_scenario(span_days=30, power="StrongBullish")
        long_ = _make_scenario(span_days=2000, power="Bearish")
        assert _pick_primary([short, long_]) is long_

    def test_empty_returns_none(self):
        assert _pick_primary([]) is None


# ─── Fib zone extraction ─────────────────────────────────────────────────────


class TestZoneToFibLine:
    def test_basic(self):
        line = _zone_to_fib_line({"low": 90.0, "high": 100.0, "label": "0.618",
                                   "source_ratio": 0.618})
        assert line is not None
        assert line.price == 95.0
        assert line.low == 90.0
        assert line.high == 100.0
        assert line.label == "0.618"
        assert line.source_ratio == 0.618

    def test_missing_low_returns_none(self):
        assert _zone_to_fib_line({"high": 100.0}) is None

    def test_string_low_returns_none(self):
        # 對齊「資料污染」防呆
        assert _zone_to_fib_line({"low": "x", "high": 100.0}) is None

    def test_bool_rejected(self):
        # bool 是 int 子類,須擋掉
        assert _zone_to_fib_line({"low": True, "high": 100.0}) is None


# ─── Invalidation ────────────────────────────────────────────────────────────


class TestInvalidation:
    def test_bullish_invalidated_below_threshold(self):
        s = _make_scenario(
            invalidation_triggers=[{
                "on_trigger": "InvalidateScenario",
                "trigger_type": {"PriceBreakBelow": 80.0},
            }]
        )
        assert _extract_invalidation_price(s, "bullish") == 80.0
        assert scenario_is_invalidated(
            direction="bullish", invalidation_price=80.0, current_price=75.0
        ) is True

    def test_bullish_not_invalidated_above(self):
        assert scenario_is_invalidated(
            direction="bullish", invalidation_price=80.0, current_price=85.0
        ) is False

    def test_bearish_invalidated_above_threshold(self):
        s = _make_scenario(
            power="Bearish",
            invalidation_triggers=[{
                "on_trigger": "InvalidateScenario",
                "trigger_type": {"PriceBreakAbove": 120.0},
            }]
        )
        assert _extract_invalidation_price(s, "bearish") == 120.0
        assert scenario_is_invalidated(
            direction="bearish", invalidation_price=120.0, current_price=125.0
        ) is True

    def test_weaken_action_ignored(self):
        """只認 InvalidateScenario,WeakenScenario / PromoteAlternative 不算。"""
        s = _make_scenario(
            invalidation_triggers=[{
                "on_trigger": "WeakenScenario",
                "trigger_type": {"PriceBreakBelow": 80.0},
            }]
        )
        assert _extract_invalidation_price(s, "bullish") is None

    def test_missing_current_returns_false(self):
        assert scenario_is_invalidated(
            direction="bullish", invalidation_price=80.0, current_price=None
        ) is False


# ─── read_track1 整合測試 ────────────────────────────────────────────────────


class TestReadTrack1:
    def test_no_snapshot(self):
        with patch("fusion.dual_track.track1.fetch_structural_latest", return_value=[]):
            t1 = read_track1(None, stock_id="2330", as_of=date(2024, 6, 1))
        assert t1.has_snapshot is False
        assert t1.fib_lines == []
        assert "no neely_core" in t1.notes[0]

    def test_empty_forest(self):
        snap = _make_snapshot([])
        with patch("fusion.dual_track.track1.fetch_structural_latest", return_value=[snap]):
            t1 = read_track1(None, stock_id="2330", as_of=date(2024, 6, 1))
        assert t1.has_snapshot is True
        assert t1.fib_lines == []
        assert t1.pattern_type is None

    def test_basic_emit_with_fib_lines(self):
        primary = _make_scenario(
            span_days=400,
            fib_zones=[
                {"label": "0.382", "low": 88.0, "high": 92.0, "source_ratio": 0.382},
                {"label": "0.618", "low": 95.0, "high": 105.0, "source_ratio": 0.618},
            ],
            invalidation_triggers=[{
                "on_trigger": "InvalidateScenario",
                "trigger_type": {"PriceBreakBelow": 80.0},
            }],
        )
        snap = _make_snapshot([primary])
        with patch("fusion.dual_track.track1.fetch_structural_latest", return_value=[snap]):
            t1 = read_track1(None, stock_id="2330", as_of=date(2024, 6, 1),
                              current_price=85.0)
        assert t1.has_snapshot is True
        assert t1.pattern_type == "Impulse"
        assert t1.direction == "bullish"
        assert t1.effective_degree == "Minute"
        assert t1.wave_count == 5  # 從 structure_label "5-wave ..." parse
        assert len(t1.fib_lines) == 2
        # 升序
        assert t1.fib_lines[0].price == 90.0
        assert t1.fib_lines[1].price == 100.0
        assert t1.invalidation_price == 80.0
        assert t1.invalidated is False  # 85 > 80
        assert t1.fallback_to_flat_union is False

    def test_invalidation_gate_triggered(self):
        primary = _make_scenario(
            fib_zones=[{"label": "0.5", "low": 90.0, "high": 110.0}],
            invalidation_triggers=[{
                "on_trigger": "InvalidateScenario",
                "trigger_type": {"PriceBreakBelow": 80.0},
            }],
        )
        snap = _make_snapshot([primary])
        with patch("fusion.dual_track.track1.fetch_structural_latest", return_value=[snap]):
            t1 = read_track1(None, stock_id="2330", as_of=date(2024, 6, 1),
                              current_price=75.0)
        assert t1.invalidated is True
        assert any("A-3 invalidation gate" in n for n in t1.notes)

    def test_fallback_to_flat_union(self):
        # primary 無 zones,flat_fib_zones 有
        primary = _make_scenario(fib_zones=[])
        snap = _make_snapshot([primary], flat=[
            {"label": "u_0.382", "low": 88.0, "high": 92.0, "source_ratio": 0.382},
        ])
        with patch("fusion.dual_track.track1.fetch_structural_latest", return_value=[snap]):
            t1 = read_track1(None, stock_id="2330", as_of=date(2024, 6, 1))
        assert t1.fallback_to_flat_union is True
        assert len(t1.fib_lines) == 1
        assert t1.fib_lines[0].price == 90.0

    def test_picks_correct_timeframe(self):
        """fetch_structural_latest 回 daily / weekly 兩筆 → 只取 daily。"""
        daily_primary = _make_scenario(
            span_days=400,
            fib_zones=[{"label": "d", "low": 90.0, "high": 100.0}],
        )
        weekly_primary = _make_scenario(
            span_days=2000,
            fib_zones=[{"label": "w", "low": 200.0, "high": 300.0}],
        )
        daily_snap = {**_make_snapshot([daily_primary]), "timeframe": "daily"}
        weekly_snap = {**_make_snapshot([weekly_primary]), "timeframe": "weekly"}
        with patch("fusion.dual_track.track1.fetch_structural_latest",
                   return_value=[daily_snap, weekly_snap]):
            t1 = read_track1(None, stock_id="2330", as_of=date(2024, 6, 1),
                              timeframe="daily")
        assert len(t1.fib_lines) == 1
        assert t1.fib_lines[0].label == "d"
