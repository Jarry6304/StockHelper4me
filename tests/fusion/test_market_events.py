"""Fusion Layer · market_events 單元測試(mock conn,不依賴真實 PG)。"""

from datetime import date
from unittest.mock import MagicMock

from fusion._shared import fact_to_event, severity_to_int, severity_to_label
from fusion.market_events import market_events


def _mock_conn(rows):
    conn = MagicMock()
    cur = MagicMock()
    cur.fetchall.return_value = rows
    conn.cursor.return_value.__enter__.return_value = cur
    return conn


def test_severity_round_trip():
    assert severity_to_int("warning") == 3
    assert severity_to_int("CRITICAL") == 4
    assert severity_to_int("nonsense") == 1  # 退化 info
    assert severity_to_int(3) == 3
    assert severity_to_label(4) == "critical"
    assert severity_to_label(None) == "info"


def test_market_events_maps_and_aggregates():
    rows = [
        {
            "stock_id": "_index_taiex_", "fact_date": date(2026, 5, 18), "timeframe": "daily",
            "source_core": "taiex_core", "statement": "TAIEX Drawdown5pct ...", "severity": 3,
            "metadata": {"event_kind": "Drawdown5pct", "drawdown_pct": -6.2},
        },
        {
            "stock_id": "_global_", "fact_date": date(2026, 5, 17), "timeframe": "daily",
            "source_core": "fear_greed_core", "statement": "Fear&Greed EnterPanic ...", "severity": 4,
            "metadata": {"event_kind": "EnterPanic", "value": 8.0},
        },
    ]
    out = market_events(
        date(2026, 5, 1), date(2026, 5, 18), severity_min="notable", conn=_mock_conn(rows)
    )
    assert out["event_count"] == 2
    assert out["severity_min"] == "notable"
    assert out["events"][0]["kind"] == "Drawdown5pct"
    assert out["events"][0]["severity"] == "warning"
    assert out["events"][0]["value"] == -6.2
    assert out["events"][0]["source"] == "taiex_core"
    assert out["events"][1]["severity"] == "critical"
    assert out["by_severity"] == {"warning": 1, "critical": 1}
    assert out["start_date"] == "2026-05-01"
    assert out["end_date"] == "2026-05-18"


def test_market_events_pushes_severity_filter_into_sql():
    conn = _mock_conn([])
    market_events(date(2026, 5, 1), date(2026, 5, 18), severity_min="warning", conn=conn)
    cur = conn.cursor.return_value.__enter__.return_value
    _, params = cur.execute.call_args[0]
    # min_rank=3 + 區間日期 都進 SQL params
    assert 3 in params
    assert date(2026, 5, 1) in params and date(2026, 5, 18) in params


def test_market_events_empty():
    out = market_events(date(2026, 5, 1), date(2026, 5, 18), conn=_mock_conn([]))
    assert out["event_count"] == 0
    assert out["events"] == []
    assert out["by_severity"] == {}


def test_fact_to_event_extracts_numeric_value():
    assert fact_to_event({"metadata": {"z": 2.5}})["value"] == 2.5
    assert fact_to_event({"metadata": {"change_pct": -3}})["value"] == -3.0
    assert fact_to_event({"metadata": {"event_kind": "X"}})["value"] is None
    assert fact_to_event({"metadata": {"value": True}})["value"] is None  # bool 不算數值


def test_fact_to_event_handles_missing_metadata():
    ev = fact_to_event({"fact_date": date(2026, 5, 18), "source_core": "taiex_core",
                        "statement": "s", "severity": 1, "metadata": None})
    assert ev["kind"] is None
    assert ev["severity"] == "info"
    assert ev["value"] is None
