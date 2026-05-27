"""Fusion Layer · indicator_assembly 單元測試(monkeypatch fetch,不依賴真實 PG)。"""

from datetime import date
from unittest.mock import MagicMock

import fusion.indicator_assembly as ia_mod
from fusion.indicator_assembly import (
    INDICATOR_STACK_PRESETS,
    assemble_indicators,
    category_indicators,
)


def test_category_indicators_filters_and_normalizes():
    # 省略 → 全類別
    assert "macd_core" in category_indicators("momentum")
    assert len(category_indicators("volatility")) == 4
    # 指定 + 正規化("macd" → "macd_core",大小寫不拘)
    assert category_indicators("momentum", ["macd", "RSI"]) == ["macd_core", "rsi_core"]
    # 不在該類別的被濾掉
    assert category_indicators("volume", ["macd", "obv"]) == ["obv_core"]
    assert category_indicators("nonsense") == []


def test_assemble_indicators_series_and_events(monkeypatch):
    def fake_iv(conn, *, stock_id, as_of, cores):
        if cores[0] == "macd_core":
            return [{"value_date": date(2026, 5, 18),
                     "value": {"series": [{"date": "2026-05-18", "macd": 1.2}]}}]
        return []

    def fake_facts(conn, *, stock_ids, as_of, lookback_days, cores):
        if cores[0] == "macd_core":
            return [{"fact_date": date(2026, 5, 18), "source_core": "macd_core",
                     "statement": "MACD GoldenCross", "severity": 1,
                     "metadata": {"event_kind": "GoldenCross"}}]
        return []

    monkeypatch.setattr(ia_mod, "fetch_indicator_latest", fake_iv)
    monkeypatch.setattr(ia_mod, "fetch_facts", fake_facts)

    out = assemble_indicators("2330", date(2026, 5, 18), ["macd_core", "rsi_core"],
                              conn=MagicMock())
    assert out["indicator_count"] == 1
    assert out["missing"] == ["rsi_core"]  # rsi 無 indicator_values 也無 facts
    macd = out["indicators"]["macd_core"]
    assert macd["value_date"] == "2026-05-18"
    assert macd["series"]["series"][0]["macd"] == 1.2
    assert macd["events"][0]["kind"] == "GoldenCross"
    assert macd["events"][0]["severity"] == "info"


def test_assemble_indicators_all_missing(monkeypatch):
    monkeypatch.setattr(ia_mod, "fetch_indicator_latest", lambda *a, **k: [])
    monkeypatch.setattr(ia_mod, "fetch_facts", lambda *a, **k: [])
    out = assemble_indicators("2330", date(2026, 5, 18), ["macd_core"], conn=MagicMock())
    assert out["indicator_count"] == 0
    assert out["missing"] == ["macd_core"]


def test_stack_presets_well_formed():
    assert set(INDICATOR_STACK_PRESETS) == {"default", "day_trade", "swing", "position"}
    for cores in INDICATOR_STACK_PRESETS.values():
        assert cores and all(c.endswith("_core") for c in cores)


# v4.28+ Bug B fix:lookback_days slice indicator series to bound MCP payload size


def test_series_sliced_to_lookback_days(monkeypatch):
    long_series = [{"date": f"2026-{m:02d}-15", "macd": float(m)} for m in range(1, 13)] \
                  + [{"date": f"2025-{m:02d}-15", "macd": float(m)} for m in range(1, 13)]
    assert len(long_series) == 24

    def fake_iv(conn, *, stock_id, as_of, cores):
        return [{"value_date": date(2026, 5, 18),
                 "value": {"series": long_series, "meta": "preserved"}}]

    monkeypatch.setattr(ia_mod, "fetch_indicator_latest", fake_iv)
    monkeypatch.setattr(ia_mod, "fetch_facts", lambda *a, **k: [])

    out = assemble_indicators("2330", date(2026, 5, 18), ["macd_core"],
                              lookback_days=5, conn=MagicMock())
    series = out["indicators"]["macd_core"]["series"]["series"]
    assert len(series) == 5
    assert series == long_series[-5:]
    assert out["indicators"]["macd_core"]["series"]["meta"] == "preserved"


def test_no_slice_when_series_shorter_than_lookback(monkeypatch):
    short_series = [{"date": "2026-05-18", "macd": 1.0}]

    def fake_iv(conn, *, stock_id, as_of, cores):
        return [{"value_date": date(2026, 5, 18),
                 "value": {"series": short_series}}]

    monkeypatch.setattr(ia_mod, "fetch_indicator_latest", fake_iv)
    monkeypatch.setattr(ia_mod, "fetch_facts", lambda *a, **k: [])

    out = assemble_indicators("2330", date(2026, 5, 18), ["macd_core"],
                              lookback_days=60, conn=MagicMock())
    assert out["indicators"]["macd_core"]["series"]["series"] == short_series


def test_non_list_series_passthrough(monkeypatch):
    pattern_blob = {"series_by_spec": {"hammer": [1, 2, 3]}, "series": {"oops": "dict"}}

    def fake_iv(conn, *, stock_id, as_of, cores):
        return [{"value_date": date(2026, 5, 18), "value": pattern_blob}]

    monkeypatch.setattr(ia_mod, "fetch_indicator_latest", fake_iv)
    monkeypatch.setattr(ia_mod, "fetch_facts", lambda *a, **k: [])

    out = assemble_indicators("2330", date(2026, 5, 18), ["candlestick_pattern_core"],
                              lookback_days=60, conn=MagicMock())
    # Non-list series should not be sliced (guarded by isinstance(series, list))
    assert out["indicators"]["candlestick_pattern_core"]["series"] == pattern_blob


def test_value_blob_not_dict_passthrough(monkeypatch):
    def fake_iv(conn, *, stock_id, as_of, cores):
        return [{"value_date": date(2026, 5, 18), "value": None}]

    monkeypatch.setattr(ia_mod, "fetch_indicator_latest", fake_iv)
    monkeypatch.setattr(ia_mod, "fetch_facts", lambda *a, **k: [{
        "fact_date": date(2026, 5, 18), "source_core": "macd_core",
        "statement": "x", "severity": 1, "metadata": {"event_kind": "X"},
    }])

    out = assemble_indicators("2330", date(2026, 5, 18), ["macd_core"],
                              lookback_days=60, conn=MagicMock())
    assert out["indicators"]["macd_core"]["series"] is None
