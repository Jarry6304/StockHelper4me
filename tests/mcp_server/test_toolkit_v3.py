"""Unit tests for MCP server v3 toolkit(public 5 tools)。

對齊 v3.4 plan §Phase C(Tool 4 magic_formula_screen + Tool 5 kalman_trend)。

策略:
- TestMagicFormulaScreen:mock conn cursor 回固定 rows,驗 top_stocks 結構
  / narrative / stats / payload size
- TestKalmanTrend:mock agg.as_of 回固定 snapshot,驗 regime / band / narrative
- TestToolkitV3PublicSurface:driveby 確認 5 個 public tools 都 callable
"""

from __future__ import annotations

import json
import sys
from datetime import date, timedelta
from pathlib import Path
from unittest.mock import MagicMock

_REPO_ROOT = Path(__file__).resolve().parent.parent.parent
_SRC_ROOT = _REPO_ROOT / "src"
for p in (str(_SRC_ROOT), str(_REPO_ROOT)):
    if p not in sys.path:
        sys.path.insert(0, p)

import pytest

from mcp_server.tools import data as data_tools


# ════════════════════════════════════════════════════════════
# Fixtures
# ════════════════════════════════════════════════════════════

def _mk_mf_cursor(
    *,
    ranking_date: date,
    universe_size: int = 1400,
    top_rows: list[dict] | None = None,
    stats_row: dict | None = None,
):
    """組裝 MagicMock cursor 對 magic_formula_screen 3 個 query 順序回值。"""
    cursor = MagicMock()
    # Query 1: SELECT MAX(date)
    # Query 2: stats(universe_size + median EY / ROIC)
    # Query 3: top N rows
    fetchone_iter = iter([
        {"d": ranking_date},
        stats_row or {
            "universe_size": universe_size,
            "median_ey":     0.045,
            "median_roic":   0.08,
        },
    ])
    cursor.fetchone.side_effect = lambda: next(fetchone_iter)
    cursor.fetchall.return_value = top_rows or []
    cursor.__enter__ = lambda self: self
    cursor.__exit__ = lambda *args: False
    return cursor


def _patch_mf_get_connection(monkeypatch, cursor):
    """Mock mcp_server._conn.get_connection 回 conn(cursor 給定)。"""
    from mcp_server import _magic_formula

    conn = MagicMock()
    conn.cursor.return_value = cursor
    conn.close = MagicMock()
    monkeypatch.setattr(_magic_formula, "get_connection", lambda *a, **kw: conn)


# ════════════════════════════════════════════════════════════
# Magic Formula
# ════════════════════════════════════════════════════════════


class TestMagicFormulaScreen:

    def test_top_stocks_structure(self, monkeypatch):
        """top_stocks 各 row 應有完整 9 個 keys + rank 從 1 遞增。"""
        rows = [
            {"stock_id": "2330", "earnings_yield": 0.082, "roic": 0.31,
             "ey_rank": 145, "roic_rank": 12, "combined_rank": 157,
             "stock_name": "台積電", "industry_category": "半導體業"},
            {"stock_id": "2317", "earnings_yield": 0.065, "roic": 0.20,
             "ey_rank": 220, "roic_rank": 35, "combined_rank": 255,
             "stock_name": "鴻海", "industry_category": "電子零組件業"},
        ]
        cur = _mk_mf_cursor(ranking_date=date(2026, 5, 15), top_rows=rows)
        _patch_mf_get_connection(monkeypatch, cur)

        result = data_tools.magic_formula_screen("2026-05-15", top_n=30)
        assert result["as_of"] == "2026-05-15"
        assert result["ranking_date"] == "2026-05-15"
        assert result["top_n"] == 30
        assert result["universe_size"] == 1400
        assert len(result["top_stocks"]) == 2

        s = result["top_stocks"][0]
        for key in ("rank", "stock_id", "name", "industry", "earnings_yield",
                    "roic", "ey_rank", "roic_rank", "combined_rank"):
            assert key in s
        assert result["top_stocks"][0]["rank"] == 1
        assert result["top_stocks"][1]["rank"] == 2
        assert result["top_stocks"][0]["stock_id"] == "2330"
        assert result["top_stocks"][0]["name"] == "台積電"

    def test_stats_present(self, monkeypatch):
        cur = _mk_mf_cursor(
            ranking_date=date(2026, 5, 15),
            top_rows=[{"stock_id": "2330", "earnings_yield": 0.082, "roic": 0.31,
                       "ey_rank": 100, "roic_rank": 10, "combined_rank": 110,
                       "stock_name": "TSMC", "industry_category": "半導體業"}],
        )
        _patch_mf_get_connection(monkeypatch, cur)
        result = data_tools.magic_formula_screen("2026-05-15", top_n=30)
        stats = result["stats"]
        assert "median_ey" in stats and "median_roic" in stats
        assert stats["min_combined_rank"] == 110
        assert stats["max_combined_rank_in_top_n"] == 110

    def test_empty_when_no_data(self, monkeypatch):
        """無 magic_formula_ranked_derived 資料 → 回 empty narrative。"""
        cur = MagicMock()
        cur.fetchone.return_value = {"d": None}     # MAX(date) → NULL
        cur.fetchall.return_value = []
        cur.__enter__ = lambda self: self
        cur.__exit__ = lambda *args: False
        _patch_mf_get_connection(monkeypatch, cur)

        result = data_tools.magic_formula_screen("2026-05-15", top_n=30)
        assert result["ranking_date"] is None
        assert result["top_stocks"] == []
        assert "資料缺失" in result["narrative"]

    def test_payload_size_bounded(self, monkeypatch):
        """30 個 stocks 的 payload 應 < 10 KB(對齊 plan ≤ 5K tokens budget)。"""
        rows = [
            {"stock_id": f"{1100 + i:04d}", "earnings_yield": 0.05 + 0.001 * i,
             "roic": 0.10 + 0.005 * i, "ey_rank": 100 + i, "roic_rank": 50 + i,
             "combined_rank": 150 + 2 * i, "stock_name": f"Test Co {i}",
             "industry_category": "電子零組件業"}
            for i in range(30)
        ]
        cur = _mk_mf_cursor(ranking_date=date(2026, 5, 15), top_rows=rows)
        _patch_mf_get_connection(monkeypatch, cur)
        result = data_tools.magic_formula_screen("2026-05-15", top_n=30)
        n = len(json.dumps(result, ensure_ascii=False))
        assert n < 10_000, f"payload {n} bytes 超 budget 10 KB"
        assert len(result["top_stocks"]) == 30


# ════════════════════════════════════════════════════════════
# Kalman Trend
# ════════════════════════════════════════════════════════════


def _mk_indicator_row(value: dict, source_core: str = "kalman_filter_core"):
    """Mock IndicatorRow(對齊 src/agg/_types.py)。"""
    row = MagicMock()
    row.source_core = source_core
    row.value = value
    return row


def _mk_fact(source_core: str, fact_date: date, metadata: dict):
    """Mock FactRow。"""
    f = MagicMock()
    f.source_core = source_core
    f.fact_date = fact_date
    f.metadata = metadata
    return f


def _patch_agg_as_of(monkeypatch, *, indicator_value: dict | None,
                     facts: list | None = None):
    """Mock agg.as_of 回 snapshot 對應 _kalman.compute_kalman_trend 用。"""
    from mcp_server import _kalman

    snapshot = MagicMock()
    if indicator_value is not None:
        snapshot.indicator_latest = {
            "kalman_filter_core/daily": _mk_indicator_row(indicator_value),
        }
    else:
        snapshot.indicator_latest = {}
    snapshot.facts = facts or []

    # 不用 patch sys.modules.agg.as_of(它在 _kalman 內 lazy import)
    def fake_as_of(*args, **kwargs):
        return snapshot
    # 走 agg 模組
    import agg as agg_mod
    monkeypatch.setattr(agg_mod, "as_of", fake_as_of)
    return snapshot


class TestKalmanTrend:

    def test_regime_stable_up(self, monkeypatch):
        _patch_agg_as_of(
            monkeypatch,
            indicator_value={
                "raw_close": 1234.5,
                "smoothed_price": 1220.3,
                "velocity": 0.42,
                "uncertainty": 8.5,
                "regime": "StableUp",
            },
        )
        result = data_tools.kalman_trend("2330", "2026-05-15", lookback_days=180)
        assert result["stock_id"] == "2330"
        assert result["as_of"] == "2026-05-15"
        assert result["current_price"] == 1234.5
        assert result["smoothed_price"] == 1220.3
        assert result["regime"] == "StableUp"
        assert result["regime_label"] == "穩定上漲"
        # uncertainty_band = smoothed ± uncertainty
        assert result["uncertainty_band"] == [1211.8, 1228.8]
        # deviation = (1234.5 - 1220.3) / 8.5 ≈ 1.67
        assert abs(result["deviation_sigma"] - 1.67) < 0.05

    def test_recent_regime_changes_extracted(self, monkeypatch):
        as_of = date(2026, 5, 15)
        facts = [
            _mk_fact("kalman_filter_core", as_of - timedelta(days=5),
                     {"from_regime": "Sideway", "to_regime": "StableUp"}),
            _mk_fact("kalman_filter_core", as_of - timedelta(days=20),
                     {"from_regime": "Decelerating", "to_regime": "Sideway"}),
            # 不同 core 的 fact 應被過濾
            _mk_fact("macd_core", as_of - timedelta(days=3),
                     {"foo": "bar"}),
        ]
        _patch_agg_as_of(
            monkeypatch,
            indicator_value={"raw_close": 100, "smoothed_price": 99,
                             "velocity": 0.1, "uncertainty": 1.0,
                             "regime": "StableUp"},
            facts=facts,
        )
        result = data_tools.kalman_trend("2330", "2026-05-15")
        changes = result["recent_regime_changes"]
        assert len(changes) == 2
        assert changes[0]["to"] == "StableUp"
        assert changes[1]["from"] == "Decelerating"

    def test_empty_when_no_indicator(self, monkeypatch):
        _patch_agg_as_of(monkeypatch, indicator_value=None)
        result = data_tools.kalman_trend("9999", "2026-05-15")
        assert result["regime"] is None
        assert "無 kalman_filter_core 資料" in result["narrative"]

    def test_payload_size_bounded(self, monkeypatch):
        _patch_agg_as_of(
            monkeypatch,
            indicator_value={
                "raw_close": 1234.5, "smoothed_price": 1220.3,
                "velocity": 0.42, "uncertainty": 8.5, "regime": "StableUp",
            },
            facts=[
                _mk_fact("kalman_filter_core", date(2026, 5, 10 + i),
                         {"from_regime": "Sideway", "to_regime": "StableUp"})
                for i in range(5)
            ],
        )
        result = data_tools.kalman_trend("2330", "2026-05-15")
        n = len(json.dumps(result, ensure_ascii=False))
        assert n < 3_000, f"payload {n} bytes 超 budget 3 KB"


class TestToolkitV3PublicSurface:
    """確認 5 個 public tools 都 importable + 可 invoke(callable check 不打 PG)。"""

    def test_all_five_tools_present(self):
        for name in ("neely_forecast", "stock_health", "market_context",
                     "magic_formula_screen", "kalman_trend"):
            assert hasattr(data_tools, name), f"data_tools.{name} 缺失"
            assert callable(getattr(data_tools, name))
