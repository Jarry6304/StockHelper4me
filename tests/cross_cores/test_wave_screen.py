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

        # target = midpoint (102.5) of [100,105]; entry=100; invalidation=90 → rr=2.5/10=0.25
        # r4:target must be > current(102.5 > 100 ✓);invalidation must be < current(90 < 100 ✓)
        s = _zigzag(end_date="2026-05-22", direction="down",
                    fib_zones=[{"source_ratio": 1.618, "low": 100.01, "high": 105}])
        row = _build_row(
            stock_id="X", target_date=SNAPSHOT_DATE, timeframe="daily",
            snapshot={"scenario_forest": [s]}, current_price=100.0,
            excluded_reason=None, snapshot_date=SNAPSHOT_DATE,
        )
        assert row["is_candidate"] is False
        assert row["excluded_reason"] == "rr_below_threshold"

    def test_target_below_current_excluded(self):
        """r4 geometry sanity:target ≤ current → 不入 candidate。"""
        from cross_cores.wave_impulse_screen import _build_row

        # target midpoint = 80(< current 100)
        s = _zigzag(end_date="2026-05-22", direction="down",
                    fib_zones=[{"source_ratio": 1.618, "low": 75, "high": 85}])
        row = _build_row(
            stock_id="X", target_date=SNAPSHOT_DATE, timeframe="daily",
            snapshot={"scenario_forest": [s]}, current_price=100.0,
            excluded_reason=None, snapshot_date=SNAPSHOT_DATE,
        )
        assert row["is_candidate"] is False
        assert row["excluded_reason"] == "target_below_current"

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
        """r4 invalidation trigger 不存在 → no_invalidation。"""
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
