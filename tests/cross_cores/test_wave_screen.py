"""Tests for wave_impulse_screen builder (commit 4/7)。

對齊 plan §9 Tests:
  1. axis_a_only_W2_done (monowave 空 → loose)
  2. axis_b_strict_L5_W3_mature (W3+L5 → mature, not candidate)
  3. axis_b_F3_W2_done (W2+F3 → candidate)
  4. W5_emit_row_not_candidate (W5 phase emit row 但 candidate=False)
  5. pattern_type_diagonal_included (Diagonal 走 impulse 系)
  6. pattern_type_zigzag_filtered (Zigzag → OTHER + excluded_reason)
  7. rr_below_threshold_demoted (rr < 1.5 → excluded)
  8. cross_tf_aligned_daily_weekly_same_dir (both True)
  9. cross_tf_aligned_false_when_dir_diverge (daily bullish + weekly bearish)
 10. empty_forest_no_crash (graceful)
 11. universe_filter_exclusion (金融股)
 12. assign_ranks_by_rr (eligible 多 stock 按 rr_ratio↓ sort)

對齊 test_v3_32_builders.py MagicMock db.query 風格。
"""

from __future__ import annotations

import sys
from datetime import date
from pathlib import Path
from unittest.mock import MagicMock

import pytest  # noqa: F401

_REPO_ROOT = Path(__file__).resolve().parent.parent.parent
_SRC_ROOT = _REPO_ROOT / "src"
for p in (str(_SRC_ROOT), str(_REPO_ROOT)):
    if p not in sys.path:
        sys.path.insert(0, p)


# ════════════════════════════════════════════════════════════
# Helper:組合 scenario fixture(對齊 production JSON shape)
# ════════════════════════════════════════════════════════════


def _scenario(
    *,
    wave_tree_label: str = "5-wave",
    children_labels: list[str] | None = None,
    pattern_type: dict | str = "Impulse",
    power_rating: str = "Bullish",
    monowave_labels: dict[int, str] | None = None,
    expected_fib_zones: list[dict] | None = None,
    invalidation_triggers: list[dict] | None = None,
    rules_passed: int = 7,
) -> dict:
    """Build production-shaped Scenario dict。

    monowave_labels: {monowave_index: label_string} → 自動組 structure_label_candidates
    """
    children = [{"label": lbl} for lbl in (children_labels or [])]
    msl: list[dict] = []
    for idx, lbl in (monowave_labels or {}).items():
        msl.append({
            "monowave_index": idx,
            "labels": [{"label": lbl, "certainty": "Primary"}] if lbl else [],
        })
    return {
        "wave_tree": {
            "label": wave_tree_label,
            "start": "2023-01-01",
            "end": "2026-05-15",
            "children": children,
        },
        "pattern_type": pattern_type if isinstance(pattern_type, dict)
                        else {pattern_type: None},
        "power_rating": power_rating,
        "monowave_structure_labels": msl,
        "expected_fib_zones": expected_fib_zones or [],
        "invalidation_triggers": invalidation_triggers or [],
        "structure_label": wave_tree_label,
        "rules_passed_count": rules_passed,
    }


# ════════════════════════════════════════════════════════════
# current_wave_position
# ════════════════════════════════════════════════════════════


class TestCurrentWavePosition:

    def test_axis_a_only_W2_done(self):
        """plan §9 case 1:monowave_structure_labels 空 → loose fallback。"""
        from cross_cores.wave_impulse_screen import current_wave_position

        s = _scenario(children_labels=["W1:↑", "W2:↓"], monowave_labels={})
        res = current_wave_position(s)
        assert res["wave_number"] == 2
        assert res["phase"] == "W2_DONE"
        assert res["confidence_level"] == "loose"
        assert res["is_candidate"] is True
        assert res["axis_b_label"] is None

    def test_axis_b_strict_L5_W3_mature(self):
        """plan §9 case 2:W3 + Pass-2 label L5 → W3_MATURE not candidate。"""
        from cross_cores.wave_impulse_screen import current_wave_position

        s = _scenario(
            children_labels=["W1:F5↑", "W2:F3↓", "W3:L5↑"],
            monowave_labels={0: "F5", 1: "F3", 2: "L5"},
        )
        res = current_wave_position(s)
        assert res["wave_number"] == 3
        assert res["phase"] == "W3_MATURE"
        assert res["confidence_level"] == "strict"
        assert res["is_candidate"] is False
        assert res["excluded_reason"] == "w3_mature"
        assert res["axis_b_label"] == "L5"

    def test_axis_b_F3_W2_done(self):
        """plan §9 case 3:W2 + Pass-2 label F3 → W2_DONE candidate strict。"""
        from cross_cores.wave_impulse_screen import current_wave_position

        s = _scenario(
            children_labels=["W1:F5↑", "W2:F3↓"],
            monowave_labels={0: "F5", 1: "F3"},
        )
        res = current_wave_position(s)
        assert res["phase"] == "W2_DONE"
        assert res["confidence_level"] == "strict"
        assert res["is_candidate"] is True
        assert res["axis_b_label"] == "F3"

    def test_W3_ongoing_with_F5(self):
        """W3 + F5 → W3_ONGOING candidate(W3 仍可加倉)。"""
        from cross_cores.wave_impulse_screen import current_wave_position

        s = _scenario(
            children_labels=["W1", "W2", "W3"],
            monowave_labels={0: "F5", 1: "F3", 2: "UnknownFive"},
        )
        res = current_wave_position(s)
        assert res["phase"] == "W3_ONGOING"
        assert res["is_candidate"] is True

    def test_W4_done_observe_only(self):
        """plan §9 case 4 變體:W4 done → 進 W5 observe,not candidate。"""
        from cross_cores.wave_impulse_screen import current_wave_position

        s = _scenario(
            children_labels=["W1", "W2", "W3", "W4"],
            monowave_labels={0: "F5", 1: "F3", 2: "L5", 3: "C3"},
        )
        res = current_wave_position(s)
        assert res["phase"] == "W4_DONE"
        assert res["is_candidate"] is False
        assert res["excluded_reason"] == "w5_observe_only"

    def test_W5_emit_row_not_candidate(self):
        """plan §9 case 4:W5 phase emit row 但 candidate=False。"""
        from cross_cores.wave_impulse_screen import current_wave_position

        s = _scenario(
            children_labels=["W1", "W2", "W3", "W4", "W5"],
            monowave_labels={0: "F5", 1: "F3", 2: "L5", 3: "C3", 4: "L5"},
        )
        res = current_wave_position(s)
        assert res["phase"] == "W5_MATURE"
        assert res["is_candidate"] is False
        assert res["emit_row"] is True
        assert res["excluded_reason"] == "w5_observe_only"

    def test_no_children_excluded(self):
        from cross_cores.wave_impulse_screen import current_wave_position

        s = _scenario(children_labels=[], monowave_labels={})
        res = current_wave_position(s)
        assert res["phase"] == "OTHER"
        assert res["excluded_reason"] == "no_children"

    def test_no_W_regex_excluded(self):
        from cross_cores.wave_impulse_screen import current_wave_position

        s = _scenario(children_labels=["random_label"], monowave_labels={})
        res = current_wave_position(s)
        assert res["phase"] == "OTHER"
        assert res["excluded_reason"] == "no_W_regex"


# ════════════════════════════════════════════════════════════
# Pattern_type gate
# ════════════════════════════════════════════════════════════


class TestPatternTypeGate:

    def test_pattern_type_diagonal_included(self):
        """plan §9 case 5:Diagonal 走 impulse 系(NEoWave compacted_base_label==Five)。"""
        from cross_cores.wave_impulse_screen import _pattern_kind_ok

        s = _scenario(pattern_type={"Diagonal": {"Leading": None}})
        ok, label = _pattern_kind_ok(s)
        assert ok is True
        assert label == "Diagonal"

    def test_pattern_type_impulse_ok(self):
        from cross_cores.wave_impulse_screen import _pattern_kind_ok

        ok, label = _pattern_kind_ok(_scenario(pattern_type="Impulse"))
        assert ok is True
        assert label == "Impulse"

    def test_pattern_type_zigzag_filtered(self):
        """plan §9 case 6:Zigzag 非 impulse 系 → 拒絕。"""
        from cross_cores.wave_impulse_screen import _pattern_kind_ok

        ok, label = _pattern_kind_ok(
            _scenario(pattern_type={"Zigzag": {"Single": None}})
        )
        assert ok is False
        assert label == "Zigzag"

    def test_pattern_type_combination_filtered(self):
        from cross_cores.wave_impulse_screen import _pattern_kind_ok

        ok, label = _pattern_kind_ok(
            _scenario(pattern_type={"Combination": {"sub_kinds": []}})
        )
        assert ok is False
        assert label == "Combination"


# ════════════════════════════════════════════════════════════
# Target / invalidation 抽取
# ════════════════════════════════════════════════════════════


class TestExtractMetrics:

    def test_extract_target_in_range(self):
        from cross_cores.wave_impulse_screen import _extract_target_price

        zones = [
            {"source_ratio": 1.0, "low": 80, "high": 90},          # 不在 [1.382, 2.618]
            {"source_ratio": 1.618, "low": 120, "high": 130},      # 中位 125 ✓
            {"source_ratio": 2.000, "low": 140, "high": 150},      # 中位 145
        ]
        t = _extract_target_price(_scenario(expected_fib_zones=zones))
        # 取最近(最小)= 125
        assert t == 125.0

    def test_extract_target_no_match(self):
        from cross_cores.wave_impulse_screen import _extract_target_price

        zones = [{"source_ratio": 0.5, "low": 50, "high": 60}]
        assert _extract_target_price(_scenario(expected_fib_zones=zones)) is None

    def test_extract_below_invalidation_max(self):
        from cross_cores.wave_impulse_screen import _extract_below_invalidation

        triggers = [
            {"on_trigger": "InvalidateScenario",
             "trigger_type": {"PriceBreakBelow": 80.0}},
            {"on_trigger": "InvalidateScenario",
             "trigger_type": {"PriceBreakBelow": 75.0}},
            {"on_trigger": "WeakenScenario",
             "trigger_type": {"PriceBreakBelow": 99.0}},  # 非 invalidate → 略
            {"on_trigger": "InvalidateScenario",
             "trigger_type": {"PriceBreakAbove": 200.0}},  # above 不算
        ]
        v = _extract_below_invalidation(_scenario(invalidation_triggers=triggers))
        assert v == 80.0   # max of below triggers(最緊 stop)


# ════════════════════════════════════════════════════════════
# _build_row 行為(R/R 計算 + 觸發 demote)
# ════════════════════════════════════════════════════════════


class TestBuildRow:

    def _good_scenario(self):
        """W2 done + bullish + 有 target + 有 invalidation。"""
        return _scenario(
            children_labels=["W1", "W2"],
            monowave_labels={0: "F5", 1: "F3"},
            power_rating="StrongBullish",
            expected_fib_zones=[{"source_ratio": 1.618, "low": 130, "high": 140}],
            invalidation_triggers=[
                {"on_trigger": "InvalidateScenario",
                 "trigger_type": {"PriceBreakBelow": 90.0}},
            ],
        )

    def test_rr_above_threshold_keeps_candidate(self):
        from cross_cores.wave_impulse_screen import _build_row

        s = self._good_scenario()
        snap = {"scenario_forest": [s]}
        # entry=100,inv=90,target=135 → rr = (135-100)/(100-90) = 3.5 ≥ 1.5 ✓
        row = _build_row(
            stock_id="2330", target_date=date(2026, 5, 15), timeframe="daily",
            snapshot=snap, current_price=100.0, excluded_reason=None,
        )
        assert row["is_candidate"] is True
        assert row["phase"] == "W2_DONE"
        assert row["rr_ratio"] is not None and row["rr_ratio"] >= 1.5
        assert row["entry_price"] == 100.0
        assert row["target_price"] == 135.0
        assert row["invalidation_price"] == 90.0

    def test_rr_below_threshold_demoted(self):
        """plan §9 case 7:rr=0.8 < 1.5 → demote candidate=False。"""
        from cross_cores.wave_impulse_screen import _build_row

        s = _scenario(
            children_labels=["W1", "W2"],
            monowave_labels={0: "F5", 1: "F3"},
            power_rating="StrongBullish",
            # target=105(midpoint of [100,110]),inv=90 → rr=(105-100)/(100-90)=0.5 < 1.5
            expected_fib_zones=[{"source_ratio": 1.618, "low": 100, "high": 110}],
            invalidation_triggers=[
                {"on_trigger": "InvalidateScenario",
                 "trigger_type": {"PriceBreakBelow": 90.0}},
            ],
        )
        row = _build_row(
            stock_id="X", target_date=date(2026, 5, 15), timeframe="daily",
            snapshot={"scenario_forest": [s]}, current_price=100.0,
            excluded_reason=None,
        )
        assert row["is_candidate"] is False
        assert row["excluded_reason"] == "rr_below_threshold"
        # row 仍 emit
        assert row["phase"] == "W2_DONE"
        assert row["entry_price"] == 100.0

    def test_no_target_demoted_to_no_target(self):
        from cross_cores.wave_impulse_screen import _build_row

        s = _scenario(
            children_labels=["W1", "W2"],
            monowave_labels={0: "F5", 1: "F3"},
            power_rating="StrongBullish",
            expected_fib_zones=[],   # 無 fib zone
            invalidation_triggers=[
                {"on_trigger": "InvalidateScenario",
                 "trigger_type": {"PriceBreakBelow": 90.0}},
            ],
        )
        row = _build_row(
            stock_id="X", target_date=date(2026, 5, 15), timeframe="daily",
            snapshot={"scenario_forest": [s]}, current_price=100.0,
            excluded_reason=None,
        )
        assert row["is_candidate"] is False
        assert row["excluded_reason"] == "no_target"

    def test_non_bullish_W3_demoted(self):
        """W3 bearish primary → direction gate 拒絕。"""
        from cross_cores.wave_impulse_screen import _build_row

        s = _scenario(
            children_labels=["W1", "W2", "W3"],
            monowave_labels={0: "F5", 1: "F3", 2: "UnknownFive"},
            power_rating="StrongBearish",
        )
        row = _build_row(
            stock_id="X", target_date=date(2026, 5, 15), timeframe="daily",
            snapshot={"scenario_forest": [s]}, current_price=100.0,
            excluded_reason=None,
        )
        assert row["is_candidate"] is False
        assert row["excluded_reason"] == "non_bullish_direction"
        assert row["direction"] == "bearish"
        assert row["phase"] == "W3_ONGOING"   # phase 仍記錄

    def test_empty_forest_no_crash(self):
        """plan §9 case 10:empty forest → graceful emit empty row。"""
        from cross_cores.wave_impulse_screen import _build_row

        row = _build_row(
            stock_id="X", target_date=date(2026, 5, 15), timeframe="daily",
            snapshot={"scenario_forest": []}, current_price=100.0,
            excluded_reason=None,
        )
        assert row["is_candidate"] is False
        assert row["excluded_reason"] == "empty_forest"

    def test_no_snapshot_emits_excluded_row(self):
        from cross_cores.wave_impulse_screen import _build_row

        row = _build_row(
            stock_id="X", target_date=date(2026, 5, 15), timeframe="daily",
            snapshot=None, current_price=100.0, excluded_reason=None,
        )
        assert row["excluded_reason"] == "no_snapshot"
        assert row["is_candidate"] is False

    def test_universe_excluded_passed_through(self):
        """plan §9 case 11:金融股 universe filter → excluded_reason='financial'。"""
        from cross_cores.wave_impulse_screen import _build_row

        row = _build_row(
            stock_id="2880", target_date=date(2026, 5, 15), timeframe="daily",
            snapshot={"scenario_forest": []}, current_price=100.0,
            excluded_reason="financial",
        )
        assert row["excluded_reason"] == "financial"
        assert row["is_candidate"] is False


# ════════════════════════════════════════════════════════════
# Cross-TF aligned
# ════════════════════════════════════════════════════════════


class TestCrossTFAlignment:

    def test_cross_tf_aligned_daily_weekly_same_dir(self):
        """plan §9 case 8:daily+weekly 同 W3 同向 → both True。"""
        from cross_cores.wave_impulse_screen import _apply_cross_tf_alignment

        rows = [
            {"stock_id": "A", "timeframe": "daily", "phase": "W2_DONE",
             "direction": "bullish", "cross_tf_aligned": False},
            {"stock_id": "A", "timeframe": "weekly", "phase": "W3_ONGOING",
             "direction": "bullish", "cross_tf_aligned": False},
            {"stock_id": "A", "timeframe": "monthly", "phase": "OTHER",
             "direction": "neutral", "cross_tf_aligned": False},
        ]
        _apply_cross_tf_alignment(rows)
        # 三 row 都標 True(per-stock 而非 per-row)
        for r in rows:
            assert r["cross_tf_aligned"] is True

    def test_cross_tf_aligned_false_when_dir_diverge(self):
        """plan §9 case 9:daily bullish + weekly bearish → False。"""
        from cross_cores.wave_impulse_screen import _apply_cross_tf_alignment

        rows = [
            {"stock_id": "B", "timeframe": "daily", "phase": "W3_ONGOING",
             "direction": "bullish", "cross_tf_aligned": False},
            {"stock_id": "B", "timeframe": "weekly", "phase": "W3_ONGOING",
             "direction": "bearish", "cross_tf_aligned": False},
        ]
        _apply_cross_tf_alignment(rows)
        for r in rows:
            assert r["cross_tf_aligned"] is False

    def test_cross_tf_aligned_false_when_phase_diverge(self):
        rows = [
            {"stock_id": "C", "timeframe": "daily", "phase": "W3_MATURE",
             "direction": "bullish", "cross_tf_aligned": False},
            {"stock_id": "C", "timeframe": "weekly", "phase": "W3_ONGOING",
             "direction": "bullish", "cross_tf_aligned": False},
        ]
        from cross_cores.wave_impulse_screen import _apply_cross_tf_alignment

        _apply_cross_tf_alignment(rows)
        # daily W3_MATURE 不在 _CANDIDATE_PHASES → 不對齊
        for r in rows:
            assert r["cross_tf_aligned"] is False


# ════════════════════════════════════════════════════════════
# Ranking
# ════════════════════════════════════════════════════════════


class TestRanking:

    def test_assign_ranks_by_rr(self):
        """plan §9 case 12:eligible 按 (cross_tf↓, rr↓, power↓) 排;top 30 標 is_top_n。"""
        from cross_cores.wave_impulse_screen import _assign_impulse_ranks

        rows = [
            {"stock_id": "A", "timeframe": "daily", "is_candidate": True,
             "rr_ratio": 2.5, "cross_tf_aligned": False,
             "detail": {"power_strength": 2}},
            {"stock_id": "B", "timeframe": "daily", "is_candidate": True,
             "rr_ratio": 1.8, "cross_tf_aligned": True,
             "detail": {"power_strength": 3}},
            {"stock_id": "C", "timeframe": "daily", "is_candidate": True,
             "rr_ratio": 3.2, "cross_tf_aligned": False,
             "detail": {"power_strength": 1}},
        ]
        _assign_impulse_ranks(rows)
        ranks = {r["stock_id"]: r.get("impulse_rank") for r in rows}
        # B cross_tf=True 優先(雖然 rr 較低),A/C 同 cross_tf=False 看 rr_ratio
        assert ranks["B"] == 1
        assert ranks["C"] == 2     # rr 3.2 > 2.5
        assert ranks["A"] == 3
        for r in rows:
            assert r["is_top_n"] is True   # top_n=30,3 個全進

    def test_assign_ranks_per_timeframe_independent(self):
        """各 timeframe 獨立 rank(daily rank 1 與 weekly rank 1 各自)。"""
        from cross_cores.wave_impulse_screen import _assign_impulse_ranks

        rows = [
            {"stock_id": "A", "timeframe": "daily", "is_candidate": True,
             "rr_ratio": 2.0, "cross_tf_aligned": False, "detail": {"power_strength": 2}},
            {"stock_id": "B", "timeframe": "weekly", "is_candidate": True,
             "rr_ratio": 5.0, "cross_tf_aligned": False, "detail": {"power_strength": 2}},
        ]
        _assign_impulse_ranks(rows)
        # 兩個 row 都 rank 1(各自 timeframe)
        for r in rows:
            assert r["impulse_rank"] == 1

    def test_non_candidates_no_rank(self):
        from cross_cores.wave_impulse_screen import _assign_impulse_ranks

        rows = [
            {"stock_id": "A", "timeframe": "daily", "is_candidate": False,
             "rr_ratio": None, "cross_tf_aligned": False, "detail": {}},
        ]
        _assign_impulse_ranks(rows)
        assert rows[0].get("impulse_rank") is None


# ════════════════════════════════════════════════════════════
# End-to-end run() smoke(MagicMock db)
# ════════════════════════════════════════════════════════════


class TestRunSmoke:

    def test_run_empty_db_graceful(self):
        """無 snapshot → graceful empty result。"""
        from cross_cores import wave_impulse_screen as mod

        db = MagicMock()
        # universe + prices + snapshots 全空
        db.query = MagicMock(return_value=[])
        result = mod.run(db)
        assert result["name"] == "wave_impulse_screen"
        assert result["rows_read"] == 0
        assert result["rows_written"] == 0

    def test_run_emits_rows_for_universe(self):
        """1 stock × 3 tf 都跑 → 3 row emit(W2_DONE)。"""
        from cross_cores import wave_impulse_screen as mod

        snap = {
            "scenario_forest": [_scenario(
                children_labels=["W1", "W2"],
                monowave_labels={0: "F5", 1: "F3"},
                power_rating="StrongBullish",
                expected_fib_zones=[{"source_ratio": 1.618, "low": 130, "high": 140}],
                invalidation_triggers=[{
                    "on_trigger": "InvalidateScenario",
                    "trigger_type": {"PriceBreakBelow": 90.0},
                }],
            )],
        }
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
                return [
                    {"stock_id": "2330", "snapshot_date": date(2026, 5, 15),
                     "timeframe": "daily", "snapshot": snap},
                    {"stock_id": "2330", "snapshot_date": date(2026, 5, 15),
                     "timeframe": "weekly", "snapshot": snap},
                    {"stock_id": "2330", "snapshot_date": date(2026, 5, 15),
                     "timeframe": "monthly", "snapshot": snap},
                ]
            return []

        db.query = MagicMock(side_effect=_query)
        # upsert_silver patched to no-op
        from unittest.mock import patch

        with patch("cross_cores.wave_impulse_screen.upsert_silver", return_value=3):
            result = mod.run(db)
        assert result["rows_read"] == 3
        assert result["candidates"] >= 1   # 至少 1 個入 candidate


# ════════════════════════════════════════════════════════════
# Builder contract surface
# ════════════════════════════════════════════════════════════


# ════════════════════════════════════════════════════════════
# r2:_pick_actionable + simplified label table
# ════════════════════════════════════════════════════════════


class TestPickActionable:
    """r2 揭露 production picker bias → _pick_actionable 修正(對齊 wave-impulse
    r2 production verify:r1 完整 5 波 picker 永遠選到 W5_MATURE,無 candidate)。"""

    def _scen(self, *, children_n, pattern_type="Impulse", power="Bullish",
              rules=5, start="2023-01-01", end="2026-05-15"):
        return {
            "wave_tree": {
                "children": [{"label": f"W{i+1}"} for i in range(children_n)],
                "start": start, "end": end,
            },
            "pattern_type": ({pattern_type: None}
                             if isinstance(pattern_type, str) else pattern_type),
            "power_rating": power,
            "rules_passed_count": rules,
            "monowave_structure_labels": [],
        }

    def test_picks_incomplete_over_complete(self):
        """forest 同時有 complete W5 + incomplete W2 → 挑 incomplete。"""
        from cross_cores.wave_impulse_screen import _pick_actionable

        complete = self._scen(children_n=5, power="StrongBullish", rules=7)
        incomplete = self._scen(children_n=2, power="Bullish", rules=5)
        picked = _pick_actionable([complete, incomplete])
        assert picked is incomplete

    def test_picks_highest_power_among_incomplete(self):
        """多個 incomplete:按 (degree↓, power↓, rules↓) 排。"""
        from cross_cores.wave_impulse_screen import _pick_actionable

        weak = self._scen(children_n=2, power="SlightBullish", rules=4,
                          start="2025-12-01")   # 短 span → Subminuette
        strong = self._scen(children_n=2, power="StrongBullish", rules=6,
                            start="2024-01-01") # 長 span → Minor
        picked = _pick_actionable([weak, strong])
        assert picked is strong   # higher degree + stronger power

    def test_fallback_to_pick_primary_when_all_complete(self):
        """無 incomplete → 走 _pick_primary canonical(返回 complete W5)。"""
        from cross_cores.wave_impulse_screen import _pick_actionable

        complete_a = self._scen(children_n=5, power="StrongBullish", rules=7,
                                start="2020-01-01")  # 高 degree
        complete_b = self._scen(children_n=5, power="Bullish", rules=5,
                                start="2024-01-01")  # 低 degree
        picked = _pick_actionable([complete_a, complete_b])
        # _pick_primary 按 (degree↓, power↓, rules↓) → complete_a wins
        assert picked is complete_a

    def test_non_impulse_excluded_from_incomplete_pool(self):
        """Zigzag/Flat/Combination 不算 impulse 系 → 略過。"""
        from cross_cores.wave_impulse_screen import _pick_actionable

        zigzag_inc = self._scen(children_n=2, pattern_type={"Zigzag": {"sub_kind": "Single"}})
        impulse_inc = self._scen(children_n=2, pattern_type="Impulse")
        picked = _pick_actionable([zigzag_inc, impulse_inc])
        assert picked is impulse_inc

    def test_diagonal_included_in_incomplete_pool(self):
        """Diagonal (Leading/Ending Diagonal) 屬 impulse 系 — 可被 actionable picker 挑。"""
        from cross_cores.wave_impulse_screen import _pick_actionable

        diagonal_inc = self._scen(children_n=3,
                                  pattern_type={"Diagonal": {"Ending": None}})
        picked = _pick_actionable([diagonal_inc])
        assert picked is diagonal_inc

    def test_children_5_not_incomplete(self):
        """children=5 是完整,不算 incomplete。"""
        from cross_cores.wave_impulse_screen import _pick_actionable

        c5 = self._scen(children_n=5)
        c2 = self._scen(children_n=2)
        picked = _pick_actionable([c5, c2])
        assert picked is c2

    def test_children_4_counts_as_incomplete_W4_done(self):
        """children=4 算 incomplete(W4_DONE 進 observe 但仍 emit row)。"""
        from cross_cores.wave_impulse_screen import _pick_actionable

        c4 = self._scen(children_n=4)
        c5 = self._scen(children_n=5)
        picked = _pick_actionable([c4, c5])
        assert picked is c4

    def test_empty_forest_returns_none(self):
        from cross_cores.wave_impulse_screen import _pick_actionable

        assert _pick_actionable([]) is None


class TestSimplifiedLabelTable:
    """r2 對照表簡化:last_n 為 source of truth;Axis-B L5/S5 只升級為 mature。
    原 r1 的 `label_mismatch` 條件移除(production verify 揭露 224 row 被誤排除)。"""

    def test_W2_with_L5_is_W2_DONE(self):
        """r1 中 W2+L5 → label_mismatch;r2 → W2_DONE(以 last_n 為主)。"""
        from cross_cores.wave_impulse_screen import current_wave_position

        s = {
            "wave_tree": {"children": [{"label": "W1"}, {"label": "W2"}]},
            "monowave_structure_labels": [
                {"monowave_index": 1, "labels": [{"label": "L5"}]},
            ],
        }
        res = current_wave_position(s)
        assert res["phase"] == "W2_DONE"
        assert res["is_candidate"] is True
        assert res["excluded_reason"] is None

    def test_W3_with_F3_is_W3_ONGOING(self):
        """r1 中 W3+F3 → label_mismatch;r2 → W3_ONGOING(Diagonal W3 可為 :3)。"""
        from cross_cores.wave_impulse_screen import current_wave_position

        s = {
            "wave_tree": {"children": [{"label": "W1"}, {"label": "W2"}, {"label": "W3"}]},
            "monowave_structure_labels": [
                {"monowave_index": 2, "labels": [{"label": "F3"}]},
            ],
        }
        res = current_wave_position(s)
        assert res["phase"] == "W3_ONGOING"
        assert res["is_candidate"] is True

    def test_W4_with_Five_is_W4_DONE(self):
        """r1 中 W4+Five → label_mismatch;r2 → W4_DONE。"""
        from cross_cores.wave_impulse_screen import current_wave_position

        s = {
            "wave_tree": {"children": [{"label": "W1"}, {"label": "W2"},
                                        {"label": "W3"}, {"label": "W4"}]},
            "monowave_structure_labels": [
                {"monowave_index": 3, "labels": [{"label": "Five"}]},
            ],
        }
        res = current_wave_position(s)
        assert res["phase"] == "W4_DONE"
        assert res["excluded_reason"] == "w5_observe_only"

    def test_W3_L5_still_mature(self):
        """L5/S5 仍升級 W3_MATURE(NEoWave :L5 = last impulse 訊號)。"""
        from cross_cores.wave_impulse_screen import current_wave_position

        s = {
            "wave_tree": {"children": [{"label": "W1"}, {"label": "W2"}, {"label": "W3"}]},
            "monowave_structure_labels": [
                {"monowave_index": 2, "labels": [{"label": "L5"}]},
            ],
        }
        res = current_wave_position(s)
        assert res["phase"] == "W3_MATURE"
        assert res["is_candidate"] is False
        assert res["excluded_reason"] == "w3_mature"

    def test_W5_S5_mature(self):
        """W5+S5 → W5_MATURE(Special Five 同 L5 等級)。"""
        from cross_cores.wave_impulse_screen import current_wave_position

        s = {
            "wave_tree": {"children": [{"label": "W1"}, {"label": "W2"},
                                        {"label": "W3"}, {"label": "W4"}, {"label": "W5"}]},
            "monowave_structure_labels": [
                {"monowave_index": 4, "labels": [{"label": "S5"}]},
            ],
        }
        res = current_wave_position(s)
        assert res["phase"] == "W5_MATURE"


def test_builder_contract():
    """對齊 cross_cores._base.CrossStockBuilder protocol。"""
    from cross_cores import wave_impulse_screen as m

    assert m.NAME == "wave_impulse_screen"
    assert m.OUTPUT_TABLE == "wave_impulse_screen_derived"
    assert "structural_snapshots" in m.UPSTREAM_TABLES
    assert "price_daily_fwd" in m.UPSTREAM_TABLES
    assert callable(m.run)


def test_registered_in_orchestrator():
    from cross_cores.orchestrator import BUILDERS

    assert "wave_impulse_screen" in BUILDERS
