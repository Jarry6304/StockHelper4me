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
    _extract_all_invalidation_thresholds,
    _check_any_threshold_breached,
    _zone_to_fib_line,
    _pick_primary,
    _effective_degree,
    _direction_from_power,
    _cluster_and_cap_fib_lines,
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
        # B1:< 1 yr → "SubMinuette"(canonical 對齊 Rust output.rs::Degree
        # enum 大小寫;舊版「Subminuette」小寫 'm' 為 producer-side label drift)
        s = _make_scenario(span_days=30)
        assert _effective_degree(s) == "SubMinuette"

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


# ─── v4.25.x:neutral A-3 + 全 trigger 檢查 ───────────────────────────────────


class TestExtractAllThresholds:
    def test_extracts_both_kinds(self):
        s = _make_scenario(invalidation_triggers=[
            {"on_trigger": "InvalidateScenario", "trigger_type": {"PriceBreakBelow": 80.0}},
            {"on_trigger": "InvalidateScenario", "trigger_type": {"PriceBreakAbove": 120.0}},
        ])
        ts = _extract_all_invalidation_thresholds(s)
        assert ("below", 80.0) in ts
        assert ("above", 120.0) in ts
        assert len(ts) == 2

    def test_weaken_action_excluded(self):
        s = _make_scenario(invalidation_triggers=[
            {"on_trigger": "InvalidateScenario", "trigger_type": {"PriceBreakBelow": 80.0}},
            {"on_trigger": "WeakenScenario", "trigger_type": {"PriceBreakAbove": 120.0}},
        ])
        ts = _extract_all_invalidation_thresholds(s)
        assert ts == [("below", 80.0)]

    def test_empty_scenario(self):
        assert _extract_all_invalidation_thresholds(_make_scenario()) == []


class TestCheckAnyThresholdBreached:
    def test_no_thresholds_no_breach(self):
        breached, kind, th = _check_any_threshold_breached([], 100.0)
        assert breached is False and kind is None and th is None

    def test_below_breached(self):
        breached, kind, th = _check_any_threshold_breached([("below", 80.0)], 75.0)
        assert breached is True and kind == "below" and th == 80.0

    def test_below_not_breached(self):
        breached, kind, th = _check_any_threshold_breached([("below", 80.0)], 85.0)
        assert breached is False

    def test_above_breached(self):
        breached, kind, th = _check_any_threshold_breached([("above", 120.0)], 125.0)
        assert breached is True and kind == "above" and th == 120.0

    def test_first_match_wins(self):
        # current=75 → below 80 first, even if there's also above 120
        breached, kind, th = _check_any_threshold_breached(
            [("below", 80.0), ("above", 120.0)], 75.0
        )
        assert breached is True and kind == "below"

    def test_no_current_returns_false(self):
        breached, kind, th = _check_any_threshold_breached([("below", 80.0)], None)
        assert breached is False


class TestNeutralA3Gate:
    """B3:**neutral 不濾**(對齊 b1 canonical;v4.25.x 「neutral 走 ALL kinds」自此退役)。

    對齊 ETF / index proxy(如 0050)等無明顯多空偏見的 scenario:neutral 無方向性
    thesis,不可能被 invalidation trigger「破」。MCP / LLM 看到 mode: neutral
    (no directional thesis)即可,不應誤判 invalidated。
    """

    def test_neutral_with_below_trigger_not_invalidated(self):
        # B3:0050-like case → neutral + PriceBreakBelow 觸發 → 仍 not invalidated
        # 對齊 b1 「無方向性 thesis 不可能 invalidated」原則
        primary = _make_scenario(
            power="Neutral",
            invalidation_triggers=[{
                "on_trigger": "InvalidateScenario",
                "trigger_type": {"PriceBreakBelow": 544.45},
            }],
        )
        snap = _make_snapshot([primary])
        with patch("fusion.dual_track.track1.fetch_structural_latest", return_value=[snap]):
            t1 = read_track1(None, stock_id="0050", as_of=date(2024, 6, 1),
                              current_price=95.85)
        assert t1.direction == "neutral"
        # B3:neutral + trigger 仍 not invalidated
        assert t1.invalidated is False
        # narrative 不應出現「跌破」(neutral 不該 surface direction-aware warning)
        assert not any("跌破" in n for n in t1.notes)

    def test_neutral_with_above_trigger_not_invalidated(self):
        primary = _make_scenario(
            power="Neutral",
            invalidation_triggers=[{
                "on_trigger": "InvalidateScenario",
                "trigger_type": {"PriceBreakAbove": 200.0},
            }],
        )
        snap = _make_snapshot([primary])
        with patch("fusion.dual_track.track1.fetch_structural_latest", return_value=[snap]):
            t1 = read_track1(None, stock_id="X", as_of=date(2024, 6, 1),
                              current_price=250.0)
        assert t1.direction == "neutral"
        # B3:同上,neutral + above trigger 仍 not invalidated
        assert t1.invalidated is False
        assert not any("漲破" in n for n in t1.notes)

    def test_neutral_no_breach(self):
        # neutral + 無 trigger 觸發 → not invalidated(行為不變,仍 False)
        primary = _make_scenario(
            power="Neutral",
            invalidation_triggers=[{
                "on_trigger": "InvalidateScenario",
                "trigger_type": {"PriceBreakBelow": 80.0},
            }],
        )
        snap = _make_snapshot([primary])
        with patch("fusion.dual_track.track1.fetch_structural_latest", return_value=[snap]):
            t1 = read_track1(None, stock_id="X", as_of=date(2024, 6, 1),
                              current_price=100.0)
        assert t1.invalidated is False

    def test_bullish_ignores_above_trigger(self):
        """v4.25.x:bullish 只看 below(對齊 spec §四「跌破」字面 + production 不誤判)。

        原本 v4.25.x 草案擴成「bullish ANY-trigger」,但 production 2330 case
        揭露這會把正常 PriceBreakAbove(如 wave 5 extended)誤判為失效。回退
        為 direction-filtered:bullish 只看 below trigger。
        """
        primary = _make_scenario(
            power="StrongBullish",
            invalidation_triggers=[
                {"on_trigger": "InvalidateScenario",
                 "trigger_type": {"PriceBreakBelow": 80.0}},
                {"on_trigger": "InvalidateScenario",
                 "trigger_type": {"PriceBreakAbove": 200.0}},
            ],
        )
        snap = _make_snapshot([primary])
        with patch("fusion.dual_track.track1.fetch_structural_latest", return_value=[snap]):
            t1 = read_track1(None, stock_id="X", as_of=date(2024, 6, 1),
                              current_price=250.0)
        # bullish + above 200 → 應仍 False(bullish 只看 below trigger,
        # current 250 > below 80 → 不觸發 below)
        assert t1.invalidated is False

    def test_bullish_with_below_breach_still_invalidates(self):
        """bullish + PriceBreakBelow + current 跌破 → 仍 invalidated。"""
        primary = _make_scenario(
            power="StrongBullish",
            invalidation_triggers=[
                {"on_trigger": "InvalidateScenario",
                 "trigger_type": {"PriceBreakBelow": 80.0}},
                {"on_trigger": "InvalidateScenario",
                 "trigger_type": {"PriceBreakAbove": 200.0}},
            ],
        )
        snap = _make_snapshot([primary])
        with patch("fusion.dual_track.track1.fetch_structural_latest", return_value=[snap]):
            t1 = read_track1(None, stock_id="X", as_of=date(2024, 6, 1),
                              current_price=75.0)
        assert t1.invalidated is True

    def test_bearish_ignores_below_trigger(self):
        """bearish 只看 above(對齊 v4.25.x 回退 + spec §四 字面)。"""
        primary = _make_scenario(
            power="StrongBearish",
            invalidation_triggers=[
                {"on_trigger": "InvalidateScenario",
                 "trigger_type": {"PriceBreakBelow": 80.0}},
                {"on_trigger": "InvalidateScenario",
                 "trigger_type": {"PriceBreakAbove": 200.0}},
            ],
        )
        snap = _make_snapshot([primary])
        with patch("fusion.dual_track.track1.fetch_structural_latest", return_value=[snap]):
            t1 = read_track1(None, stock_id="X", as_of=date(2024, 6, 1),
                              current_price=70.0)
        # bearish + below 80 + current=70 → 不該 invalidated(bearish 不看 below)
        assert t1.invalidated is False


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


# ─── fib_lines cluster + cap(對齊 §六 MCP payload budget 防呆)─────────────


class TestClusterAndCapFibLines:
    def test_empty_returns_empty(self):
        out, n_raw, was_reduced = _cluster_and_cap_fib_lines([])
        assert out == [] and n_raw == 0 and was_reduced is False

    def test_under_max_no_change(self):
        """≤ max_count 不改動(0.5% 距離 < 1% bucket → cluster 會合一)。"""
        lines = [
            FibLine(price=p, low=p - 1, high=p + 1, label=f"L{i}", source_ratio=0.5)
            for i, p in enumerate([100.0, 105.0, 110.0])  # spacing > 1%
        ]
        out, n_raw, was_reduced = _cluster_and_cap_fib_lines(lines, max_count=30)
        assert len(out) == 3
        assert n_raw == 3
        assert was_reduced is False

    def test_cluster_within_1pct(self):
        """price 在 1% 內被合一(2330 case:99.5 / 100 / 100.3 → 1 cluster)。"""
        lines = [
            FibLine(price=99.5, low=99, high=100, label="a", source_ratio=0.382),
            FibLine(price=100.0, low=99.5, high=100.5, label="b", source_ratio=0.5),
            FibLine(price=100.3, low=99.8, high=100.8, label="c", source_ratio=0.618),
        ]
        out, n_raw, was_reduced = _cluster_and_cap_fib_lines(lines, max_count=30)
        assert len(out) == 1
        assert n_raw == 3
        assert was_reduced is True
        # cluster 後 label 含 "clustered(3)" + 合併標籤 + price 為 median
        assert "clustered(3)" in out[0].label
        assert out[0].price == 100.0
        # 範圍包含所有原 low/high
        assert out[0].low == 99.0
        assert out[0].high == 100.8

    def test_cap_after_clustering(self):
        """100 條全離散 1% 外的 fib_line → cluster 不縮 → cap 到 max_count。"""
        # 100 條,每條間隔 5% 確保不會被 cluster
        lines = [
            FibLine(price=100.0 * (1.05 ** i), low=99 * (1.05 ** i),
                     high=101 * (1.05 ** i), label=f"L{i}", source_ratio=0.5)
            for i in range(100)
        ]
        out, n_raw, was_reduced = _cluster_and_cap_fib_lines(lines, max_count=30)
        assert len(out) == 30
        assert n_raw == 100
        assert was_reduced is True
        # 取樣後應保留首尾範圍(等距取樣)
        assert out[0].price == lines[0].price

    def test_flat_union_production_case(self):
        """模擬 2330 production case:155 條 flat_union → cluster+cap 後 ≤ 30。"""
        # 155 條,price 落在 233-3031(對齊用戶實機 output)
        import random
        random.seed(42)
        lines = [
            FibLine(price=233.0 + i * (3031 - 233) / 154,
                     low=233.0 + i * (3031 - 233) / 154 - 5,
                     high=233.0 + i * (3031 - 233) / 154 + 5,
                     label=f"fib_{i}", source_ratio=0.5)
            for i in range(155)
        ]
        out, n_raw, was_reduced = _cluster_and_cap_fib_lines(lines, max_count=30)
        assert n_raw == 155
        assert len(out) <= 30
        assert was_reduced is True

    def test_read_track1_note_when_reduced(self):
        """fib_lines reduced 時 notes 應記錄 raw → final count。"""
        # primary 給 50 條離散 fib zones
        primary = _make_scenario(
            span_days=400,
            fib_zones=[
                {"label": f"L{i}", "low": 100.0 * (1.05 ** i) - 1,
                 "high": 100.0 * (1.05 ** i) + 1}
                for i in range(50)
            ],
        )
        snap = _make_snapshot([primary])
        with patch("fusion.dual_track.track1.fetch_structural_latest", return_value=[snap]):
            t1 = read_track1(None, stock_id="2330", as_of=date(2024, 6, 1))
        # cluster + cap 後應 ≤ 30
        assert len(t1.fib_lines) <= 30
        # notes 含 reduction message
        assert any("fib_lines reduced" in n and "50" in n for n in t1.notes)


# ════════════════════════════════════════════════════════════════════════════
# B3 cross-tool consistency:track1.invalidated 與 canonical_is_invalidated
# 對相同 (scenario, current_price) 必同結論(single source of truth)。
# ════════════════════════════════════════════════════════════════════════════


class TestB3CrossToolConsistency:
    """B3 統一保證:read_track1 的 `invalidated` 對任何 (direction, triggers,
    current_price) 必與 _picker.canonical_is_invalidated 同結論。
    """

    @pytest.mark.parametrize("power,trigger_kind,trigger_value,current_price,expected", [
        # bullish
        ("Bullish",      "PriceBreakBelow", 100.0,  95.0, True),
        ("Bullish",      "PriceBreakBelow", 100.0, 105.0, False),
        ("Bullish",      "PriceBreakAbove", 100.0, 105.0, False),   # bullish ignores above
        ("StrongBullish","PriceBreakBelow", 100.0,  95.0, True),
        # bearish
        ("Bearish",      "PriceBreakAbove", 100.0, 105.0, True),
        ("Bearish",      "PriceBreakAbove", 100.0,  95.0, False),
        ("Bearish",      "PriceBreakBelow", 100.0,  95.0, False),   # bearish ignores below
        # neutral(B3 重點:不濾,永遠 False)
        ("Neutral",      "PriceBreakBelow", 100.0,  95.0, False),
        ("Neutral",      "PriceBreakAbove", 100.0, 105.0, False),
    ])
    def test_track1_matches_canonical(
        self, power, trigger_kind, trigger_value, current_price, expected,
    ):
        from fusion._picker import canonical_is_invalidated

        primary = _make_scenario(
            power=power,
            invalidation_triggers=[{
                "on_trigger": "InvalidateScenario",
                "trigger_type": {trigger_kind: trigger_value},
            }],
        )
        snap = _make_snapshot([primary])

        # canonical(write-side / b1)
        canon_result = canonical_is_invalidated(primary, current_price)

        # track1 (read-side / B3)
        with patch("fusion.dual_track.track1.fetch_structural_latest", return_value=[snap]):
            t1 = read_track1(None, stock_id="TEST", as_of=date(2024, 6, 1),
                              current_price=current_price)

        # 兩端結論必相同 + 都等於 expected
        assert canon_result == expected, (
            f"canonical_is_invalidated({power}, {trigger_kind}={trigger_value}, "
            f"current={current_price}) = {canon_result}, expected {expected}"
        )
        assert t1.invalidated == expected, (
            f"track1.read({power}, {trigger_kind}={trigger_value}, "
            f"current={current_price}).invalidated = {t1.invalidated}, expected {expected}"
        )
        # cross-tool 一致(冗余 assertion 但 highlights 設計意圖)
        assert canon_result == t1.invalidated
