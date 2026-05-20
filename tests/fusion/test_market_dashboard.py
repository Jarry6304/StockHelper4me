"""Fusion Layer · market_dashboard 單元測試(monkeypatch fetch,不依賴真實 PG)。"""

from datetime import date
from unittest.mock import MagicMock

import fusion.market_dashboard as md_mod
from fusion.market_dashboard import market_dashboard


def _fake_fetch(rows_by_core):
    """回一個 fetch_indicator_latest stub:依 cores[0] 回對應 rows。"""

    def _fetch(conn, *, stock_id, as_of, cores):
        return rows_by_core.get(cores[0], [])

    return _fetch


def test_market_dashboard_extracts_components(monkeypatch):
    rows_by_core = {
        "taiex_core": [{"value": {"series_by_index": [
            {"index_code": "Taiex", "series": [
                {"date": "2026-05-18", "close": 21000.0, "change_pct": 1.2,
                 "trend_state": "BullishMa", "rsi": 62.0, "percentile_252": 0.88}]},
            {"index_code": "Tpex", "series": []},
        ]}}],
        "fear_greed_core": [{"value": {"series": [
            {"date": "2026-05-18", "value": 35.0, "zone": "Fear", "percentile_252": 0.22}]}}],
        "market_margin_core": [{"value": {"series": [
            {"date": "2026-05-18", "maintenance_rate": 142.0, "change_pct": -1.0,
             "zone": "Warning", "margin_balance": 3.2e11, "percentile_252": 0.4}]}}],
    }
    monkeypatch.setattr(md_mod, "fetch_indicator_latest", _fake_fetch(rows_by_core))
    out = market_dashboard(date(2026, 5, 18), conn=MagicMock())

    assert out["as_of"] == "2026-05-18"
    assert out["component_count"] == 3
    assert out["components"]["taiex_core"]["value"] == 21000.0
    assert out["components"]["taiex_core"]["percentile_252"] == 0.88
    assert out["components"]["taiex_core"]["state"] == "BullishMa"
    assert out["components"]["fear_greed_core"]["value"] == 35.0
    assert out["components"]["fear_greed_core"]["change_pct"] is None
    assert out["components"]["market_margin_core"]["margin_balance"] == 3.2e11
    assert set(out["missing"]) == {
        "us_market_core", "exchange_rate_core", "commodity_macro_core",
        "business_indicator_core",
    }


def test_market_dashboard_all_missing(monkeypatch):
    monkeypatch.setattr(md_mod, "fetch_indicator_latest", _fake_fetch({}))
    out = market_dashboard(date(2026, 5, 18), conn=MagicMock())
    assert out["component_count"] == 0
    assert len(out["missing"]) == 7
    assert out["components"] == {}


def test_market_dashboard_taiex_empty_series_is_missing(monkeypatch):
    rows = {"taiex_core": [{"value": {"series_by_index": [
        {"index_code": "Taiex", "series": []}]}}]}
    monkeypatch.setattr(md_mod, "fetch_indicator_latest", _fake_fetch(rows))
    out = market_dashboard(date(2026, 5, 18), conn=MagicMock())
    assert "taiex_core" in out["missing"]
