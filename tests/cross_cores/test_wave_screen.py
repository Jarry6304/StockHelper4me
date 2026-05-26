"""Tests for wave_impulse_screen r3 (post-correction entry pivot)。

r3 pivot rationale(production verify 2026-05-27):
neely_core forest 全市場 1152 stocks **wave_count=5 100%** — 完全不 emit
incomplete Impulse,r1/r2 設計「找 incomplete W3」根本走不通。

r3 pivot:**改抓 3-wave Zigzag/Flat 修正剛完成 + 方向 DOWN 訊號**(對齊
NEoWave「A-B-C 結束後啟動新 impulse」),candidates 預期:
- CORRECTION_DONE_DOWN:多頭 entry candidate
- CORRECTION_DONE_UP:空頭 observe(TW 多單市場略過)
- IMPULSE_COMPLETE:完整 5 波 → reverse warning observe
- CORRECTION_ONGOING:修正中,observe

對齊 test_v3_32_builders.py MagicMock db.query 風格。
"""

from __future__ import annotations

import sys
from datetime import date
from pathlib import Path
from unittest.mock import MagicMock, patch

import pytest  # noqa: F401

_REPO_ROOT = Path(__file__).resolve().parent.parent.parent
_SRC_ROOT = _REPO_ROOT / "src"
for p in (str(_SRC_ROOT), str(_REPO_ROOT)):
    if p not in sys.path:
        sys.path.insert(0, p)


# ════════════════════════════════════════════════════════════
# Fixture helpers
# ════════════════════════════════════════════════════════════


SNAPSHOT_DATE = date(2026, 5, 25)


def _zigzag(*, end_date: str, direction: str = "down",
            children_extra: int = 2, **kw):
    """3-wave Zigzag scenario fixture。

    direction: "down" or "up" → rightmost child label arrow
    children_extra: 前 N 個 placeholder children(本 fixture 預設 W1+W2),
                    rightmost C-wave 用 end_date / direction arrow
    """
    arrow = "↓" if direction == "down" else "↑"
    children = [{"label": f"W{i+1}↑"} for i in range(children_extra)]
    children.append({"label": f"W{children_extra+1}:L5{arrow}", "end": end_date})
    return {
        "wave_tree": {
            "start": "2026-04-01", "end": end_date,
            "children": children,
        },
        "pattern_type": {"Zigzag": {"sub_kind": "Single"}},
        "power_rating": kw.get("power_rating", "Bullish"),
        "rules_passed_count": kw.get("rules_passed", 5),
        "monowave_structure_labels": kw.get("monowave_labels", []),
        "expected_fib_zones": kw.get("fib_zones",
                                       [{"source_ratio": 1.618, "low": 110, "high": 120}]),
        "invalidation_triggers": kw.get("triggers",
                                          [{"on_trigger": "InvalidateScenario",
                                            "trigger_type": {"PriceBreakBelow": 90.0}}]),
    }


def _flat(*, end_date: str, direction: str = "up"):
    arrow = "↑" if direction == "up" else "↓"
    return {
        "wave_tree": {
            "start": "2026-04-01", "end": end_date,
            "children": [{"label": "W1↑"}, {"label": "W2↓"},
                          {"label": f"W3:Five{arrow}", "end": end_date}],
        },
        "pattern_type": {"Flat": {"sub_kind": "Common"}},
        "power_rating": "Neutral",
        "rules_passed_count": 5,
        "monowave_structure_labels": [],
        "expected_fib_zones": [],
        "invalidation_triggers": [],
    }


def _impulse(*, end_date: str = "2026-05-15"):
    return {
        "wave_tree": {
            "start": "2024-01-01", "end": end_date,
            "children": [{"label": "W1↑"}, {"label": "W2↓"}, {"label": "W3↑"},
                          {"label": "W4↓"}, {"label": "W5:L5↑", "end": end_date}],
        },
        "pattern_type": "Impulse",
        "power_rating": "StrongBullish",
        "rules_passed_count": 7,
        "monowave_structure_labels": [],
        "expected_fib_zones": [],
        "invalidation_triggers": [],
    }


# ════════════════════════════════════════════════════════════
# current_wave_position(r3)
# ════════════════════════════════════════════════════════════


class TestCurrentWavePositionR3:

    def test_zigzag_down_recent_is_correction_done_down(self):
        """剛完成的 Zigzag down → CORRECTION_DONE_DOWN candidate。"""
        from cross_cores.wave_impulse_screen import current_wave_position

        s = _zigzag(end_date="2026-05-20", direction="down")
        pos = current_wave_position(s, SNAPSHOT_DATE)
        assert pos["phase"] == "CORRECTION_DONE_DOWN"
        assert pos["direction"] == "down"
        assert pos["is_candidate"] is True
        assert pos["days_since"] == 5

    def test_zigzag_up_recent_is_correction_done_up_observe(self):
        """Zigzag up → CORRECTION_DONE_UP observe(空頭 setup,r3 不入 candidate)。"""
        from cross_cores.wave_impulse_screen import current_wave_position

        s = _zigzag(end_date="2026-05-20", direction="up")
        pos = current_wave_position(s, SNAPSHOT_DATE)
        assert pos["phase"] == "CORRECTION_DONE_UP"
        assert pos["is_candidate"] is False
        assert pos["excluded_reason"] == "bearish_setup_observe_only"

    def test_flat_up_recent_is_correction_done_up(self):
        from cross_cores.wave_impulse_screen import current_wave_position

        s = _flat(end_date="2026-05-22", direction="up")
        pos = current_wave_position(s, SNAPSHOT_DATE)
        assert pos["phase"] == "CORRECTION_DONE_UP"

    def test_zigzag_too_old_is_correction_ongoing(self):
        """rightmost > RECENT_DAYS 前 → CORRECTION_ONGOING observe。"""
        from cross_cores.wave_impulse_screen import current_wave_position

        s = _zigzag(end_date="2026-04-10", direction="down")  # 45 days ago
        pos = current_wave_position(s, SNAPSHOT_DATE)
        assert pos["phase"] == "CORRECTION_ONGOING"
        assert pos["is_candidate"] is False

    def test_impulse_complete_is_impulse_complete(self):
        """完整 5 波 Impulse → IMPULSE_COMPLETE observe(反轉警示)。"""
        from cross_cores.wave_impulse_screen import current_wave_position

        s = _impulse()
        pos = current_wave_position(s, SNAPSHOT_DATE)
        assert pos["phase"] == "IMPULSE_COMPLETE"
        assert pos["is_candidate"] is False

    def test_diagonal_falls_into_impulse_complete(self):
        """Diagonal 屬 impulse 系 → IMPULSE_COMPLETE。"""
        from cross_cores.wave_impulse_screen import current_wave_position

        s = _impulse()
        s["pattern_type"] = {"Diagonal": {"Ending": None}}
        pos = current_wave_position(s, SNAPSHOT_DATE)
        assert pos["phase"] == "IMPULSE_COMPLETE"

    def test_triangle_is_other(self):
        """Triangle 不是 Zigzag/Flat/Impulse/Diagonal → OTHER。"""
        from cross_cores.wave_impulse_screen import current_wave_position

        s = _zigzag(end_date="2026-05-20", direction="down")
        s["pattern_type"] = {"Triangle": {"sub_kind": "Contracting"}}
        pos = current_wave_position(s, SNAPSHOT_DATE)
        assert pos["phase"] == "OTHER"
        assert "non_corrective_pattern" in pos["excluded_reason"]

    def test_combination_is_other(self):
        from cross_cores.wave_impulse_screen import current_wave_position

        s = _zigzag(end_date="2026-05-20", direction="down")
        s["pattern_type"] = {"Combination": {"sub_kinds": []}}
        pos = current_wave_position(s, SNAPSHOT_DATE)
        assert pos["phase"] == "OTHER"

    def test_no_direction_arrow_is_other(self):
        from cross_cores.wave_impulse_screen import current_wave_position

        s = _zigzag(end_date="2026-05-20", direction="down")
        # 改 rightmost label 去掉方向 arrow
        s["wave_tree"]["children"][-1]["label"] = "W3"
        pos = current_wave_position(s, SNAPSHOT_DATE)
        assert pos["excluded_reason"] == "no_direction"


# ════════════════════════════════════════════════════════════
# _pick_recent_correction(r3)
# ════════════════════════════════════════════════════════════


class TestPickRecentCorrection:

    def test_picks_most_recent_correction(self):
        from cross_cores.wave_impulse_screen import _pick_recent_correction

        old = _zigzag(end_date="2026-05-10", direction="down")     # 15 days, OUT of RECENT_DAYS=14
        recent = _zigzag(end_date="2026-05-22", direction="down")   # 3 days
        picked = _pick_recent_correction([old, recent], SNAPSHOT_DATE)
        assert picked is recent

    def test_fallback_to_impulse_when_no_recent_correction(self):
        from cross_cores.wave_impulse_screen import _pick_recent_correction

        too_old = _zigzag(end_date="2026-04-10", direction="down")
        imp = _impulse()
        picked = _pick_recent_correction([too_old, imp], SNAPSHOT_DATE)
        assert picked is imp

    def test_picks_flat_or_zigzag_either(self):
        """Flat / Zigzag 都算 correction。"""
        from cross_cores.wave_impulse_screen import _pick_recent_correction

        flat = _flat(end_date="2026-05-22", direction="down")
        picked = _pick_recent_correction([flat], SNAPSHOT_DATE)
        assert picked is flat

    def test_recent_within_14_days_boundary(self):
        from cross_cores.wave_impulse_screen import _pick_recent_correction

        # 14 days exactly = boundary
        s_14 = _zigzag(end_date="2026-05-11", direction="down")
        picked = _pick_recent_correction([s_14], SNAPSHOT_DATE)
        assert picked is s_14   # 14 == RECENT_DAYS,inclusive

    def test_empty_forest_returns_none(self):
        from cross_cores.wave_impulse_screen import _pick_recent_correction

        assert _pick_recent_correction([], SNAPSHOT_DATE) is None

    def test_ties_break_by_degree_then_power(self):
        """Same end_date,(degree↓, power↓) 排。"""
        from cross_cores.wave_impulse_screen import _pick_recent_correction

        # 同 end_date 2026-05-22,但 weak 短 span / strong 長 span
        weak = _zigzag(end_date="2026-05-22", direction="down",
                       power_rating="SlightBullish", rules_passed=3)
        weak["wave_tree"]["start"] = "2026-05-01"   # 短 span → Subminuette
        strong = _zigzag(end_date="2026-05-22", direction="down",
                         power_rating="StrongBullish", rules_passed=6)
        strong["wave_tree"]["start"] = "2025-01-01"  # 長 span → Minor
        picked = _pick_recent_correction([weak, strong], SNAPSHOT_DATE)
        assert picked is strong


# ════════════════════════════════════════════════════════════
# Pattern_kind gate(r3)
# ════════════════════════════════════════════════════════════


class TestPatternKindGate:

    def test_zigzag_is_correction(self):
        from cross_cores.wave_impulse_screen import _pattern_kind_ok

        ok, label = _pattern_kind_ok({"pattern_type": {"Zigzag": {"sub_kind": "Single"}}})
        assert ok is True
        assert label == "Zigzag"

    def test_flat_is_correction(self):
        from cross_cores.wave_impulse_screen import _pattern_kind_ok

        ok, label = _pattern_kind_ok({"pattern_type": {"Flat": {"sub_kind": "Common"}}})
        assert ok is True

    def test_impulse_not_correction(self):
        """Impulse 不算 correction(走 IMPULSE_COMPLETE 路徑)。"""
        from cross_cores.wave_impulse_screen import _pattern_kind_ok, _pattern_is_impulse

        s = {"pattern_type": "Impulse"}
        ok, _ = _pattern_kind_ok(s)
        assert ok is False
        assert _pattern_is_impulse(s) is True

    def test_diagonal_is_impulse_not_correction(self):
        from cross_cores.wave_impulse_screen import _pattern_kind_ok, _pattern_is_impulse

        s = {"pattern_type": {"Diagonal": {"Leading": None}}}
        ok, _ = _pattern_kind_ok(s)
        assert ok is False
        assert _pattern_is_impulse(s) is True


# ════════════════════════════════════════════════════════════
# Direction parser
# ════════════════════════════════════════════════════════════


class TestDirectionParser:

    def test_parse_down_arrow(self):
        from cross_cores.wave_impulse_screen import _parse_direction

        assert _parse_direction("W3:L5↓") == "down"

    def test_parse_up_arrow(self):
        from cross_cores.wave_impulse_screen import _parse_direction

        assert _parse_direction("W3:Five↑") == "up"

    def test_parse_text_down(self):
        from cross_cores.wave_impulse_screen import _parse_direction

        assert _parse_direction("3-wave Down") == "down"

    def test_parse_text_up(self):
        from cross_cores.wave_impulse_screen import _parse_direction

        assert _parse_direction("3-wave Up") == "up"

    def test_parse_no_direction(self):
        from cross_cores.wave_impulse_screen import _parse_direction

        assert _parse_direction("W3") is None
        assert _parse_direction("") is None


# ════════════════════════════════════════════════════════════
# _build_row(r3 end-to-end)
# ════════════════════════════════════════════════════════════


class TestBuildRowR3:

    def test_correction_done_down_with_rr_passes(self):
        from cross_cores.wave_impulse_screen import _build_row

        s = _zigzag(end_date="2026-05-22", direction="down")
        row = _build_row(
            stock_id="X", target_date=SNAPSHOT_DATE, timeframe="daily",
            snapshot={"scenario_forest": [s]}, current_price=100.0,
            excluded_reason=None, snapshot_date=SNAPSHOT_DATE,
        )
        assert row["phase"] == "CORRECTION_DONE_DOWN"
        assert row["is_candidate"] is True
        assert row["direction"] == "down"
        assert row["rr_ratio"] is not None and row["rr_ratio"] >= 1.5
        assert row["entry_price"] == 100.0
        # detail 含 rightmost_end + days_since_completion
        assert row["detail"]["rightmost_end"] == "2026-05-22"
        assert row["detail"]["days_since_completion"] == 3

    def test_impulse_complete_emits_observe_row(self):
        from cross_cores.wave_impulse_screen import _build_row

        row = _build_row(
            stock_id="X", target_date=SNAPSHOT_DATE, timeframe="daily",
            snapshot={"scenario_forest": [_impulse()]}, current_price=100.0,
            excluded_reason=None, snapshot_date=SNAPSHOT_DATE,
        )
        assert row["phase"] == "IMPULSE_COMPLETE"
        assert row["is_candidate"] is False
        assert row["excluded_reason"] == "impulse_complete_observe"

    def test_rr_below_threshold_demoted(self):
        from cross_cores.wave_impulse_screen import _build_row

        # r7:upside 必須 ≥ 3% 才考慮 R/R;
        # 用 target=104.5(4.5% upside > 3% MIN_UPSIDE)+ inv 92(stop too loose)
        # → rr=(104.5-100)/(100-92)=4.5/8=0.56 < 1.5
        s = _zigzag(
            end_date="2026-05-22", direction="down",
            fib_zones=[{"source_ratio": 1.618, "low": 103, "high": 106}],
            triggers=[{"on_trigger": "InvalidateScenario",
                       "trigger_type": {"PriceBreakBelow": 92.0}}],
        )
        row = _build_row(
            stock_id="X", target_date=SNAPSHOT_DATE, timeframe="daily",
            snapshot={"scenario_forest": [s]}, current_price=100.0,
            excluded_reason=None, snapshot_date=SNAPSHOT_DATE,
        )
        assert row["is_candidate"] is False
        assert row["excluded_reason"] == "rr_below_threshold"

    def test_upside_too_small_demoted(self):
        """r7:upside < 3% → upside_too_small。"""
        from cross_cores.wave_impulse_screen import _build_row

        # midpoint = 102.5,upside = 2.5% < 3%
        s = _zigzag(end_date="2026-05-22", direction="down",
                    fib_zones=[{"source_ratio": 1.618, "low": 100.01, "high": 105}])
        row = _build_row(
            stock_id="X", target_date=SNAPSHOT_DATE, timeframe="daily",
            snapshot={"scenario_forest": [s]}, current_price=100.0,
            excluded_reason=None, snapshot_date=SNAPSHOT_DATE,
        )
        assert row["is_candidate"] is False
        assert row["excluded_reason"] == "upside_too_small"

    def test_target_below_current_filtered_to_no_target(self):
        """r6 後 _extract_reversal_target_upside 預先 filter mid <= current,
        所以 target=None → excluded='no_target'(r4 的 target_below_current 分支
        在 _build_row 仍保留但實務不會走到)。"""
        from cross_cores.wave_impulse_screen import _build_row

        s = _zigzag(end_date="2026-05-22", direction="down",
                    fib_zones=[{"source_ratio": 1.618, "low": 75, "high": 85}])
        row = _build_row(
            stock_id="X", target_date=SNAPSHOT_DATE, timeframe="daily",
            snapshot={"scenario_forest": [s]}, current_price=100.0,
            excluded_reason=None, snapshot_date=SNAPSHOT_DATE,
        )
        assert row["is_candidate"] is False
        assert row["excluded_reason"] == "no_target"

    def test_stop_above_current_excluded(self):
        """r4 geometry sanity:invalidation ≥ current → 不入 candidate
        (對應 r3 揭露的 237 個 invalid_rr_geometry root cause)。"""
        from cross_cores.wave_impulse_screen import _build_row

        # invalidation 110 > current 100(MAX 取錯方向會踩這個)
        s = _zigzag(
            end_date="2026-05-22", direction="down",
            fib_zones=[{"source_ratio": 1.618, "low": 110, "high": 120}],
            triggers=[{"on_trigger": "InvalidateScenario",
                       "trigger_type": {"PriceBreakBelow": 110.0}}],
        )
        row = _build_row(
            stock_id="X", target_date=SNAPSHOT_DATE, timeframe="daily",
            snapshot={"scenario_forest": [s]}, current_price=100.0,
            excluded_reason=None, snapshot_date=SNAPSHOT_DATE,
        )
        assert row["is_candidate"] is False
        assert row["excluded_reason"] == "stop_above_current"

    def test_multiple_below_triggers_takes_min(self):
        """r4:對 corrective bottom 取 MIN of below triggers(loosest stop)。
        對應 r3 揭露的 NEoWave 對 Zigzag/Flat emit 多筆 triggers,MAX 抓錯方向。"""
        from cross_cores.wave_impulse_screen import _extract_correction_stop

        s = {
            "invalidation_triggers": [
                {"on_trigger": "InvalidateScenario",
                 "trigger_type": {"PriceBreakBelow": 110.0}},   # 較高(MAX 會抓)
                {"on_trigger": "InvalidateScenario",
                 "trigger_type": {"PriceBreakBelow": 90.0}},    # 較低(MIN 抓,= corrective bottom)
                {"on_trigger": "InvalidateScenario",
                 "trigger_type": {"PriceBreakBelow": 95.0}},
            ],
        }
        assert _extract_correction_stop(s) == 90.0   # MIN

    def test_no_invalidation_excluded(self):
        """r4 invalidation trigger 不存在 → no_invalidation(r5 之後會 rescore)。"""
        from cross_cores.wave_impulse_screen import _build_row

        s = _zigzag(end_date="2026-05-22", direction="down",
                    triggers=[])   # 無 invalidation trigger
        row = _build_row(
            stock_id="X", target_date=SNAPSHOT_DATE, timeframe="daily",
            snapshot={"scenario_forest": [s]}, current_price=100.0,
            excluded_reason=None, snapshot_date=SNAPSHOT_DATE,
        )
        assert row["is_candidate"] is False
        assert row["excluded_reason"] == "no_invalidation"


# ════════════════════════════════════════════════════════════
# r5 corrective bottom lookup + rescore
# ════════════════════════════════════════════════════════════


class TestR5CorrectiveBottomRescore:

    def test_rescore_lifts_no_invalidation_to_candidate(self):
        """r5:no_invalidation row + price_daily_fwd close lookup → 升 candidate。"""
        from cross_cores.wave_impulse_screen import _populate_corrective_bottoms_and_rescore

        rows = [
            {"stock_id": "A", "phase": "CORRECTION_DONE_DOWN",
             "excluded_reason": "no_invalidation",
             "entry_price": 100.0, "target_price": 130.0,
             "detail": {"rightmost_end": "2026-05-22"},
             "invalidation_price": None, "rr_ratio": None, "is_candidate": False},
        ]
        db = MagicMock()
        db.query = MagicMock(return_value=[
            {"stock_id": "A", "date": date(2026, 5, 22), "close": 90.0},
        ])
        _populate_corrective_bottoms_and_rescore(db, rows)
        # r7:invalidation = 90 × (1 - 0.03) = 87.3;current=100;target=130
        # rr = (130-100)/(100-87.3) = 30/12.7 ≈ 2.36;upside = 30% ✓
        assert rows[0]["is_candidate"] is True
        assert rows[0]["invalidation_price"] == 87.3
        assert rows[0]["rr_ratio"] is not None and rows[0]["rr_ratio"] > 2.0
        assert rows[0]["excluded_reason"] is None
        assert rows[0]["detail"]["invalidation_source"] == "price_daily_fwd_at_rightmost_end"

    def test_rescore_skips_non_correction_done_down(self):
        """r5:其他 phase 的 row 不動。"""
        from cross_cores.wave_impulse_screen import _populate_corrective_bottoms_and_rescore

        rows = [
            {"stock_id": "X", "phase": "IMPULSE_COMPLETE",
             "excluded_reason": "impulse_complete_observe", "detail": {}},
            {"stock_id": "Y", "phase": "CORRECTION_DONE_UP",
             "excluded_reason": "bearish_setup_observe_only", "detail": {}},
        ]
        db = MagicMock()
        db.query = MagicMock(return_value=[])
        _populate_corrective_bottoms_and_rescore(db, rows)
        # 沒任何改動
        assert rows[0]["excluded_reason"] == "impulse_complete_observe"
        assert rows[1]["excluded_reason"] == "bearish_setup_observe_only"
        db.query.assert_not_called()   # 沒 lookup 需求 → 不打 DB

    def test_rescore_no_price_data_marked(self):
        """r5:price_daily_fwd 查不到 close → no_price_at_correction_end。"""
        from cross_cores.wave_impulse_screen import _populate_corrective_bottoms_and_rescore

        rows = [
            {"stock_id": "Z", "phase": "CORRECTION_DONE_DOWN",
             "excluded_reason": "no_invalidation",
             "entry_price": 100.0, "target_price": 130.0,
             "detail": {"rightmost_end": "2026-05-22"},
             "invalidation_price": None, "rr_ratio": None, "is_candidate": False},
        ]
        db = MagicMock()
        # 模擬 LATERAL JOIN 找不到 → close=None
        db.query = MagicMock(return_value=[
            {"stock_id": "Z", "date": date(2026, 5, 22), "close": None},
        ])
        _populate_corrective_bottoms_and_rescore(db, rows)
        assert rows[0]["excluded_reason"] == "no_price_at_correction_end"
        assert rows[0]["is_candidate"] is False

    def test_rescore_stop_above_current_demoted(self):
        """r5:corrective bottom × 0.99 仍 ≥ current → stop_above_current。"""
        from cross_cores.wave_impulse_screen import _populate_corrective_bottoms_and_rescore

        rows = [
            {"stock_id": "Q", "phase": "CORRECTION_DONE_DOWN",
             "excluded_reason": "no_invalidation",
             "entry_price": 100.0, "target_price": 130.0,
             "detail": {"rightmost_end": "2026-05-22"},
             "invalidation_price": None, "rr_ratio": None, "is_candidate": False},
        ]
        db = MagicMock()
        # bottom=150 → invalidation=150*0.99=148.5 > current=100
        db.query = MagicMock(return_value=[
            {"stock_id": "Q", "date": date(2026, 5, 22), "close": 150.0},
        ])
        _populate_corrective_bottoms_and_rescore(db, rows)
        assert rows[0]["excluded_reason"] == "stop_above_current"
        assert rows[0]["is_candidate"] is False

    def test_rescore_rr_below_threshold_demoted(self):
        """r5:rr 後 < RR_MIN(1.5) → rr_below_threshold。"""
        from cross_cores.wave_impulse_screen import _populate_corrective_bottoms_and_rescore

        rows = [
            {"stock_id": "R", "phase": "CORRECTION_DONE_DOWN",
             "excluded_reason": "no_invalidation",
             "entry_price": 100.0, "target_price": 105.0,    # small target
             "detail": {"rightmost_end": "2026-05-22"},
             "invalidation_price": None, "rr_ratio": None, "is_candidate": False},
        ]
        db = MagicMock()
        # bottom=95:invalidation=94.05;rr=(105-100)/(100-94.05)=5/5.95=0.84
        db.query = MagicMock(return_value=[
            {"stock_id": "R", "date": date(2026, 5, 22), "close": 95.0},
        ])
        _populate_corrective_bottoms_and_rescore(db, rows)
        assert rows[0]["excluded_reason"] == "rr_below_threshold"
        assert rows[0]["rr_ratio"] is not None and rows[0]["rr_ratio"] < 1.5
        assert rows[0]["is_candidate"] is False


# ════════════════════════════════════════════════════════════
# r6 reversal target upside(nearest fib zone above current,with max_multiple cap)
# ════════════════════════════════════════════════════════════


class TestR6ReversalTargetUpside:

    def test_nearest_upside_picked(self):
        from cross_cores.wave_impulse_screen import _extract_reversal_target_upside

        s = {"expected_fib_zones": [
            {"low": 5, "high": 7},      # mid=6,below current 10,skip
            {"low": 11, "high": 13},    # mid=12,upside candidate
            {"low": 15, "high": 17},    # mid=16,upside further
            {"low": 30, "high": 40},    # mid=35 > 10×2 cap,skip
        ]}
        assert _extract_reversal_target_upside(s, 10.0) == 12.0

    def test_no_upside_returns_none(self):
        """Production 233 個 target_below_current 對應「fib zones 都 < current」。"""
        from cross_cores.wave_impulse_screen import _extract_reversal_target_upside

        s = {"expected_fib_zones": [{"low": 3, "high": 5}, {"low": 6, "high": 8}]}
        assert _extract_reversal_target_upside(s, 10.0) is None

    def test_outlier_blocked_by_max_multiple(self):
        """Production 7780 case:fib midpoint 880 vs current 18 → 47x outlier。"""
        from cross_cores.wave_impulse_screen import _extract_reversal_target_upside

        s = {"expected_fib_zones": [
            {"low": 20, "high": 22},      # mid=21,within 18×2=36
            {"low": 800, "high": 960},    # mid=880,>>36 → skip
        ]}
        assert _extract_reversal_target_upside(s, 18.0) == 21.0

    def test_current_price_none_returns_none(self):
        from cross_cores.wave_impulse_screen import _extract_reversal_target_upside

        assert _extract_reversal_target_upside({"expected_fib_zones": []}, None) is None

    def test_empty_zones_returns_none(self):
        from cross_cores.wave_impulse_screen import _extract_reversal_target_upside

        assert _extract_reversal_target_upside({}, 10.0) is None

    def test_boundary_inclusive_upper(self):
        """target = current × max_multiple 邊界 inclusive。"""
        from cross_cores.wave_impulse_screen import _extract_reversal_target_upside

        s = {"expected_fib_zones": [{"low": 19, "high": 21}]}   # mid=20 = 10×2
        assert _extract_reversal_target_upside(s, 10.0) == 20.0

    def test_strictly_above_current(self):
        """mid == current → 不算 upside(strict >)。"""
        from cross_cores.wave_impulse_screen import _extract_reversal_target_upside

        s = {"expected_fib_zones": [{"low": 9, "high": 11}]}   # mid=10 = current
        assert _extract_reversal_target_upside(s, 10.0) is None

    def test_custom_max_multiple(self):
        from cross_cores.wave_impulse_screen import _extract_reversal_target_upside

        s = {"expected_fib_zones": [{"low": 49, "high": 51}]}   # mid=50 = 5x
        assert _extract_reversal_target_upside(s, 10.0) is None  # 2x default
        assert _extract_reversal_target_upside(s, 10.0, max_multiple=5.0) == 50.0

    def test_no_target_demoted(self):
        from cross_cores.wave_impulse_screen import _build_row

        s = _zigzag(end_date="2026-05-22", direction="down", fib_zones=[])
        row = _build_row(
            stock_id="X", target_date=SNAPSHOT_DATE, timeframe="daily",
            snapshot={"scenario_forest": [s]}, current_price=100.0,
            excluded_reason=None, snapshot_date=SNAPSHOT_DATE,
        )
        assert row["is_candidate"] is False
        assert row["excluded_reason"] == "no_target"

    def test_empty_forest_no_crash(self):
        from cross_cores.wave_impulse_screen import _build_row

        row = _build_row(
            stock_id="X", target_date=SNAPSHOT_DATE, timeframe="daily",
            snapshot={"scenario_forest": []}, current_price=100.0,
            excluded_reason=None, snapshot_date=SNAPSHOT_DATE,
        )
        assert row["is_candidate"] is False
        assert row["excluded_reason"] == "empty_forest"

    def test_universe_excluded_passed_through(self):
        from cross_cores.wave_impulse_screen import _build_row

        row = _build_row(
            stock_id="2880", target_date=SNAPSHOT_DATE, timeframe="daily",
            snapshot={"scenario_forest": []}, current_price=100.0,
            excluded_reason="financial", snapshot_date=SNAPSHOT_DATE,
        )
        assert row["excluded_reason"] == "financial"

    def test_no_snapshot_date_excluded(self):
        from cross_cores.wave_impulse_screen import _build_row

        # target_date 是非法字串 + snapshot_date None
        row = _build_row(
            stock_id="X", target_date="not_a_date", timeframe="daily",
            snapshot={"scenario_forest": [_zigzag(end_date="2026-05-22")]},
            current_price=100.0,
            excluded_reason=None, snapshot_date=None,
        )
        assert row["excluded_reason"] == "no_snapshot_date"


# ════════════════════════════════════════════════════════════
# Cross-TF aligned(r3 phase 集合適配)
# ════════════════════════════════════════════════════════════


class TestCrossTFAlignment:

    def test_aligned_daily_weekly_same_correction_done_down(self):
        from cross_cores.wave_impulse_screen import _apply_cross_tf_alignment

        rows = [
            {"stock_id": "A", "timeframe": "daily", "phase": "CORRECTION_DONE_DOWN",
             "direction": "down", "cross_tf_aligned": False},
            {"stock_id": "A", "timeframe": "weekly", "phase": "CORRECTION_DONE_DOWN",
             "direction": "down", "cross_tf_aligned": False},
        ]
        _apply_cross_tf_alignment(rows)
        for r in rows:
            assert r["cross_tf_aligned"] is True

    def test_not_aligned_when_phase_diverge(self):
        from cross_cores.wave_impulse_screen import _apply_cross_tf_alignment

        rows = [
            {"stock_id": "A", "timeframe": "daily", "phase": "CORRECTION_DONE_DOWN",
             "direction": "down", "cross_tf_aligned": False},
            {"stock_id": "A", "timeframe": "weekly", "phase": "IMPULSE_COMPLETE",
             "direction": "down", "cross_tf_aligned": False},
        ]
        _apply_cross_tf_alignment(rows)
        for r in rows:
            assert r["cross_tf_aligned"] is False


# ════════════════════════════════════════════════════════════
# Ranking(r3)
# ════════════════════════════════════════════════════════════


class TestRanking:

    def test_assign_ranks_by_rr(self):
        from cross_cores.wave_impulse_screen import _assign_impulse_ranks

        rows = [
            {"stock_id": "A", "timeframe": "daily", "is_candidate": True,
             "rr_ratio": 2.5, "cross_tf_aligned": False, "detail": {"power_strength": 2}},
            {"stock_id": "B", "timeframe": "daily", "is_candidate": True,
             "rr_ratio": 1.8, "cross_tf_aligned": True, "detail": {"power_strength": 3}},
            {"stock_id": "C", "timeframe": "daily", "is_candidate": True,
             "rr_ratio": 3.2, "cross_tf_aligned": False, "detail": {"power_strength": 1}},
        ]
        _assign_impulse_ranks(rows)
        ranks = {r["stock_id"]: r["impulse_rank"] for r in rows}
        assert ranks["B"] == 1   # cross_tf=True 優先
        assert ranks["C"] == 2
        assert ranks["A"] == 3


# ════════════════════════════════════════════════════════════
# End-to-end run() smoke(MagicMock db)
# ════════════════════════════════════════════════════════════


class TestRunSmoke:

    def test_run_empty_db_graceful(self):
        from cross_cores import wave_impulse_screen as mod

        db = MagicMock()
        db.query = MagicMock(return_value=[])
        result = mod.run(db)
        assert result["rows_read"] == 0
        assert result["rows_written"] == 0

    def test_run_emits_rows_for_universe(self):
        from cross_cores import wave_impulse_screen as mod

        snap = {"scenario_forest": [_zigzag(end_date="2026-05-22", direction="down")]}
        db = MagicMock()

        def _query(sql, params=None):
            sql_low = sql.lower().replace("\n", " ")
            if "stock_info_ref" in sql_low:
                return [{"stock_id": "2330",
                         "industry_category": "半導體",
                         "delisting_date": None}]
            if "price_daily_fwd" in sql_low:
                return [{"stock_id": "2330", "close": 100.0}]
            if "structural_snapshots" in sql_low:
                return [{"stock_id": "2330", "snapshot_date": date(2026, 5, 25),
                         "timeframe": "daily", "snapshot": snap},
                        {"stock_id": "2330", "snapshot_date": date(2026, 5, 25),
                         "timeframe": "weekly", "snapshot": snap},
                        {"stock_id": "2330", "snapshot_date": date(2026, 5, 25),
                         "timeframe": "monthly", "snapshot": snap}]
            return []

        db.query = MagicMock(side_effect=_query)
        with patch("cross_cores.wave_impulse_screen.upsert_silver", return_value=3):
            result = mod.run(db)
        assert result["rows_read"] == 3
        assert result["candidates"] >= 1


# ════════════════════════════════════════════════════════════
# Builder contract
# ════════════════════════════════════════════════════════════


def test_builder_contract():
    from cross_cores import wave_impulse_screen as m

    assert m.NAME == "wave_impulse_screen"
    assert m.OUTPUT_TABLE == "wave_impulse_screen_derived"
    assert "structural_snapshots" in m.UPSTREAM_TABLES
    assert callable(m.run)
    # r3 phase constants exposed
    assert m.PHASE_CORRECTION_DONE_DOWN == "CORRECTION_DONE_DOWN"
    assert m.RECENT_DAYS == 14


def test_registered_in_orchestrator():
    from cross_cores.orchestrator import BUILDERS

    assert "wave_impulse_screen" in BUILDERS
