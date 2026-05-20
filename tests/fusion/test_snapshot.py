"""Fusion Layer · snapshot(10-in-1)單元測試(mock,不依賴真實 PG)。"""

from datetime import date
from unittest.mock import MagicMock

import fusion.snapshot as snap_mod
from fusion.snapshot import (
    _fact_section,
    _fundamentals,
    _narrative,
    _technical_summary,
    stock_snapshot,
)


def _conn_with_fetchone(row):
    conn = MagicMock()
    cur = MagicMock()
    cur.fetchone.return_value = row
    conn.cursor.return_value.__enter__.return_value = cur
    return conn


def test_narrative_composes_signals():
    txt = _narrative("2330", {"overall_score": 45}, {"overall_climate": "bullish"}, {})
    assert "2330" in txt and "偏多" in txt and "bullish" in txt


def test_narrative_fallback_when_empty():
    assert "2330" in _narrative("2330", {}, {}, {})


def test_fundamentals_reads_two_silver_tables():
    conn = _conn_with_fetchone({"date": date(2026, 5, 18), "per": 18.5, "pbr": 4.2,
                                "dividend_yield": 2.1, "market_value_weight": 0.03})
    out = _fundamentals(conn, "2330", date(2026, 5, 18))
    assert out["valuation"]["per"] == 18.5
    assert out["valuation"]["date"] == "2026-05-18"
    assert "monthly_revenue" in out  # 同 conn 第二次查(mock 回同 row,但 key 存在)


def test_fact_section_maps_events(monkeypatch):
    facts = [{"fact_date": date(2026, 5, 18), "source_core": "institutional_core",
              "statement": "...", "severity": 2, "metadata": {"event_kind": "NetBuyStreak"}}]
    monkeypatch.setattr(snap_mod, "fetch_facts", lambda *a, **k: facts)
    out = _fact_section(MagicMock(), "2330", date(2026, 5, 18), "institutional_core")
    assert out["source_core"] == "institutional_core"
    assert out["event_count"] == 1
    assert out["recent_events"][0]["kind"] == "NetBuyStreak"


def test_technical_summary_groups_by_core(monkeypatch):
    facts = [
        {"fact_date": date(2026, 5, 18), "source_core": "macd_core",
         "statement": "s", "severity": 1, "metadata": {"event_kind": "GoldenCross"}},
        {"fact_date": date(2026, 5, 17), "source_core": "rsi_core",
         "statement": "s", "severity": 1, "metadata": {"event_kind": "Oversold"}},
    ]
    monkeypatch.setattr(snap_mod, "fetch_facts", lambda *a, **k: facts)
    out = _technical_summary(MagicMock(), "2330", date(2026, 5, 18))
    assert out["signal_count"] == 2
    assert out["by_core"] == {"macd_core": 1, "rsi_core": 1}


def test_stock_snapshot_assembles_10_sections(monkeypatch):
    # 6 個 mcp_server helper → 簡單 stub
    monkeypatch.setattr("mcp_server._health.compute_stock_health",
                        lambda *a, **k: {"overall_score": 10})
    monkeypatch.setattr("mcp_server._climate.compute_market_context",
                        lambda *a, **k: {"overall_climate": "neutral"})
    monkeypatch.setattr("mcp_server._loan_collateral.compute_loan_collateral_snapshot",
                        lambda *a, **k: {"ok": True})
    monkeypatch.setattr("mcp_server._block_trade.compute_block_trade_summary",
                        lambda *a, **k: {"ok": True})
    monkeypatch.setattr("mcp_server._risk_alert.compute_risk_alert_status",
                        lambda *a, **k: {"ok": True})
    monkeypatch.setattr("mcp_server._commodity_macro.compute_commodity_macro_snapshot",
                        lambda *a, **k: {"ok": True})
    monkeypatch.setattr(snap_mod, "fetch_facts", lambda *a, **k: [])

    out = stock_snapshot("2330", date(2026, 5, 18), conn=_conn_with_fetchone(None))
    for key in ("health", "loan_collateral", "block_trade", "risk_alert",
                "market_context", "commodity_macro", "fundamentals",
                "institutional", "shareholder", "technical_summary", "narrative"):
        assert key in out, f"missing section: {key}"
    assert out["stock_id"] == "2330"
    assert out["health"]["overall_score"] == 10
