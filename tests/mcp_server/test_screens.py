"""v3.32 4 MCP cross-stock factor screen wrapper tests。

對齊 v3.31 test_stock_snapshot.py patch 風格 — 不打 PG,純 mock。
"""

from __future__ import annotations

import json
import sys
from datetime import date
from pathlib import Path
from unittest.mock import MagicMock, patch

import pytest

_REPO_ROOT = Path(__file__).resolve().parent.parent.parent
for p in (str(_REPO_ROOT / "src"), str(_REPO_ROOT)):
    if p not in sys.path:
        sys.path.insert(0, p)

from mcp_server.tools import data as data_tools


# ════════════════════════════════════════════════════════════
# Helpers
# ════════════════════════════════════════════════════════════


def _patch_screens_db(monkeypatch, *, ranking_date=date(2026, 5, 15), rows=None):
    """Patch agg._db.get_connection + fetch_cross_stock_ranked + raw SQL conn."""
    if rows is None:
        rows = [
            {"stock_id": "2330", "stock_name": "台積電", "industry_category": "半導體業",
             "momentum_rank": 1, "score_rank": 1, "vol_rank": 1, "yield_rank": 1,
             "mom_rank": 1, "concert_rank": 1, "revenue_rank": 1, "gp_rank": 1,
             "return_6m": 0.15, "return_12m_1m": 0.30, "persistent_months": 4,
             "revenue_yoy_latest": 0.25, "consecutive_positive": 3,
             "concert_days": 12, "foreign_cumulative_20d": 1000000.0,
             "cumulative_pct": 0.001,
             "f_score": 8, "profitability": 4, "leverage": 2, "efficiency": 2,
             "std_252d": 0.015, "std_36m": 0.012,
             "gross_profitability": 0.35, "industry_median_gp": 0.20,
             "industry_adj_gp": 0.15,
             "dividend_yield_pct": 5.2, "return_12m_pct": 8.5, "payout_years_5y": 5,
             "return_12m_1m": 0.30,
             "detail": {"vol_managed_scale": 1.0, "realized_vol_6m": 0.015,
                        "cross_mean_vol": 0.014}},
        ]

    # Mock cursor for fetch_cross_stock_ranked
    cur = MagicMock()
    cur.fetchone.return_value = {"d": ranking_date}
    cur.fetchall.return_value = rows
    cur.__enter__ = lambda self: self
    cur.__exit__ = lambda *a: False

    conn = MagicMock()
    conn.cursor.return_value = cur
    conn.close = MagicMock()

    # patch get_connection in _screens
    from mcp_server import _screens
    monkeypatch.setattr(_screens, "get_connection", lambda *a, **kw: conn)
    # patch fetch_cross_stock_ranked path
    monkeypatch.setattr(
        _screens, "fetch_cross_stock_ranked",
        lambda conn, *, source_table, as_of, top_n=30, rank_col, is_top_col="is_top_n":
            (ranking_date, rows),
    )
    return cur, conn


# ════════════════════════════════════════════════════════════
# Toolkit A:Monthly screen
# ════════════════════════════════════════════════════════════


class TestMonthlyScreen:

    def test_three_factors_present(self, monkeypatch):
        _patch_screens_db(monkeypatch)
        result = data_tools.monthly_screen("2026-05-15", top_n=30)

        assert result["as_of"] == "2026-05-15"
        assert result["top_n"] == 30
        assert result["toolkit"] == "A_monthly"
        for factor in ("persistent_momentum", "revenue_momentum", "institutional_concert"):
            assert factor in result["factors"]
            f = result["factors"][factor]
            assert "ranking_date" in f
            assert "top_stocks" in f
            assert isinstance(f["top_stocks"], list)

    def test_vol_overlay_present(self, monkeypatch):
        _patch_screens_db(monkeypatch)
        result = data_tools.monthly_screen("2026-05-15")
        assert "vol_managed_overlay" in result
        assert result["vol_managed_overlay"]["scale"] == 1.0   # 預設

    def test_narrative_aggregates(self, monkeypatch):
        _patch_screens_db(monkeypatch)
        result = data_tools.monthly_screen("2026-05-15")
        assert "narrative" in result
        assert "Toolkit A" in result["narrative"]
        assert "3/3" in result["narrative"]


# ════════════════════════════════════════════════════════════
# Toolkit B:Quarterly screen
# ════════════════════════════════════════════════════════════


class TestQuarterlyScreen:

    def test_three_factors_present(self, monkeypatch):
        _patch_screens_db(monkeypatch)
        result = data_tools.quarterly_screen("2026-05-15", top_n=30)
        assert result["toolkit"] == "B_quarterly"
        for factor in ("f_score", "low_volatility", "industry_adj_gp"):
            assert factor in result["factors"]

    def test_narrative_mentions_toolkit_b(self, monkeypatch):
        _patch_screens_db(monkeypatch)
        result = data_tools.quarterly_screen("2026-05-15")
        assert "Toolkit B" in result["narrative"]


# ════════════════════════════════════════════════════════════
# Toolkit C:Annual low-risk screen
# ════════════════════════════════════════════════════════════


class TestAnnualLowRiskScreen:

    def test_three_factors_present(self, monkeypatch):
        _patch_screens_db(monkeypatch)
        result = data_tools.annual_low_risk_screen("2026-05-15", top_n=30)
        assert result["toolkit"] == "C_annual_low_risk"
        for factor in ("long_term_low_vol", "dividend_yield", "mom_12_1"):
            assert factor in result["factors"]

    def test_narrative_mentions_toolkit_c(self, monkeypatch):
        _patch_screens_db(monkeypatch)
        result = data_tools.annual_low_risk_screen("2026-05-15")
        assert "Toolkit C" in result["narrative"]


# ════════════════════════════════════════════════════════════
# Layer 5:Monthly trigger scan
# ════════════════════════════════════════════════════════════


class TestMonthlyTriggerScan:

    def test_no_signals(self, monkeypatch):
        """signal_date None → return empty triggers + 友善 narrative。"""
        cur = MagicMock()
        cur.fetchone.return_value = {"d": None}
        cur.fetchall.return_value = []
        cur.__enter__ = lambda self: self
        cur.__exit__ = lambda *a: False
        conn = MagicMock()
        conn.cursor.return_value = cur
        conn.close = MagicMock()
        from mcp_server import _screens
        monkeypatch.setattr(_screens, "get_connection", lambda *a, **kw: conn)

        result = data_tools.monthly_trigger_scan("2026-05-15")
        assert result["signal_date"] is None
        assert result["positive_triggers"] == []
        assert result["negative_triggers"] == []
        assert "無" in result["narrative"]

    def test_with_signals(self, monkeypatch):
        """signal_date 有資料 + 1 positive + 1 negative trigger。"""
        cur = MagicMock()
        cur.fetchone.return_value = {"d": date(2026, 5, 15)}
        cur.fetchall.return_value = [
            {"stock_id": "1234", "trigger_type": "positive",
             "revenue_yoy_pct": 45.0, "institutional_20d": 5000000.0,
             "shares_outstanding": 1000000000.0, "institutional_pct": 0.005,
             "action_hint": "increase_20pct",
             "detail": {"rationale": "revenue_yoy=45.0% > 30% + 法人買超"},
             "stock_name": "Test Co", "industry_category": "電子業"},
            {"stock_id": "5678", "trigger_type": "negative",
             "revenue_yoy_pct": -25.0, "institutional_20d": -8000000.0,
             "shares_outstanding": 500000000.0, "institutional_pct": -0.016,
             "action_hint": "decrease_50pct",
             "detail": {"rationale": "revenue_yoy=-25% < -20%"},
             "stock_name": "Test Co 2", "industry_category": "電子業"},
        ]
        cur.__enter__ = lambda self: self
        cur.__exit__ = lambda *a: False
        conn = MagicMock()
        conn.cursor.return_value = cur
        conn.close = MagicMock()
        from mcp_server import _screens
        monkeypatch.setattr(_screens, "get_connection", lambda *a, **kw: conn)

        result = data_tools.monthly_trigger_scan("2026-05-15")
        assert result["signal_date"] == "2026-05-15"
        assert len(result["positive_triggers"]) == 1
        assert len(result["negative_triggers"]) == 1
        assert result["positive_triggers"][0]["stock_id"] == "1234"
        assert result["positive_triggers"][0]["action_hint"] == "increase_20pct"
        assert result["negative_triggers"][0]["action_hint"] == "decrease_50pct"


# ════════════════════════════════════════════════════════════
# Public surface
# ════════════════════════════════════════════════════════════


class TestMonthlyTriggerScanV332Hotfix:
    """v3.32 hotfix:加 stock_id filter + top_n_per_type 防 payload 爆量。"""

    def _mock_with_500_signals(self, monkeypatch, stock_filter: str | None = None):
        """Mock 500 signals 模擬 production-like 規模(原 user 報 ~464 / ~94KB)。"""
        signal_date = date(2026, 5, 15)
        cur = MagicMock()
        cur.fetchone.return_value = {"d": signal_date}
        # 300 positive + 200 negative,covers user's 464 scale
        rows = []
        for i in range(300):
            rows.append({
                "stock_id": f"{1000 + i:04d}", "trigger_type": "positive",
                "revenue_yoy_pct": 30.0 + (i % 50), "institutional_20d": 1e6,
                "shares_outstanding": 1e9, "institutional_pct": 0.001,
                "action_hint": "increase_20pct",
                "detail": {"rationale": f"row {i}"},
                "stock_name": f"Co {i}", "industry_category": "電子業",
            })
        for i in range(200):
            rows.append({
                "stock_id": f"{2000 + i:04d}", "trigger_type": "negative",
                "revenue_yoy_pct": -20.0 - (i % 30), "institutional_20d": -1e6,
                "shares_outstanding": 1e9, "institutional_pct": -0.002,
                "action_hint": "decrease_50pct",
                "detail": {"rationale": f"neg row {i}"},
                "stock_name": f"Co {i}", "industry_category": "電子業",
            })
        if stock_filter:
            rows = [r for r in rows if r["stock_id"] == stock_filter]
        cur.fetchall.return_value = rows
        cur.__enter__ = lambda self: self
        cur.__exit__ = lambda *a: False
        conn = MagicMock()
        conn.cursor.return_value = cur
        conn.close = MagicMock()
        from mcp_server import _screens
        monkeypatch.setattr(_screens, "get_connection", lambda *a, **kw: conn)
        return cur, conn

    def test_default_summary_mode_truncates(self, monkeypatch):
        """無 stock_id → 預設 top 20 per type;counts 仍回 total。"""
        self._mock_with_500_signals(monkeypatch)
        result = data_tools.monthly_trigger_scan("2026-05-15")
        # truncate 到 top 20 each
        assert len(result["positive_triggers"]) == 20
        assert len(result["negative_triggers"]) == 20
        # counts 仍是 total(不被截斷)
        assert result["counts"]["positive_total"] == 300
        assert result["counts"]["negative_total"] == 200
        # narrative 提到 truncate
        assert "truncate" in result["narrative"] or "top 20" in result["narrative"]

    def test_stock_id_filter_returns_all_matching(self, monkeypatch):
        """指定 stock_id → 只回該股 trigger(典型 0-2 筆)。"""
        self._mock_with_500_signals(monkeypatch, stock_filter="1005")
        result = data_tools.monthly_trigger_scan("2026-05-15", stock_id="1005")
        assert result["stock_filter"] == "1005"
        # 1005 在 positive 內 (idx 5)
        assert len(result["positive_triggers"]) == 1
        assert result["positive_triggers"][0]["stock_id"] == "1005"
        assert "1005 命中 positive" in result["narrative"]

    def test_payload_size_bounded(self, monkeypatch):
        """500 signal scale 下 payload 必 ≤ 20KB(remediation v3.32 user bug)。"""
        self._mock_with_500_signals(monkeypatch)
        result = data_tools.monthly_trigger_scan("2026-05-15")
        n = len(json.dumps(result, ensure_ascii=False))
        assert n < 20_000, f"payload {n} bytes 超 budget(user 報 94KB 必須 < 20KB)"


class TestScreensPublicSurface:

    def test_all_four_screens_callable(self):
        """v3.32:4 new screen wrappers + 4 既有 = 8 public tools。"""
        for name in ("monthly_screen", "quarterly_screen",
                     "annual_low_risk_screen", "monthly_trigger_scan"):
            assert hasattr(data_tools, name), f"data_tools.{name} 缺失"
            assert callable(getattr(data_tools, name))
