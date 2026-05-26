"""Tests for fusion/_picker.py shared NEoWave picker helpers (v4.26 follow-up)。

對齊 v4.25.x + v4.26 揭露的 track1.py / _forecast.py drift,本檔抽出共用
helpers 後的 unit tests。

範圍:純 helper coverage(coerce_date / power_rating_* / pattern_type_label /
wave_count_from_label / direction_from_power)。

Out of scope:
- effective_degree / DEGREE_RANK / pick_primary(track1 vs _forecast 仍有
  semantic drift,待獨立 audit PR)
- scenario_is_invalidated(v4.25.x direction-aware vs _forecast ANY-trigger,
  drift 已揭露)
"""

from __future__ import annotations

import sys
from datetime import date, datetime
from pathlib import Path

import pytest  # noqa: F401

_REPO_ROOT = Path(__file__).resolve().parent.parent.parent
for p in (str(_REPO_ROOT / "src"), str(_REPO_ROOT)):
    if p not in sys.path:
        sys.path.insert(0, p)

from fusion._picker import (  # noqa: E402
    coerce_date,
    direction_from_power,
    pattern_type_label,
    power_rating_label,
    power_rating_sign,
    power_rating_strength,
    wave_count_from_label,
)


# ════════════════════════════════════════════════════════════
# coerce_date
# ════════════════════════════════════════════════════════════


class TestCoerceDate:

    def test_iso_string(self):
        assert coerce_date("2026-05-25") == date(2026, 5, 25)

    def test_iso_with_time_suffix(self):
        # track1.py 風格:首 10 字元 fallback
        assert coerce_date("2026-05-25T12:34:56") == date(2026, 5, 25)

    def test_date_object_passthrough(self):
        d = date(2026, 5, 25)
        assert coerce_date(d) is d

    def test_datetime_object_via_fallback(self):
        # datetime.fromisoformat fallback path
        dt = datetime(2026, 5, 25, 12, 0)
        # datetime 也是 date 子類 → 直接 return
        assert coerce_date(dt) is dt   # date subclass passthrough

    def test_invalid_returns_none(self):
        assert coerce_date("not-a-date") is None

    def test_none_input(self):
        assert coerce_date(None) is None

    def test_int_input_returns_none(self):
        assert coerce_date(20260525) is None


# ════════════════════════════════════════════════════════════
# power_rating_label
# ════════════════════════════════════════════════════════════


class TestPowerRatingLabel:

    def test_string_passthrough(self):
        assert power_rating_label("Bullish") == "Bullish"

    def test_dict_takes_first_key(self):
        assert power_rating_label({"StrongBullish": None}) == "StrongBullish"

    def test_empty_dict_defaults_neutral(self):
        assert power_rating_label({}) == "Neutral"

    def test_none_defaults_neutral(self):
        assert power_rating_label(None) == "Neutral"

    def test_int_defaults_neutral(self):
        assert power_rating_label(42) == "Neutral"


# ════════════════════════════════════════════════════════════
# power_rating_strength
# ════════════════════════════════════════════════════════════


class TestPowerRatingStrength:

    @pytest.mark.parametrize("rating,expected", [
        ("StrongBullish", 3),
        ("StrongBearish", 3),
        ("Bullish",       2),
        ("Bearish",       2),
        ("SlightBullish", 1),
        ("SlightBearish", 1),
        ("Neutral",       0),
        ("Unknown",       0),
    ])
    def test_string_variants(self, rating, expected):
        assert power_rating_strength(rating) == expected

    def test_dict_variant(self):
        assert power_rating_strength({"StrongBullish": None}) == 3

    def test_none_returns_zero(self):
        assert power_rating_strength(None) == 0

    def test_empty_returns_zero(self):
        assert power_rating_strength("") == 0
        assert power_rating_strength({}) == 0


# ════════════════════════════════════════════════════════════
# power_rating_sign
# ════════════════════════════════════════════════════════════


class TestPowerRatingSign:

    @pytest.mark.parametrize("rating,expected", [
        ("StrongBullish",  +1),
        ("Bullish",        +1),
        ("SlightBullish",  +1),
        ("Neutral",         0),
        ("SlightBearish",  -1),
        ("Bearish",        -1),
        ("StrongBearish",  -1),
        ("Unknown",         0),
    ])
    def test_string_variants(self, rating, expected):
        assert power_rating_sign(rating) == expected

    def test_dict_variant(self):
        assert power_rating_sign({"StrongBearish": None}) == -1

    def test_none_returns_zero(self):
        assert power_rating_sign(None) == 0


# ════════════════════════════════════════════════════════════
# direction_from_power
# ════════════════════════════════════════════════════════════


class TestDirectionFromPower:

    @pytest.mark.parametrize("rating,expected", [
        ("StrongBullish",  "bullish"),
        ("Bullish",        "bullish"),
        ("SlightBullish",  "bullish"),
        ("Neutral",         "neutral"),
        ("SlightBearish",  "bearish"),
        ("StrongBearish",  "bearish"),
    ])
    def test_string_variants(self, rating, expected):
        assert direction_from_power(rating) == expected

    def test_dict_variant(self):
        assert direction_from_power({"Bullish": None}) == "bullish"

    def test_none_returns_neutral(self):
        assert direction_from_power(None) == "neutral"


# ════════════════════════════════════════════════════════════
# pattern_type_label
# ════════════════════════════════════════════════════════════


class TestPatternTypeLabel:

    def test_string_variant(self):
        assert pattern_type_label("Impulse") == "Impulse"

    def test_dict_variant(self):
        assert pattern_type_label({"Diagonal": {"Leading": None}}) == "Diagonal"

    def test_nested_combination(self):
        assert pattern_type_label(
            {"Combination": {"sub_kinds": []}}
        ) == "Combination"

    def test_none_returns_none(self):
        assert pattern_type_label(None) is None

    def test_empty_dict_returns_none(self):
        assert pattern_type_label({}) is None


# ════════════════════════════════════════════════════════════
# wave_count_from_label
# ════════════════════════════════════════════════════════════


class TestWaveCountFromLabel:

    def test_5_wave(self):
        assert wave_count_from_label("5-wave from mw27 to mw31") == 5

    def test_3_wave(self):
        assert wave_count_from_label("3-wave Zigzag in W4") == 3

    def test_7_wave(self):
        assert wave_count_from_label("7-wave Combination") == 7

    def test_no_match(self):
        assert wave_count_from_label("Impulse pattern") == 0

    def test_none_input(self):
        assert wave_count_from_label(None) == 0

    def test_empty_string(self):
        assert wave_count_from_label("") == 0


# ════════════════════════════════════════════════════════════
# Backward-compat:既有 caller imports 仍 work
# ════════════════════════════════════════════════════════════


def test_track1_imports_picker():
    """v4.26 follow-up:track1.py 從 _picker import 共用 helpers。"""
    from fusion.dual_track.track1 import (
        _direction_from_power,
        _pattern_type_label,
        _power_rating_label,
        _power_rating_strength,
        _wave_count_from_label,
        _coerce_date,
    )
    # 行為對齊 picker 共用版本
    assert _direction_from_power("Bullish") == "bullish"
    assert _pattern_type_label("Impulse") == "Impulse"
    assert _power_rating_label("Bullish") == "Bullish"
    assert _power_rating_strength("StrongBullish") == 3
    assert _wave_count_from_label("5-wave") == 5
    assert _coerce_date("2026-05-25") == date(2026, 5, 25)


def test_wave_impulse_screen_imports_picker():
    """v4.26 follow-up:wave_impulse_screen 改從 _picker import。"""
    from cross_cores.wave_impulse_screen import (
        _direction_from_power,
        _pattern_type_label,
        _power_rating_label,
        _power_rating_strength,
        _wave_count_from_label,
    )
    assert _direction_from_power("Bullish") == "bullish"


def test_forecast_imports_picker():
    """v4.26 follow-up:_forecast.py 從 _picker import _power_rating_* 3 個。"""
    from mcp_server._forecast import (
        _power_rating_label,
        _power_rating_strength,
        _power_rating_sign,
    )
    assert _power_rating_label("Bullish") == "Bullish"
    assert _power_rating_strength("StrongBullish") == 3
    assert _power_rating_sign("Bearish") == -1
