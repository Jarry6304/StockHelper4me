"""v4.32 Golden L3:MCP 工具改讀物化 row(present → 直回;miss → compute fallback)。"""

from unittest.mock import MagicMock

import mcp_server.tools.data as data_tools


def _patch_materialized(monkeypatch, snapshot):
    """fetch_fusion_doc 回 {snapshot} 或 None;get_connection 給 MagicMock。"""
    monkeypatch.setattr("fusion.raw._db.get_connection", lambda *a, **k: MagicMock())
    row = {"snapshot": snapshot} if snapshot is not None else None
    monkeypatch.setattr("fusion.materialize.read.fetch_fusion_doc", lambda *a, **k: row)


# ── market_context ──────────────────────────────────────────────────────────
def test_market_context_serves_materialized(monkeypatch):
    climate = {
        "as_of": "2026-05-13", "overall_climate": "bullish", "climate_score": 12.3,
        "components": {}, "systemic_risks": [], "narrative": "x",
    }
    _patch_materialized(monkeypatch, climate)
    out = data_tools.market_context("2026-05-13")
    assert out["overall_climate"] == "bullish" and out["climate_score"] == 12.3


def test_market_context_falls_back_to_compute_on_miss(monkeypatch):
    _patch_materialized(monkeypatch, None)
    called = {}

    def _fake(as_of, lookback_days=60, **k):
        called["yes"] = True
        return {"overall_climate": "computed"}

    monkeypatch.setattr("mcp_server._climate.compute_market_context", _fake)
    out = data_tools.market_context("2026-05-13")
    assert called.get("yes") and out["overall_climate"] == "computed"


def test_market_context_non_default_lookback_skips_materialized(monkeypatch):
    # 非預設 lookback → 不讀物化,直接 compute
    _patch_materialized(monkeypatch, {"overall_climate": "should_not_be_used"})
    called = {}

    def _fake(as_of, lookback_days=60, **k):
        called["lb"] = lookback_days
        return {"overall_climate": "computed"}

    monkeypatch.setattr("mcp_server._climate.compute_market_context", _fake)
    out = data_tools.market_context("2026-05-13", lookback_days=90)
    assert called.get("lb") == 90 and out["overall_climate"] == "computed"


# ── stock_levels(只 levels 區段改讀物化)──────────────────────────────────
def test_stock_levels_serves_materialized_levels(monkeypatch):
    levels_doc = {"stock_id": "2330", "levels": [{"price": 100.0}], "level_count": 1}
    _patch_materialized(monkeypatch, levels_doc)
    monkeypatch.setattr("fusion.pattern_scan.pattern_scan", lambda *a, **k: {"patterns": []})
    out = data_tools.stock_levels("2330", "2026-05-15")
    assert out["key_levels"] == levels_doc
    assert out["stop_loss"] is None  # 未給 entry_price


def test_stock_levels_falls_back_to_compute_on_miss(monkeypatch):
    _patch_materialized(monkeypatch, None)
    monkeypatch.setattr("fusion.key_levels.key_levels",
                        lambda *a, **k: {"computed": True, "levels": []})
    monkeypatch.setattr("fusion.pattern_scan.pattern_scan", lambda *a, **k: {"patterns": []})
    out = data_tools.stock_levels("2330", "2026-05-15")
    assert out["key_levels"] == {"computed": True, "levels": []}


# ── dual_track_resonance ─────────────────────────────────────────────────────
def test_dual_track_serves_materialized(monkeypatch):
    reso = {"stock_id": "2330", "as_of": "2024-06-01", "track1": {}, "track2": {},
            "findings": [], "single_track_mode": False}
    _patch_materialized(monkeypatch, reso)
    out = data_tools.dual_track_resonance("2330", "2024-06-01")
    assert out == reso


def test_dual_track_non_default_params_skips_materialized(monkeypatch):
    # 非預設 horizon → 不讀物化(compute path,既有 30s 安全網)
    _patch_materialized(monkeypatch, {"should_not": "be_used"})
    from unittest.mock import patch
    with patch("fusion.dual_track.resonance.read_track1", return_value=MagicMock()), \
         patch("fusion.dual_track.resonance.read_track2", return_value=MagicMock()), \
         patch("fusion.dual_track.resonance.fetch_is_top_30", return_value=(False, None)), \
         patch("fusion.dual_track.resonance.fetch_latest_close", return_value={"close": 100.0}):
        # 走 compute path(回 DualTrackResult,非我們的物化 dict)
        out = data_tools.dual_track_resonance("2330", "2024-06-01", primary_horizon=21)
    assert out.get("should_not") != "be_used"  # 沒回物化值
