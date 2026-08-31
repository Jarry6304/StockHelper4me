"""dossier builder tests(wave_judgment_loop §4;mock,不依賴真實 PG)。"""

from __future__ import annotations

import sys
from datetime import date
from pathlib import Path
from unittest.mock import MagicMock

sys.path.insert(0, str(Path(__file__).resolve().parents[2]))
sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "src"))

import fusion.judgment.dossier as dossier_mod  # noqa: E402
from fusion.judgment import CANDIDATES_CAP, build_dossier  # noqa: E402

AS_OF = date(2026, 8, 28)


def _mw(start: str, end: str, s_idx: int, e_idx: int) -> dict:
    return {
        "start_date": start, "end_date": end,
        "start_price": 100.0, "end_price": 110.0,
        "direction": "Up", "bar_indices": [s_idx, e_idx],
    }


def _scenario(
    sid: str,
    *,
    start: str,
    end: str,
    degree: int = 1,
    pattern: dict | str = "Impulse",
    direction: str = "Up",
    ch6: str | None = "Pending",
    robust: bool | None = True,
) -> dict:
    s = {
        "id": sid,
        "pattern_type": pattern,
        "structure_label": f"{sid}-label",
        "initial_direction": direction,
        "wave_tree": {
            "label": "Impulse L1↑", "base_label": "Five",
            "start": start, "end": end, "degree_level": degree, "children": [],
        },
        "passed_rules": ["Ch5_Essential"],
        "deferred_rules": [],
        "rules_passed_count": 1,
        "advisory_findings": [],
        "complexity_level": "Simple",
        "triplexity_detected": False,
        "power_rating": "Bullish",
        "post_pattern_behavior": "Unconstrained",
        "max_retracement": None,
        "invalidation_triggers": [],
        "expected_fib_zones": [{"label": "z", "low": 100.0, "high": 120.0, "source_ratio": 0.618}],
        "awaiting_l_label": False,
    }
    if ch6 is not None:
        s["ch6_status"] = ch6
    if robust is not None:
        s["robust"] = robust
    return s


def _neely_row(forest: list[dict], *, timeframe: str = "daily", with_e1e4: bool = True) -> dict:
    # monowave 端點覆蓋 scenario 端點;last bar = idx 100(2026-08-28)
    snapshot = {
        "scenario_forest": forest,
        "monowave_series": [
            _mw("2026-01-05", "2026-03-04", 0, 30),
            _mw("2026-03-04", "2026-06-10", 30, 70),
            _mw("2026-06-10", "2026-08-25", 70, 97),
            _mw("2026-08-25", "2026-08-28", 97, 100),
        ],
        "data_range": {"start": "2026-01-05", "end": "2026-08-28"},
    }
    if with_e1e4:
        snapshot["assumptions"] = [
            {"name": "REVERSAL_ATR_MULTIPLIER", "value": 0.5, "source": "Engineering"},
        ]
        snapshot["assumption_hash"] = "9f3a9f3a9f3a9f3a"
        snapshot["live_edge_ambiguity"] = {"count": 2, "kinds": ["Impulse"], "degree_level": 1}
    return {
        "stock_id": "2330", "snapshot_date": AS_OF, "timeframe": timeframe,
        "core_name": "neely_core", "source_version": "1.3.0" if with_e1e4 else "1.1.1",
        "snapshot": snapshot, "params_hash": "ph-1",
    }


def _patch(monkeypatch, *, rows: list[dict], trad: dict | None = None, judgment: dict | None = None):
    monkeypatch.setattr(dossier_mod, "fetch_structural_latest", lambda *a, **k: rows)
    monkeypatch.setattr(dossier_mod, "fetch_traditional_latest", lambda *a, **k: trad)
    monkeypatch.setattr(dossier_mod, "fetch_active_judgment", lambda *a, **k: judgment)


LIVE = "2026-08-28"       # end bar 100 → live
LIVE_EDGE = "2026-08-25"  # end bar 97 = 100-3 → live(邊界)
OLD = "2026-06-10"        # end bar 70 → historical


class TestStructure:
    def test_required_and_deleted_keys(self, monkeypatch):
        _patch(monkeypatch, rows=[_neely_row([_scenario("a", start="2026-03-04", end=LIVE)])])
        d = build_dossier(MagicMock(), stock_id="2330", as_of=AS_OF, current_price=1180.0)

        for k in ("stock_id", "as_of", "current_price", "engine", "assumptions",
                  "timeframes", "cross_timeframe", "active_judgment", "quality_caveat"):
            assert k in d, k
        # 三鍵刪除(非 rename)
        for k in ("primary_scenario", "scenario_count", "scenario_staleness"):
            assert k not in d, k
        daily = d["timeframes"]["daily"]
        assert daily["snapshot_ref"] == {"snapshot_date": "2026-08-28", "params_hash": "ph-1"}
        assert daily["monowave_count"] == 4
        assert daily["last_bar"] == "2026-08-28"
        assert daily["live_edge"]["ambiguity"]["count"] == 2
        c = daily["candidates"][0]
        for zone in ("evidence", "forward"):
            assert zone in c
        assert c["anchor_key"].startswith("Impulse|Five|2026-03-04|2026-08-28[")
        assert d["engine"]["neely"] == "1.3.0"
        assert d["engine"]["assumption_hash"] == "9f3a9f3a9f3a9f3a"

    def test_no_score_keys_on_candidates(self, monkeypatch):
        _patch(monkeypatch, rows=[_neely_row([_scenario("a", start="2026-03-04", end=LIVE)])])
        d = build_dossier(MagicMock(), stock_id="2330", as_of=AS_OF)
        c = d["timeframes"]["daily"]["candidates"][0]
        for k in ("score", "rank", "prob_up", "confidence", "primary"):
            assert k not in c, k

    def test_live_edge_split_and_sort(self, monkeypatch):
        forest = [
            _scenario("old", start="2026-01-05", end=OLD, degree=2),
            _scenario("hi-deg", start="2026-03-04", end=LIVE_EDGE, degree=2),
            _scenario("late-end", start="2026-03-04", end=LIVE, degree=1),
            _scenario("early-start", start="2026-01-05", end=LIVE, degree=1),
        ]
        _patch(monkeypatch, rows=[_neely_row(forest)])
        d = build_dossier(MagicMock(), stock_id="2330", as_of=AS_OF)
        daily = d["timeframes"]["daily"]
        ids = [c["id"] for c in daily["candidates"]]
        # degree desc → end desc → start asc
        assert ids == ["hi-deg", "early-start", "late-end"]
        assert daily["historical"]["count"] == 1

    def test_old_snapshot_defaults(self, monkeypatch):
        """1.1.1 舊 snapshot:缺 ch6_status/robust/E1/E4 → Deferred / null / 空 / null(§11)。"""
        row = _neely_row(
            [_scenario("a", start="2026-03-04", end=LIVE, ch6=None, robust=None)],
            with_e1e4=False,
        )
        _patch(monkeypatch, rows=[row])
        d = build_dossier(MagicMock(), stock_id="2330", as_of=AS_OF)
        c = d["timeframes"]["daily"]["candidates"][0]
        assert c["evidence"]["ch6_status"] == "Deferred"
        assert c["evidence"]["robust"] is None
        assert d["timeframes"]["daily"]["live_edge"]["ambiguity"] is None
        assert d["assumptions"] == []
        assert d["engine"]["assumption_hash"] is None

    def test_empty_forest(self, monkeypatch):
        _patch(monkeypatch, rows=[_neely_row([])])
        d = build_dossier(MagicMock(), stock_id="2330", as_of=AS_OF)
        assert d["timeframes"]["daily"]["candidates"] == []
        assert d["timeframes"]["daily"]["historical"]["count"] == 0

    def test_truncation_cap(self, monkeypatch):
        forest = [
            _scenario(f"s{i}", start="2026-03-04", end=LIVE, degree=1)
            for i in range(CANDIDATES_CAP + 5)
        ]
        _patch(monkeypatch, rows=[_neely_row(forest)])
        d = build_dossier(MagicMock(), stock_id="2330", as_of=AS_OF)
        daily = d["timeframes"]["daily"]
        assert len(daily["candidates"]) == CANDIDATES_CAP
        assert daily["truncated"] is True


class TestCrossTimeframe:
    def test_direction_conflict(self, monkeypatch):
        rows = [
            _neely_row([_scenario("d", start="2026-03-04", end=LIVE, direction="Up")]),
            _neely_row(
                [_scenario("w", start="2026-03-04", end=LIVE, direction="Down",
                           pattern={"Zigzag": {"sub_kind": "Single"}})],
                timeframe="weekly",
            ),
        ]
        _patch(monkeypatch, rows=rows)
        d = build_dossier(MagicMock(), stock_id="2330", as_of=AS_OF)
        assert d["cross_timeframe"]["direction_conflict"] is True

    def test_missing_weekly_notes_not_conflict(self, monkeypatch):
        _patch(monkeypatch, rows=[_neely_row([_scenario("d", start="2026-03-04", end=LIVE)])])
        d = build_dossier(MagicMock(), stock_id="2330", as_of=AS_OF)
        ct = d["cross_timeframe"]
        assert ct["direction_conflict"] is False
        assert any("資料窗不足" in n for n in ct["notes"])


class TestTraditional:
    def test_concordance_endpoints(self, monkeypatch):
        trad = {
            "forest": [{
                "id": "trad-1",
                "pattern_type": {"Zigzag": {"sub_kind": "Single"}},
                "direction": "Up",
                "wave_tree": {"start": "2026-03-04", "end": LIVE},
                "guidelines_satisfied": ["AlternationGuideline"],
                "qualifiers_met": ["FibQualifier"],
            }],
            "diagnostics": {"engine_version": "3.0.0"},
            "params_hash": "tp-1",
        }
        _patch(monkeypatch, rows=[_neely_row([_scenario("a", start="2026-03-04", end=LIVE)])], trad=trad)
        d = build_dossier(MagicMock(), stock_id="2330", as_of=AS_OF)
        t = d["timeframes"]["daily"]["traditional"]
        assert t["candidates"][0]["guidelines"] == ["AlternationGuideline", "FibQualifier"]
        assert t["candidates"][0]["rules_failed"] == []
        assert t["concordance"] == [{
            "neely": d["timeframes"]["daily"]["candidates"][0]["anchor_key"],
            "traditional": "trad-1",
            "shared": "endpoints",
        }]
        assert d["engine"]["traditional"] == "3.0.0"


class TestActiveJudgment:
    def test_judgment_summary_attached(self, monkeypatch):
        judgment = {
            "id": 7, "as_of": AS_OF, "judged_by": "human",
            "accepted": [{"anchor_key": "K", "role": "preferred"}],
            "degree_read": "Minor", "confidence_class": "single",
            "invalidation": {"price_levels": [], "time_limit_bar": None},
            "status": "active", "assumption_hash": "9f3a", "engine_version": "1.3.0",
        }
        _patch(monkeypatch, rows=[_neely_row([_scenario("a", start="2026-03-04", end=LIVE)])],
               judgment=judgment)
        d = build_dossier(MagicMock(), stock_id="2330", as_of=AS_OF)
        aj = d["active_judgment"]["daily"]
        assert aj["id"] == 7 and aj["judged_by"] == "human"
        assert aj["accepted"][0]["role"] == "preferred"
