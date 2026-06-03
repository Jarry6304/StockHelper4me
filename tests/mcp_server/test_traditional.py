"""Traditional Core MCP helper(compute_traditional_forest)測試 — fake sync conn,不打真 PG。

patch `fusion.raw._db.get_connection`(_traditional.py 於 call time import)→ FakeConn。
"""

import datetime
from unittest.mock import patch


class _FakeCur:
    def __init__(self, row):
        self._row = row

    def __enter__(self):
        return self

    def __exit__(self, *a):
        return False

    def execute(self, sql, params=None):
        pass

    def fetchone(self):
        return self._row


class _FakeConn:
    def __init__(self, row):
        self._row = row

    def cursor(self):
        return _FakeCur(self._row)

    def close(self):
        pass


def _row_with_one_scenario():
    forest = {
        "stock_id": "3363",
        "timeframe": "daily",
        "scenario_forest": [
            {
                "id": "imp_0",
                "structure_label": "1-2-3-4-5 (Impulse, ↑)",
                "pattern_type": "Impulse",
                "direction": "Up",
                "degree": "Minor",
                "preference_score": 3,
                "guidelines_satisfied": ["Alternation", "Equality", "FibWave2Retrace"],
                "qualifiers_met": [],
                "invalidation_triggers": [{"kind": "PriceBreakBelow", "price": 10.0}],
                "expected_fib_zones": [{"label": "x", "low": 1.0, "high": 2.0, "source_ratio": 0.382}],
                "wave_tree": {
                    "start": "2020-01-02",
                    "end": "2021-01-02",
                    "start_price": 10.0,
                    "end_price": 28.0,
                },
            }
        ],
    }
    diag = {
        "pivot_count": 7,
        "candidate_count": 5,
        "validator_pass_count": 2,
        "validator_reject_count": 3,
        "forest_overflow_triggered": False,
        "insufficient_data": False,
    }
    return {
        "forest": forest,
        "diagnostics": diag,
        "computed_at": datetime.datetime(2026, 6, 2, 1, 0),
        "data_start": datetime.date(2020, 1, 2),
        "data_end": datetime.date(2021, 1, 2),
    }


def test_has_snapshot_summary():
    with patch("fusion.raw._db.get_connection", return_value=_FakeConn(_row_with_one_scenario())):
        from mcp_server._traditional import compute_traditional_forest

        out = compute_traditional_forest("3363", "daily")
    assert out["has_snapshot"] is True
    assert out["scenario_count"] == 1
    assert out["pivot_count"] == 7
    assert len(out["top_scenarios"]) == 1
    s = out["top_scenarios"][0]
    assert s["preference_score"] == 3
    assert s["preference_score"] == len(s["guidelines_satisfied"]) + len(s["qualifiers_met"])
    assert s["invalidation_price"] == 10.0
    assert s["fib_zone_count"] == 1
    assert "caveat" in out and "不選 primary" in out["caveat"]


def test_no_snapshot_graceful():
    with patch("fusion.raw._db.get_connection", return_value=_FakeConn(None)):
        from mcp_server._traditional import compute_traditional_forest

        out = compute_traditional_forest("9999", "daily")
    assert out["has_snapshot"] is False
    assert out["scenario_count"] == 0
    assert out["top_scenarios"] == []
    assert "traditional_snapshots" in out["narrative"]
