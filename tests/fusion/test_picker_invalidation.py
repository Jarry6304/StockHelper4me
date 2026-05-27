"""B1:fusion/_picker.canonical_is_invalidated tests.

對齊 b1-degree-consolidation skill「驗收標準」:
- bullish 只看 PriceBreakBelow(current < threshold → True)
- bearish 只看 PriceBreakAbove(current > threshold → True)
- bullish 忽略 PriceBreakAbove(direction-aware)
- **neutral 不濾**(永遠 False)
- current_price=None → 永遠 False(不靜默放行)

Rust canonical:`rust_compute/cores/wave/neely_core/src/triggers/mod.rs`
  Up pattern → PriceBreakBelow / Down pattern → PriceBreakAbove
"""

from __future__ import annotations

import sys
from pathlib import Path

_REPO_ROOT = Path(__file__).resolve().parent.parent.parent
_SRC_ROOT = _REPO_ROOT / "src"
for _p in (str(_SRC_ROOT), str(_REPO_ROOT)):
    if _p not in sys.path:
        sys.path.insert(0, _p)

from fusion._picker import canonical_is_invalidated  # noqa: E402


def _scenario_with_triggers(
    power_rating: str,
    triggers: list[dict],
) -> dict:
    """Build minimal scenario dict for invalidation tests."""
    return {
        "power_rating": power_rating,
        "invalidation_triggers": triggers,
    }


def _below_trigger(threshold: float) -> dict:
    return {
        "on_trigger": "InvalidateScenario",
        "trigger_type": {"PriceBreakBelow": threshold},
    }


def _above_trigger(threshold: float) -> dict:
    return {
        "on_trigger": "InvalidateScenario",
        "trigger_type": {"PriceBreakAbove": threshold},
    }


# ────────────────────────────────────────────────────────────
# bullish:看 PriceBreakBelow,忽略 PriceBreakAbove
# ────────────────────────────────────────────────────────────


class TestBullish:
    def test_invalidated_when_price_below_break_threshold(self):
        scenario = _scenario_with_triggers("Bullish", [_below_trigger(100.0)])
        assert canonical_is_invalidated(scenario, current_price=95.0) is True

    def test_not_invalidated_when_above_threshold(self):
        scenario = _scenario_with_triggers("Bullish", [_below_trigger(100.0)])
        assert canonical_is_invalidated(scenario, current_price=105.0) is False

    def test_not_invalidated_exactly_at_threshold(self):
        # cp < threshold(嚴格不等);等於不算
        scenario = _scenario_with_triggers("Bullish", [_below_trigger(100.0)])
        assert canonical_is_invalidated(scenario, current_price=100.0) is False

    def test_ignores_above_trigger(self):
        """direction-aware:bullish 應忽略 PriceBreakAbove 即使被觸發。"""
        scenario = _scenario_with_triggers("Bullish", [_above_trigger(100.0)])
        assert canonical_is_invalidated(scenario, current_price=105.0) is False

    def test_strong_bullish_same_as_bullish(self):
        scenario = _scenario_with_triggers("StrongBullish", [_below_trigger(100.0)])
        assert canonical_is_invalidated(scenario, current_price=95.0) is True

    def test_slight_bullish_same_as_bullish(self):
        scenario = _scenario_with_triggers("SlightBullish", [_below_trigger(100.0)])
        assert canonical_is_invalidated(scenario, current_price=95.0) is True


# ────────────────────────────────────────────────────────────
# bearish:看 PriceBreakAbove,忽略 PriceBreakBelow
# ────────────────────────────────────────────────────────────


class TestBearish:
    def test_invalidated_when_price_above_break_threshold(self):
        scenario = _scenario_with_triggers("Bearish", [_above_trigger(100.0)])
        assert canonical_is_invalidated(scenario, current_price=105.0) is True

    def test_not_invalidated_when_below_threshold(self):
        scenario = _scenario_with_triggers("Bearish", [_above_trigger(100.0)])
        assert canonical_is_invalidated(scenario, current_price=95.0) is False

    def test_ignores_below_trigger(self):
        """direction-aware:bearish 應忽略 PriceBreakBelow 即使被觸發。"""
        scenario = _scenario_with_triggers("Bearish", [_below_trigger(100.0)])
        assert canonical_is_invalidated(scenario, current_price=95.0) is False


# ────────────────────────────────────────────────────────────
# neutral:不濾(永遠 False),即使 trigger 觸發
# ────────────────────────────────────────────────────────────


class TestNeutralNeverInvalidated:
    def test_neutral_with_below_trigger_breached_returns_false(self):
        """skill b1 spec 核心案例:neutral + PriceBreakBelow + 觸發 → False。

        對齊 b1 設計理念:neutral scenario 屬資訊性、非方向性 bet,
        write-side 不應 filter 掉(與 v4.25.x track1.scenario_is_invalidated
        的 neutral=ALL-kinds 行為不同;讀取面 track1 留 B3 統一)。
        """
        scenario = _scenario_with_triggers("Neutral", [_below_trigger(100.0)])
        assert canonical_is_invalidated(scenario, current_price=95.0) is False

    def test_neutral_with_above_trigger_breached_returns_false(self):
        scenario = _scenario_with_triggers("Neutral", [_above_trigger(100.0)])
        assert canonical_is_invalidated(scenario, current_price=105.0) is False

    def test_neutral_with_both_triggers_breached_returns_false(self):
        scenario = _scenario_with_triggers(
            "Neutral",
            [_below_trigger(100.0), _above_trigger(110.0)],
        )
        # cp 介於兩者間反方向各自觸發,neutral 都不算
        assert canonical_is_invalidated(scenario, current_price=95.0) is False
        assert canonical_is_invalidated(scenario, current_price=115.0) is False


# ────────────────────────────────────────────────────────────
# current_price=None → False(不靜默放行 — caller 控 filter 跳過)
# ────────────────────────────────────────────────────────────


class TestCurrentPriceNone:
    def test_none_returns_false_for_bullish(self):
        scenario = _scenario_with_triggers("Bullish", [_below_trigger(100.0)])
        assert canonical_is_invalidated(scenario, current_price=None) is False

    def test_none_returns_false_for_bearish(self):
        scenario = _scenario_with_triggers("Bearish", [_above_trigger(100.0)])
        assert canonical_is_invalidated(scenario, current_price=None) is False

    def test_none_returns_false_for_neutral(self):
        scenario = _scenario_with_triggers("Neutral", [_below_trigger(100.0)])
        assert canonical_is_invalidated(scenario, current_price=None) is False


# ────────────────────────────────────────────────────────────
# Edge cases:non-InvalidateScenario action / malformed triggers
# ────────────────────────────────────────────────────────────


class TestEdgeCases:
    def test_weaken_scenario_action_ignored(self):
        scenario = {
            "power_rating": "Bullish",
            "invalidation_triggers": [
                {
                    "on_trigger": "WeakenScenario",
                    "trigger_type": {"PriceBreakBelow": 100.0},
                }
            ],
        }
        # WeakenScenario 不應被視為 invalidate
        assert canonical_is_invalidated(scenario, current_price=95.0) is False

    def test_promote_alternative_action_ignored(self):
        scenario = {
            "power_rating": "Bullish",
            "invalidation_triggers": [
                {
                    "on_trigger": "PromoteAlternative",
                    "trigger_type": {"PriceBreakBelow": 100.0},
                }
            ],
        }
        assert canonical_is_invalidated(scenario, current_price=95.0) is False

    def test_empty_triggers_returns_false(self):
        scenario = _scenario_with_triggers("Bullish", [])
        assert canonical_is_invalidated(scenario, current_price=95.0) is False

    def test_missing_triggers_key_returns_false(self):
        scenario = {"power_rating": "Bullish"}
        assert canonical_is_invalidated(scenario, current_price=95.0) is False

    def test_action_as_dict_variant(self):
        """serde tagged enum 可能是 dict {"InvalidateScenario": null}。"""
        scenario = {
            "power_rating": "Bullish",
            "invalidation_triggers": [
                {
                    "on_trigger": {"InvalidateScenario": None},
                    "trigger_type": {"PriceBreakBelow": 100.0},
                }
            ],
        }
        assert canonical_is_invalidated(scenario, current_price=95.0) is True

    def test_power_rating_dict_variant(self):
        """power_rating 也可能是 dict variant。"""
        scenario = {
            "power_rating": {"Bullish": None},
            "invalidation_triggers": [_below_trigger(100.0)],
        }
        assert canonical_is_invalidated(scenario, current_price=95.0) is True

    def test_malformed_threshold_skipped(self):
        scenario = {
            "power_rating": "Bullish",
            "invalidation_triggers": [
                {
                    "on_trigger": "InvalidateScenario",
                    "trigger_type": {"PriceBreakBelow": "not_a_number"},
                }
            ],
        }
        # 無有效 threshold → 跳過 → 整體 False
        assert canonical_is_invalidated(scenario, current_price=95.0) is False
