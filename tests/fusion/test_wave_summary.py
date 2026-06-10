"""src/fusion/wave_summary.py — V2 WAVE 欄批次摘要抽取(純函式 + 批次入口)。

canned doc 不打真 DB;top scenario 選法鏡射前端 pickDefaultScenario
(invalidation 過濾 → recency tier → power → passed → days),兩端改一邊必同步。
"""

from __future__ import annotations

from datetime import date

import pytest

from fusion.wave_summary import (
    _resonance_level,
    _sparkline,
    digest_from_docs,
    wave_summary_rows,
)

AS_OF = date(2026, 6, 11)


def _scenario(
    *,
    label: str = "Impulse·W3",
    pattern_type="Impulse",
    power: str = "Bullish",
    end: str = "2026-06-01",
    passed: int = 10,
    certainty: str = "Primary",
    invalidate_below: float | None = None,
) -> dict:
    triggers = []
    if invalidate_below is not None:
        triggers.append({
            "on_trigger": "InvalidateScenario",
            "trigger_type": {"PriceBreakBelow": invalidate_below},
        })
    return {
        "structure_label": label,
        "pattern_type": pattern_type,
        "power_rating": power,
        "rules_passed_count": passed,
        "deferred_rules_count": 0,
        "wave_tree": {"end": end},
        "invalidation_triggers": triggers,
        "monowave_structure_labels": [
            {"labels": [{"certainty": certainty}]},
        ],
    }


def _neely_row(forest: list[dict], *, monowaves: list[dict] | None = None,
               snapshot_date: date = AS_OF, insufficient: bool = False) -> dict:
    if monowaves is None:
        monowaves = [
            {"end_price": 100.0}, {"end_price": 110.0}, {"end_price": 105.0},
        ]
    return {
        "stock_id": "2330",
        "snapshot_date": snapshot_date,
        "snapshot": {
            "scenario_forest": forest,
            "monowave_series": monowaves,
            "insufficient_data": insufficient,
        },
    }


def _reso_row(findings_levels: list[str], *, single_track: bool = False) -> dict:
    return {
        "stock_id": "2330",
        "snapshot_date": AS_OF,
        "snapshot": {
            "findings": [{"level": lv} for lv in findings_levels],
            "single_track_mode": single_track,
        },
    }


class TestDigestFromDocs:
    def test_normal_extraction(self):
        row = _neely_row([_scenario()])
        d = digest_from_docs("2330", row, _reso_row(["basic"]), AS_OF)
        assert d == {
            "stock_id": "2330",
            "insufficient": False,
            "label": "Impulse·W3",
            "direction": "up",
            "scenario_count": 1,
            "certainty": "Primary",
            "sparkline": [0.0, 1.0, 0.5],
            "resonance": "basic",
            "staleness_days": 0,
        }

    def test_recency_tier_beats_power(self):
        """近期 Neutral 勝過一年前 StrongBullish(鏡射 V1 fb9e166 老化形態修正)。"""
        old_strong = _scenario(label="OLD", power="StrongBullish", end="2025-01-01")
        recent_neutral = _scenario(label="NEW", power="Neutral", end="2026-06-01")
        d = digest_from_docs("2330", _neely_row([old_strong, recent_neutral]), None, AS_OF)
        assert d["label"] == "NEW"

    def test_invalidated_scenario_deprioritized(self):
        """現價跌破 InvalidateScenario 門檻的 scenario 退位(同 tier 同期)。"""
        breached = _scenario(label="BREACHED", power="StrongBullish",
                             invalidate_below=200.0)  # 現價 105 < 200 → invalidated
        alive = _scenario(label="ALIVE", power="Neutral")
        d = digest_from_docs("2330", _neely_row([breached, alive]), None, AS_OF)
        assert d["label"] == "ALIVE"

    @pytest.mark.parametrize("pattern_type,power,expect", [
        ({"Zigzag": {"sub_kind": "Normal"}}, "Bullish", "correction"),
        ({"Flat": {"sub_kind": "Normal"}}, "Bearish", "correction"),
        ({"Triangle": {"sub_kind": "Contracting"}}, "Neutral", "correction"),
        ({"Combination": {"sub_kinds": []}}, "Bullish", "correction"),
        ("RunningCorrection", "Bullish", "correction"),
        ("Impulse", "StrongBullish", "up"),
        ({"Diagonal": {"sub_kind": "Leading"}}, "Bearish", "down"),
        ("Impulse", "Neutral", "flat"),
    ])
    def test_direction_mapping(self, pattern_type, power, expect):
        sc = _scenario(pattern_type=pattern_type, power=power)
        d = digest_from_docs("2330", _neely_row([sc]), None, AS_OF)
        assert d["direction"] == expect

    def test_certainty_takes_strongest_label(self):
        sc = _scenario()
        sc["monowave_structure_labels"] = [
            {"labels": [{"certainty": "Rare"}]},
            {"labels": [{"certainty": "Possible"}, {"certainty": "Primary"}]},
        ]
        d = digest_from_docs("2330", _neely_row([sc]), None, AS_OF)
        assert d["certainty"] == "Primary"

    def test_certainty_fallback_possible(self):
        sc = _scenario()
        sc["monowave_structure_labels"] = []
        d = digest_from_docs("2330", _neely_row([sc]), None, AS_OF)
        assert d["certainty"] == "Possible"

    def test_staleness_days(self):
        row = _neely_row([_scenario()], snapshot_date=date(2026, 6, 1))
        d = digest_from_docs("2330", row, None, AS_OF)
        assert d["staleness_days"] == 10

    @pytest.mark.parametrize("row", [
        None,                                      # 無 snapshot
        _neely_row([], insufficient=False),        # 空 forest
        _neely_row([_scenario()], insufficient=True),  # 引擎報無法判斷
        {"stock_id": "2330", "snapshot": "not-a-dict", "snapshot_date": AS_OF},
    ])
    def test_insufficient_paths(self, row):
        d = digest_from_docs("2330", row, None, AS_OF)
        assert d["insufficient"] is True
        assert d["label"] == "" and d["scenario_count"] == 0
        assert d["sparkline"] == [] and d["resonance"] == "none"


class TestResonanceLevel:
    @pytest.mark.parametrize("levels,single,expect", [
        (["divergence", "strong", "basic"], False, "strong"),
        (["divergence", "basic"], False, "basic"),
        (["divergence"], False, "divergence"),
        ([], False, "none"),
        (["strong"], True, "none"),  # A-3 閘門:single_track → 共振判定跳過
    ])
    def test_reduce(self, levels, single, expect):
        assert _resonance_level(_reso_row(levels, single_track=single)["snapshot"]) == expect

    def test_no_doc_is_none(self):
        assert _resonance_level(None) == "none"


class TestSparkline:
    def test_normalize_min_max(self):
        mws = [{"end_price": p} for p in [100.0, 150.0, 125.0]]
        assert _sparkline(mws) == [0.0, 1.0, 0.5]

    def test_flat_series_all_half(self):
        mws = [{"end_price": 100.0}] * 4
        assert _sparkline(mws) == [0.5] * 4

    def test_tail_cap_10_points(self):
        mws = [{"end_price": float(i)} for i in range(25)]
        out = _sparkline(mws)
        assert len(out) == 10 and out[0] == 0.0 and out[-1] == 1.0

    def test_fewer_than_two_points_empty(self):
        assert _sparkline([{"end_price": 100.0}]) == []
        assert _sparkline([]) == []


# ── 批次入口(fake conn,依 core_name 參數分流 rows)──────────────────────────

class _DispatchCursor:
    def __init__(self, by_core: dict[str, list[dict]]):
        self._by_core = by_core
        self._rows: list[dict] = []

    def __enter__(self):
        return self

    def __exit__(self, *a):
        return False

    def execute(self, sql, params=None):
        core_name = params[1]  # [stock_ids, core_name, timeframe, as_of]
        self._rows = self._by_core.get(core_name, [])

    def fetchall(self):
        return list(self._rows)


class _DispatchConn:
    def __init__(self, by_core: dict[str, list[dict]]):
        self._by_core = by_core

    def cursor(self):
        return _DispatchCursor(self._by_core)


class TestWaveSummaryRows:
    def test_order_preserved_and_missing_is_insufficient(self):
        conn = _DispatchConn({
            "neely_core": [_neely_row([_scenario()])],  # 只有 2330 有資料
            "resonance_fusion": [_reso_row(["strong"])],
        })
        rows = wave_summary_rows(conn, ["9999", "2330"], AS_OF)
        assert [r["stock_id"] for r in rows] == ["9999", "2330"]
        assert rows[0]["insufficient"] is True
        assert rows[1]["insufficient"] is False and rows[1]["resonance"] == "strong"

    def test_malformed_doc_degrades_to_insufficient(self):
        bad = {"stock_id": "2330", "snapshot_date": AS_OF,
               "snapshot": {"scenario_forest": [{"wave_tree": 123}],  # 異常型別
                            "monowave_series": "oops"}}
        conn = _DispatchConn({"neely_core": [bad], "resonance_fusion": []})
        rows = wave_summary_rows(conn, ["2330"], AS_OF)
        assert rows[0]["insufficient"] is True

    def test_empty_ids_returns_empty(self):
        assert wave_summary_rows(_DispatchConn({}), [], AS_OF) == []

    def test_bad_timeframe_raises(self):
        with pytest.raises(ValueError):
            wave_summary_rows(_DispatchConn({}), ["2330"], AS_OF, timeframe="hourly")
