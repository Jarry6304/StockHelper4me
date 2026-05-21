"""v4.19 fusion consolidated tools(market_overview / stock_levels / indicators)測試。

對齊 test_stock_snapshot.py 風格 — 不打 PG,純 mock fusion.* 子函式驗 wrapper:
- 各 consolidated tool 正確合併子段
- 某子段 raise → 該段 graceful 變 {"error": ...},其他段不受影響
- indicators 選擇優先序 cores > groups > preset > default
- payload bounded
- 被整併的 10 個舊 fusion function 仍 callable(dashboard 用)
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

from mcp_server.tools import data as data_tools  # noqa: E402


def _raise(*_a, **_kw):
    raise RuntimeError("simulated failure")


# ════════════════════════════════════════════════════════════
# market_overview(D 視角)
# ════════════════════════════════════════════════════════════

_DASHBOARD_OK = {"as_of": "2026-05-15", "component_count": 7,
                 "components": {}, "missing": []}
_EVENTS_OK = {"start_date": "2026-04-15", "end_date": "2026-05-15",
              "severity_min": "notable", "event_count": 3,
              "by_severity": {}, "events": []}


class TestMarketOverview:

    def test_both_sections_present(self):
        with patch("fusion.market_dashboard.market_dashboard", return_value=_DASHBOARD_OK), \
             patch("fusion.market_events.market_events", return_value=_EVENTS_OK):
            r = data_tools.market_overview("2026-05-15")
        assert r["as_of"] == "2026-05-15"
        assert r["dashboard"] == _DASHBOARD_OK
        assert r["events"] == _EVENTS_OK

    def test_dashboard_failure_graceful(self):
        with patch("fusion.market_dashboard.market_dashboard", side_effect=_raise), \
             patch("fusion.market_events.market_events", return_value=_EVENTS_OK):
            r = data_tools.market_overview("2026-05-15")
        assert "error" in r["dashboard"]
        assert r["dashboard"]["section"] == "dashboard"
        assert "error" not in r["events"]

    def test_events_window_derived_from_lookback(self):
        captured: dict = {}

        def _cap(start, end, **_kw):
            captured["start"], captured["end"] = start, end
            return _EVENTS_OK

        with patch("fusion.market_dashboard.market_dashboard", return_value=_DASHBOARD_OK), \
             patch("fusion.market_events.market_events", side_effect=_cap):
            data_tools.market_overview("2026-05-15", events_lookback_days=10)
        assert captured["end"].isoformat() == "2026-05-15"
        assert captured["start"].isoformat() == "2026-05-05"


# ════════════════════════════════════════════════════════════
# stock_levels(B 視角)
# ════════════════════════════════════════════════════════════

_KL_OK = {"stock_id": "2330", "as_of": "2026-05-15", "level_count": 4, "levels": []}
_PAT_OK = {"stock_id": "2330", "as_of": "2026-05-15", "pattern_count": 2, "patterns": []}
_SL_OK = {"stock_id": "2330", "as_of": "2026-05-15", "direction": "long",
          "entry_price": 100.0, "atr": 3.2, "stops": {}, "targets": {}}


class TestStockLevels:

    def test_no_entry_price_stop_loss_none(self):
        with patch("fusion.key_levels.key_levels", return_value=_KL_OK), \
             patch("fusion.pattern_scan.pattern_scan", return_value=_PAT_OK):
            r = data_tools.stock_levels("2330", "2026-05-15")
        assert r["key_levels"] == _KL_OK
        assert r["patterns"] == _PAT_OK
        assert r["stop_loss"] is None

    def test_entry_price_computes_stop_loss(self):
        with patch("fusion.key_levels.key_levels", return_value=_KL_OK), \
             patch("fusion.pattern_scan.pattern_scan", return_value=_PAT_OK), \
             patch("fusion.stop_loss.stop_loss", return_value=_SL_OK):
            r = data_tools.stock_levels("2330", "2026-05-15", entry_price=100.0)
        assert r["stop_loss"] == _SL_OK

    def test_partial_failure_graceful(self):
        with patch("fusion.key_levels.key_levels", return_value=_KL_OK), \
             patch("fusion.pattern_scan.pattern_scan", side_effect=_raise):
            r = data_tools.stock_levels("2330", "2026-05-15")
        assert "error" not in r["key_levels"]
        assert "error" in r["patterns"]
        assert r["patterns"]["section"] == "patterns"
        assert r["stop_loss"] is None


# ════════════════════════════════════════════════════════════
# indicators(E 視角)
# ════════════════════════════════════════════════════════════

def _stub_assemble(stock_id, as_of, selected, **_kw):
    return {
        "stock_id": stock_id,
        "as_of": as_of.isoformat(),
        "indicator_count": len(selected),
        "indicators": {c: {"value_date": None, "series": None, "events": []}
                       for c in selected},
        "missing": [],
    }


class TestIndicators:

    def test_default_uses_default_preset(self):
        with patch("fusion.indicator_assembly.assemble_indicators",
                   side_effect=_stub_assemble):
            r = data_tools.indicators("2330", "2026-05-15")
        assert r["selection"] == {"mode": "preset", "value": "default"}
        assert set(r["indicators"]) == {
            "macd_core", "rsi_core", "kd_core", "bollinger_core", "ma_core"}

    def test_preset_selection(self):
        with patch("fusion.indicator_assembly.assemble_indicators",
                   side_effect=_stub_assemble):
            r = data_tools.indicators("2330", "2026-05-15", preset="swing")
        assert r["selection"] == {"mode": "preset", "value": "swing"}
        assert set(r["indicators"]) == {
            "macd_core", "ma_core", "adx_core", "atr_core"}

    def test_groups_selection(self):
        with patch("fusion.indicator_assembly.assemble_indicators",
                   side_effect=_stub_assemble):
            r = data_tools.indicators("2330", "2026-05-15", groups=["volume"])
        assert r["selection"]["mode"] == "groups"
        assert set(r["indicators"]) == {"obv_core", "vwap_core", "mfi_core"}

    def test_cores_takes_precedence_and_normalizes(self):
        with patch("fusion.indicator_assembly.assemble_indicators",
                   side_effect=_stub_assemble):
            r = data_tools.indicators("2330", "2026-05-15",
                                      cores=["macd", "RSI", "atr_core"],
                                      groups=["volume"], preset="swing")
        assert r["selection"]["mode"] == "cores"
        assert set(r["indicators"]) == {"macd_core", "rsi_core", "atr_core"}

    def test_invalid_preset_falls_back_to_default(self):
        with patch("fusion.indicator_assembly.assemble_indicators",
                   side_effect=_stub_assemble):
            r = data_tools.indicators("2330", "2026-05-15", preset="bogus")
        assert r["selection"]["value"] == "default"


# ════════════════════════════════════════════════════════════
# payload size + public surface
# ════════════════════════════════════════════════════════════

class TestPayloadAndSurface:

    def test_market_overview_payload_bounded(self):
        with patch("fusion.market_dashboard.market_dashboard", return_value=_DASHBOARD_OK), \
             patch("fusion.market_events.market_events", return_value=_EVENTS_OK):
            r = data_tools.market_overview("2026-05-15")
        assert len(json.dumps(r, ensure_ascii=False)) < 15_000

    def test_three_consolidated_tools_callable(self):
        for name in ("market_overview", "stock_levels", "indicators"):
            assert hasattr(data_tools, name) and callable(getattr(data_tools, name))

    def test_old_fusion_functions_still_callable(self):
        """被整併的 10 個舊 fusion function 仍留 data.py(dashboard / direct python 用)。"""
        for name in (
            "market_events", "market_dashboard", "key_levels", "stop_loss_calc",
            "pattern_scan", "indicator_momentum", "indicator_volatility",
            "indicator_volume", "indicator_pattern", "indicator_stack",
        ):
            assert hasattr(data_tools, name) and callable(getattr(data_tools, name))
