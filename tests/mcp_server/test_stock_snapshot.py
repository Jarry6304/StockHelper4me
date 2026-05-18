"""v3.31 stock_snapshot 6-in-1 wrapper 測試。

對齊 v3.22 既有 6 個 helper 測試風格(test_toolkit_v3.py),不打 PG,純 mock
6 個 compute_* helper 驗 wrapper 行為:

- test_six_sections_present:全部 helper 成功 → 9 個 top-level keys
- test_partial_failure_graceful:某 helper raise → 該 section 變 error,其他 5 ok
- test_payload_bounded:typical production payload ≤ 15KB
- test_narrative_aggregates_signals:narrative 包含主要 signal
- test_appears_in_public_surface:確認 stock_snapshot 已 export
"""

from __future__ import annotations

import json
import sys
from pathlib import Path
from unittest.mock import patch

_REPO_ROOT = Path(__file__).resolve().parent.parent.parent
for p in (str(_REPO_ROOT / "src"), str(_REPO_ROOT)):
    if p not in sys.path:
        sys.path.insert(0, p)

from mcp_server.tools import data as data_tools


# ════════════════════════════════════════════════════════════
# Shared fixture:patch 全 6 個 compute_* helper
# ════════════════════════════════════════════════════════════

_HEALTH_OK = {
    "stock_id": "2330", "as_of": "2026-05-15",
    "current_price": 2265.0, "overall_score": 35,
    "dimensions": {
        "technical":   {"score": 40, "trend": "bullish"},
        "chip":        {"score": 30, "trend": "bullish"},
        "valuation":   {"score": -10, "trend": "mixed"},
        "fundamental": {"score": 50, "trend": "bullish"},
    },
    "top_signals": [{"date": "2026-05-14", "core": "ma_core", "kind": "GoldenCross",
                     "sign": 1, "weight": 1.5}] * 5,
    "narrative": "2330 技術 + 籌碼皆轉強。",
}

_LOAN_OK = {
    "stock_id": "2330", "as_of": "2026-05-15", "snapshot_date": "2026-05-15",
    "categories": {"margin": {"balance": 26483, "change_pct": 1.2, "ratio": 0.24},
                   "unrestricted_loan": {"balance": 74400, "change_pct": -0.5, "ratio": 0.67}},
    "total_balance": 110000, "dominant_category": "unrestricted_loan",
    "dominant_category_label": "無限制借券", "concentration_ratio": 0.67,
    "concentration_alert": False, "narrative": "無集中警示。",
}

_BLOCK_OK = {
    "stock_id": "2330", "as_of": "2026-05-15", "period_days": 30,
    "active_days": 19, "total_volume": 152300, "total_trading_money": 35_200_000_000,
    "matching_share_avg": 0.65, "largest_single_trade_money": 80_000_000,
    "matching_spike_dates": ["2026-04-25"], "narrative": "30 日 19 日有大宗。",
}

_RISK_OK = {
    "stock_id": "2330", "as_of": "2026-05-15",
    "current_status": {"in_disposition_period": False, "severity": None,
                       "severity_label": None, "period_start": None,
                       "period_end": None, "days_remaining": None},
    "history_60d": [], "escalation_count_60d": 0,
    "narrative": "2330 過去 60 日無處置。",
}

_MARKET_OK = {
    "as_of": "2026-05-15", "overall_climate": "neutral", "climate_score": 5.3,
    "components": {"taiex": {"score": 10, "fact_count": 5},
                   "us_market": {"score": 8, "fact_count": 3},
                   "fear_greed": {"score": -5, "fact_count": 2}},
    "systemic_risks": [], "narrative": "大盤中性偏多。",
}

_COMMODITY_OK = {
    "as_of": "2026-05-15", "snapshot_date": "2026-05-15",
    "commodities": [{"name": "GOLD", "price": 2630.5, "return_pct": 0.85,
                     "return_z_score": 1.23, "momentum_state": "up",
                     "streak_days": 4, "spike_alert": False}],
    "lookback_days": 60, "narrative": "GOLD 連續 4 日上漲。",
}


def _patch_all_helpers(
    *,
    health=_HEALTH_OK, loan=_LOAN_OK, block=_BLOCK_OK,
    risk=_RISK_OK, market=_MARKET_OK, commodity=_COMMODITY_OK,
):
    """Patch 全 6 個 compute_* helper(value=None → raise RuntimeError)。"""
    def _mk(payload):
        if payload is None:
            return lambda *a, **kw: (_ for _ in ()).throw(RuntimeError("simulated failure"))
        return lambda *a, **kw: payload

    return [
        patch("mcp_server._health.compute_stock_health", side_effect=_mk(health)),
        patch("mcp_server._loan_collateral.compute_loan_collateral_snapshot",
              side_effect=_mk(loan)),
        patch("mcp_server._block_trade.compute_block_trade_summary",
              side_effect=_mk(block)),
        patch("mcp_server._risk_alert.compute_risk_alert_status",
              side_effect=_mk(risk)),
        patch("mcp_server._climate.compute_market_context", side_effect=_mk(market)),
        patch("mcp_server._commodity_macro.compute_commodity_macro_snapshot",
              side_effect=_mk(commodity)),
    ]


# ════════════════════════════════════════════════════════════
# Tests
# ════════════════════════════════════════════════════════════


class TestStockSnapshot:

    def test_six_sections_present(self):
        """全 helper 成功 → 9 top-level keys(6 section + stock_id + as_of + narrative)。"""
        patches = _patch_all_helpers()
        for p in patches: p.start()
        try:
            result = data_tools.stock_snapshot("2330", "2026-05-15")
        finally:
            for p in reversed(patches): p.stop()

        assert result["stock_id"] == "2330"
        assert result["as_of"] == "2026-05-15"
        for section in ("health", "loan_collateral", "block_trade",
                        "risk_alert", "market_context", "commodity_macro"):
            assert section in result, f"missing section {section}"
            assert "error" not in result[section], f"{section} unexpectedly errored"
        assert "narrative" in result and len(result["narrative"]) > 0

    def test_partial_failure_graceful(self):
        """某 1 個 helper raise → 該 section 變 error key,其他 5 個 ok。"""
        patches = _patch_all_helpers(loan=None)   # loan_collateral 失敗
        for p in patches: p.start()
        try:
            result = data_tools.stock_snapshot("2330", "2026-05-15")
        finally:
            for p in reversed(patches): p.stop()

        # 失敗 section 含 error
        assert "error" in result["loan_collateral"]
        assert result["loan_collateral"]["section"] == "loan_collateral"
        assert "RuntimeError" in result["loan_collateral"]["error"]

        # 其他 5 個 ok
        for section in ("health", "block_trade", "risk_alert",
                        "market_context", "commodity_macro"):
            assert "error" not in result[section], \
                f"{section} should not have errored"

    def test_multiple_failures_graceful(self):
        """3 個 helper 同時失敗 → 各自獨立 error,其他 3 個 ok。"""
        patches = _patch_all_helpers(health=None, market=None, commodity=None)
        for p in patches: p.start()
        try:
            result = data_tools.stock_snapshot("2330", "2026-05-15")
        finally:
            for p in reversed(patches): p.stop()

        for failed in ("health", "market_context", "commodity_macro"):
            assert "error" in result[failed]
        for ok in ("loan_collateral", "block_trade", "risk_alert"):
            assert "error" not in result[ok]

    def test_payload_bounded(self):
        """typical production 6 section payload ≤ 15KB(對齊 plan ~10KB 預估)。"""
        patches = _patch_all_helpers()
        for p in patches: p.start()
        try:
            result = data_tools.stock_snapshot("2330", "2026-05-15")
        finally:
            for p in reversed(patches): p.stop()
        n = len(json.dumps(result, ensure_ascii=False))
        assert n < 15_000, f"payload {n} bytes 超 budget 15 KB"

    def test_narrative_aggregates_signals(self):
        """narrative 應提到 overall_score / climate / 若有警示也應在內。"""
        # 加 1 個處置警示測 narrative pickup
        risk_with_disposition = {
            **_RISK_OK,
            "current_status": {
                "in_disposition_period": True, "severity": "disposition",
                "severity_label": "處置股(分盤撮合)", "period_start": "2026-05-08",
                "period_end": "2026-05-21", "days_remaining": 6,
            },
        }
        patches = _patch_all_helpers(risk=risk_with_disposition)
        for p in patches: p.start()
        try:
            result = data_tools.stock_snapshot("3030", "2026-05-15")
        finally:
            for p in reversed(patches): p.stop()
        narr = result["narrative"]
        # narrative 含個股 health overall_score(+35 → 偏多)+ 處置警示
        assert "3030" in narr
        assert "處置" in narr

    def test_all_section_failures_still_returns_shape(self):
        """6 個 helper 全失敗 → return shape 仍完整,narrative 走 fallback。"""
        patches = _patch_all_helpers(
            health=None, loan=None, block=None,
            risk=None, market=None, commodity=None,
        )
        for p in patches: p.start()
        try:
            result = data_tools.stock_snapshot("9999", "2026-05-15")
        finally:
            for p in reversed(patches): p.stop()
        # shape 仍完整(9 top-level keys)
        for key in ("stock_id", "as_of", "health", "loan_collateral",
                    "block_trade", "risk_alert", "market_context",
                    "commodity_macro", "narrative"):
            assert key in result
        # narrative 走 fallback(因為 4 個 signal source 都 error)
        assert "9999" in result["narrative"]


class TestStockSnapshotPublicSurface:

    def test_stock_snapshot_callable(self):
        """v3.31:確認 stock_snapshot 已 export 進 mcp_server.tools.data。"""
        assert hasattr(data_tools, "stock_snapshot")
        assert callable(data_tools.stock_snapshot)

    def test_hidden_six_helpers_still_callable(self):
        """6 個被 hidden 的 helper 仍可從 Python 直接呼叫(dashboard 用)。"""
        for name in (
            "stock_health", "market_context", "loan_collateral_snapshot",
            "block_trade_summary", "risk_alert_status", "commodity_macro_snapshot",
        ):
            assert hasattr(data_tools, name), f"{name} 被誤刪"
            assert callable(getattr(data_tools, name))
