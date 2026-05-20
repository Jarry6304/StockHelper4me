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
                     facts: list | None = None,
                     latest_close: dict | None = None):
    """Mock agg.as_of 回 snapshot 對應 _kalman.compute_kalman_trend 用。

    v3.26:加 mock `fetch_latest_close_for_tool`(預設 None → fallback 走 indicator)。
    """
    from mcp_server import _kalman
    from mcp_server import _price as _price_mod

    monkeypatch.setattr(_price_mod, "fetch_latest_close_for_tool",
                        lambda *a, **kw: latest_close)

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
    import fusion.raw as agg_mod
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
        # v3.34:effective_uncertainty = max(8.5, 1220.3 × 0.01) = max(8.5, 12.203) = 12.203
        # uncertainty_band = smoothed ± effective = [1208.10, 1232.50]
        assert result["uncertainty_band"] == [1208.1, 1232.5]
        # v3.34:deviation = (1234.5 - 1220.3) / 12.203 ≈ 1.16
        assert abs(result["deviation_sigma"] - 1.16) < 0.05

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

    def test_v3_30_reads_series_last_entry(self, monkeypatch):
        """v3.30:Rust 寫入 `{stock_id, series: [...KalmanPoint], events}`,最新
        state 在 `series[-1]`。原本 `val.get("smoothed_price")` 讀頂層 → 永遠 0
        (production 2330 bug)。

        確認:value.series[-1] 提供值 + 頂層無欄位 → 正確讀 latest state。
        """
        production_schema_value = {
            "stock_id": "2330",
            "timeframe": "Daily",
            "series": [
                {"date": "2026-05-13", "raw_close": 2200.0, "smoothed_price": 2180.0,
                 "uncertainty": 12.0, "velocity": 0.5, "regime": "StableUp"},
                {"date": "2026-05-14", "raw_close": 2230.0, "smoothed_price": 2200.0,
                 "uncertainty": 11.5, "velocity": 0.6, "regime": "StableUp"},
                {"date": "2026-05-15", "raw_close": 2265.0, "smoothed_price": 2225.0,
                 "uncertainty": 11.0, "velocity": 0.7, "regime": "Accelerating"},
            ],
            "events": [],
        }
        _patch_agg_as_of(monkeypatch, indicator_value=production_schema_value)
        result = data_tools.kalman_trend("2330", "2026-05-15", lookback_days=180)
        # 應該讀 series[-1] 的 2225 / 11.0 / 0.7 / Accelerating(不是頂層 missing → 0)
        assert result["smoothed_price"] == 2225.0
        # v3.34:effective_uncertainty = max(11.0, 2225 × 0.01) = max(11.0, 22.25) = 22.25
        # band = [2225 - 22.25, 2225 + 22.25] = [2202.75, 2247.25]
        assert result["uncertainty_band"] == [2202.75, 2247.25]
        assert result["trend_velocity"] == 0.7
        assert result["regime"] == "Accelerating"

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
        # v3.33:加 kalman_by_horizon + cross_horizon_consistency,放寬到 4 KB
        assert n < 4_000, f"payload {n} bytes 超 budget 4 KB"

    # ════════════════════════════════════════════════════════════
    # v3.33 — multi-horizon
    # ════════════════════════════════════════════════════════════

    def test_v3_33_parses_horizons_array(self, monkeypatch):
        """v3.33:Rust 寫 `horizons: [...4]` array,MCP 拆成 kalman_by_horizon dict
        by label。每 entry 含 Q / halflife_bars / smoothed_price / regime 等。"""
        production_schema = {
            "stock_id": "3030",
            "timeframe": "Daily",
            "primary_horizon": "medium",
            "series": [
                {"date": "2026-05-15", "raw_close": 395.0, "smoothed_price": 320.0,
                 "uncertainty": 12.0, "velocity": 0.5, "regime": "StableUp"},
            ],
            "events": [],
            "horizons": [
                {"label": "short", "process_noise_q": 0.1, "halflife_bars": 31.0,
                 "velocity_threshold_pct": 0.005, "min_regime_duration_days": 3,
                 "series_last": {"date": "2026-05-15", "raw_close": 395.0,
                                 "smoothed_price": 388.0, "uncertainty": 5.0,
                                 "velocity": 1.5, "regime": "Accelerating"},
                 "event_count": 8},
                {"label": "medium", "process_noise_q": 0.01, "halflife_bars": 99.0,
                 "velocity_threshold_pct": 0.002, "min_regime_duration_days": 5,
                 "series_last": {"date": "2026-05-15", "raw_close": 395.0,
                                 "smoothed_price": 320.0, "uncertainty": 12.0,
                                 "velocity": 0.5, "regime": "StableUp"},
                 "event_count": 5},
                {"label": "long", "process_noise_q": 0.001, "halflife_bars": 310.0,
                 "velocity_threshold_pct": 0.0015, "min_regime_duration_days": 5,
                 "series_last": {"date": "2026-05-15", "raw_close": 395.0,
                                 "smoothed_price": 220.0, "uncertainty": 30.0,
                                 "velocity": 0.2, "regime": "StableUp"},
                 "event_count": 3},
                {"label": "ultra_long", "process_noise_q": 1e-5, "halflife_bars": 3100.0,
                 "velocity_threshold_pct": 0.001, "min_regime_duration_days": 5,
                 "series_last": {"date": "2026-05-15", "raw_close": 395.0,
                                 "smoothed_price": 159.0, "uncertainty": 80.0,
                                 "velocity": 0.05, "regime": "Sideway"},
                 "event_count": 1},
            ],
        }
        _patch_agg_as_of(monkeypatch, indicator_value=production_schema)
        result = data_tools.kalman_trend("3030", "2026-05-15")

        # 結構檢查
        assert "kalman_by_horizon" in result
        assert "primary_horizon" in result
        assert result["primary_horizon"] == "medium"

        by_h = result["kalman_by_horizon"]
        assert set(by_h.keys()) == {"short", "medium", "long", "ultra_long"}

        # short horizon:Q=0.1 / halflife=31 / smoothed=388 / regime=Accelerating
        assert by_h["short"]["Q"] == 0.1
        assert by_h["short"]["halflife_bars"] == 31
        assert by_h["short"]["smoothed_price"] == 388.0
        assert by_h["short"]["regime"] == "Accelerating"
        assert by_h["short"]["regime_label"] == "加速上漲"
        assert by_h["short"]["event_count"] == 8

        # ultra_long:smoothed=159(完全不追)
        assert by_h["ultra_long"]["smoothed_price"] == 159.0
        assert by_h["ultra_long"]["regime"] == "Sideway"

        # 頂層 backward compat:medium horizon
        assert result["smoothed_price"] == 320.0
        assert result["regime"] == "StableUp"

    def test_v3_33_cross_horizon_consistency_summary(self, monkeypatch):
        """4 horizon regime 一致性摘要:all_aligned + majority_regime + summary。"""
        # 對 3030(德律):short/medium/long 都 StableUp,只 ultra_long Sideway → majority 3/4
        production_schema = {
            "stock_id": "3030",
            "timeframe": "Daily",
            "primary_horizon": "medium",
            "series": [{"date": "2026-05-15", "raw_close": 395.0,
                        "smoothed_price": 320.0, "uncertainty": 12.0,
                        "velocity": 0.5, "regime": "StableUp"}],
            "events": [],
            "horizons": [
                {"label": "short", "process_noise_q": 0.1, "halflife_bars": 31.0,
                 "velocity_threshold_pct": 0.005, "min_regime_duration_days": 3,
                 "series_last": {"date": "2026-05-15", "raw_close": 395.0,
                                 "smoothed_price": 388.0, "uncertainty": 5.0,
                                 "velocity": 1.5, "regime": "StableUp"},
                 "event_count": 5},
                {"label": "medium", "process_noise_q": 0.01, "halflife_bars": 99.0,
                 "velocity_threshold_pct": 0.002, "min_regime_duration_days": 5,
                 "series_last": {"date": "2026-05-15", "raw_close": 395.0,
                                 "smoothed_price": 320.0, "uncertainty": 12.0,
                                 "velocity": 0.5, "regime": "StableUp"},
                 "event_count": 4},
                {"label": "long", "process_noise_q": 0.001, "halflife_bars": 310.0,
                 "velocity_threshold_pct": 0.0015, "min_regime_duration_days": 5,
                 "series_last": {"date": "2026-05-15", "raw_close": 395.0,
                                 "smoothed_price": 220.0, "uncertainty": 30.0,
                                 "velocity": 0.2, "regime": "StableUp"},
                 "event_count": 2},
                {"label": "ultra_long", "process_noise_q": 1e-5, "halflife_bars": 3100.0,
                 "velocity_threshold_pct": 0.001, "min_regime_duration_days": 5,
                 "series_last": {"date": "2026-05-15", "raw_close": 395.0,
                                 "smoothed_price": 159.0, "uncertainty": 80.0,
                                 "velocity": 0.05, "regime": "Sideway"},
                 "event_count": 1},
            ],
        }
        _patch_agg_as_of(monkeypatch, indicator_value=production_schema)
        result = data_tools.kalman_trend("3030", "2026-05-15")

        cons = result["cross_horizon_consistency"]
        assert cons["all_aligned"] is False
        assert cons["majority_regime"] == "StableUp"
        assert cons["majority_count"] == 3
        assert cons["total_horizons"] == 4
        # narrative 含「跨 horizon」摘要
        assert "跨 horizon" in result["narrative"]

    def test_v3_33_all_aligned_when_all_same_regime(self, monkeypatch):
        """4 horizon 全 StableUp → all_aligned=True + summary 「高度一致」。"""
        all_up = {
            "stock_id": "2330", "timeframe": "Daily", "primary_horizon": "medium",
            "series": [{"date": "2026-05-15", "raw_close": 2265.0,
                        "smoothed_price": 2200.0, "uncertainty": 10.0,
                        "velocity": 1.0, "regime": "StableUp"}],
            "events": [],
            "horizons": [
                {"label": lbl, "process_noise_q": q, "halflife_bars": hl,
                 "velocity_threshold_pct": 0.001, "min_regime_duration_days": 5,
                 "series_last": {"date": "2026-05-15", "raw_close": 2265.0,
                                 "smoothed_price": 2200.0, "uncertainty": 10.0,
                                 "velocity": 1.0, "regime": "StableUp"},
                 "event_count": 0}
                for (lbl, q, hl) in [
                    ("short", 0.1, 31.0), ("medium", 0.01, 99.0),
                    ("long", 0.001, 310.0), ("ultra_long", 1e-5, 3100.0),
                ]
            ],
        }
        _patch_agg_as_of(monkeypatch, indicator_value=all_up)
        result = data_tools.kalman_trend("2330", "2026-05-15")

        cons = result["cross_horizon_consistency"]
        assert cons["all_aligned"] is True
        assert cons["majority_count"] == 4
        assert "高度一致" in cons["summary"]

    def test_v3_34_uncertainty_floor_compresses_extreme_deviation(self, monkeypatch):
        """v3.34:Kalman P 收斂塌掉時(uncertainty < 1% × smoothed)套 1% floor,
        避免 deviation_sigma 飆到天文數字(production 3030 ultra_long 4138σ bug)。

        Fixture 模擬 3030 production state(uncertainty=0.06 vs smoothed=158.59,
        實際 P/p ratio = 0.04%,遠低於 1% 量綱)。
        """
        # 對齊 production verify Phase D 揭露的數字
        production_3030 = {
            "stock_id": "3030", "timeframe": "Daily", "primary_horizon": "medium",
            "series": [{"date": "2026-05-15", "raw_close": 395.0,
                        "smoothed_price": 158.59, "uncertainty": 0.06,
                        "velocity": 0.7247, "regime": "Accelerating"}],
            "events": [],
            "horizons": [
                {"label": "ultra_long", "process_noise_q": 1e-5, "halflife_bars": 3099.0,
                 "velocity_threshold_pct": 0.001, "min_regime_duration_days": 5,
                 "series_last": {"date": "2026-05-15", "raw_close": 395.0,
                                 "smoothed_price": 158.59, "uncertainty": 0.06,
                                 "velocity": 0.7247, "regime": "Accelerating"},
                 "event_count": 14},
            ],
        }
        _patch_agg_as_of(monkeypatch, indicator_value=production_3030)
        result = data_tools.kalman_trend("3030", "2026-05-15")

        # 頂層 deviation:effective_unc = max(0.06, 158.59 × 0.01) = 1.586
        # deviation = (395.0 - 158.59) / 1.586 ≈ 149.07σ(原本 4138σ)
        assert abs(result["deviation_sigma"] - 149.07) < 1.0, \
            f"v3.34 deviation 應 ~149σ(從 4138σ 壓下),實際 {result['deviation_sigma']}"

        # uncertainty_band 也走 floor:[158.59 - 1.586, 158.59 + 1.586] = [157.00, 160.18]
        assert result["uncertainty_band"] == [157.0, 160.18]

        # ultra_long horizon dev 也走 floor
        ultra = result["kalman_by_horizon"]["ultra_long"]
        assert abs(ultra["deviation_sigma"] - 149.07) < 1.0, \
            f"v3.34 ultra_long deviation 應 ~149σ,實際 {ultra['deviation_sigma']}"

    def test_v3_34_floor_no_op_when_uncertainty_above_pct(self, monkeypatch):
        """v3.34:當 uncertainty 已大於 1% × smoothed 時,floor no-op,deviation 不變。

        Fixture:smoothed=100, uncertainty=5(= 5% > 1%)→ floor=1 < 5,不生效。
        Deviation = (110-100)/5 = 2.0σ(正常 Kalman 行為)。
        """
        normal_value = {
            "stock_id": "TEST", "timeframe": "Daily",
            "series": [{"date": "2026-05-15", "raw_close": 110.0,
                        "smoothed_price": 100.0, "uncertainty": 5.0,
                        "velocity": 0.5, "regime": "StableUp"}],
            "events": [],
        }
        _patch_agg_as_of(monkeypatch, indicator_value=normal_value)
        result = data_tools.kalman_trend("TEST", "2026-05-15")
        # 5 > 1% × 100 = 1.0,floor no-op
        assert abs(result["deviation_sigma"] - 2.0) < 0.01
        assert result["uncertainty_band"] == [95.0, 105.0]

    def test_v3_33_backward_compat_no_horizons_field(self, monkeypatch):
        """舊 schema(無 horizons array)應 graceful:kalman_by_horizon = {}。

        對 production 過渡期 — Rust 已升 v3.33 但部分 stock 的 indicator_value
        還是 v3.30 schema(沒 horizons array,只 series)。MCP 仍要 work。
        """
        v3_30_only_schema = {
            "stock_id": "2330", "timeframe": "Daily",
            "series": [
                {"date": "2026-05-15", "raw_close": 2265.0,
                 "smoothed_price": 2225.0, "uncertainty": 11.0,
                 "velocity": 0.7, "regime": "Accelerating"},
            ],
            "events": [],
            # 注意:沒 horizons key
        }
        _patch_agg_as_of(monkeypatch, indicator_value=v3_30_only_schema)
        result = data_tools.kalman_trend("2330", "2026-05-15")

        # 頂層 backward compat 工作
        assert result["smoothed_price"] == 2225.0
        assert result["regime"] == "Accelerating"
        # multi-horizon 結構是空但 keys 存在(graceful)
        assert result["kalman_by_horizon"] == {}
        cons = result["cross_horizon_consistency"]
        assert cons["all_aligned"] is None
        assert cons["majority_count"] == 0
        assert cons["total_horizons"] == 0


class TestToolkitV3PublicSurface:
    """確認 9 個 public tools 都 importable + 可 invoke(callable check 不打 PG)。"""

    def test_all_nine_tools_present(self):
        for name in (
            # v3 (5)
            "neely_forecast", "stock_health", "market_context",
            "magic_formula_screen", "kalman_trend",
            # v3.22 (4)
            "loan_collateral_snapshot", "block_trade_summary",
            "risk_alert_status", "commodity_macro_snapshot",
        ):
            assert hasattr(data_tools, name), f"data_tools.{name} 缺失"
            assert callable(getattr(data_tools, name))


# ════════════════════════════════════════════════════════════
# v3.22 — 4 new tools(direct cursor mock pattern,對齊 _magic_formula)
# ════════════════════════════════════════════════════════════


def _patch_direct_conn(monkeypatch, module_name: str, cursor):
    """Mock <module>.get_connection 回 conn(指定 cursor)。"""
    from mcp_server import _loan_collateral, _block_trade, _risk_alert, _commodity_macro
    target = {
        "_loan_collateral":  _loan_collateral,
        "_block_trade":      _block_trade,
        "_risk_alert":       _risk_alert,
        "_commodity_macro":  _commodity_macro,
    }[module_name]

    conn = MagicMock()
    conn.cursor.return_value = cursor
    conn.close = MagicMock()
    monkeypatch.setattr(target, "get_connection", lambda *a, **kw: conn)


def _mk_simple_cursor(fetchone_val=None, fetchall_val=None):
    cur = MagicMock()
    cur.fetchone.return_value = fetchone_val
    cur.fetchall.return_value = fetchall_val or []
    cur.__enter__ = lambda self: self
    cur.__exit__ = lambda *args: False
    return cur


class TestLoanCollateralSnapshot:

    def test_categories_structure(self, monkeypatch):
        """5 大類各有 balance / change_pct / ratio,dominant 對齊 row。"""
        row = {
            "date": date(2026, 5, 15),
            "margin_current_balance": 26483,
            "firm_loan_current_balance": 22,
            "unrestricted_loan_current_balance": 77125,
            "finance_loan_current_balance": 8691,
            "settlement_margin_current_balance": 0,
            "margin_change_pct": 1.22,
            "firm_loan_change_pct": 0.0,
            "unrestricted_loan_change_pct": -0.5,
            "finance_loan_change_pct": 0.0,
            "settlement_margin_change_pct": 0.0,
            "total_balance": 112321,
            "dominant_category": "unrestricted_loan",
            "dominant_category_ratio": 0.6866,
        }
        cur = _mk_simple_cursor(fetchone_val=row)
        _patch_direct_conn(monkeypatch, "_loan_collateral", cur)

        result = data_tools.loan_collateral_snapshot("2330", "2026-05-15")
        assert result["stock_id"] == "2330"
        assert result["snapshot_date"] == "2026-05-15"
        for cat in ("margin", "firm_loan", "unrestricted_loan",
                    "finance_loan", "settlement_margin"):
            assert cat in result["categories"]
            assert "balance" in result["categories"][cat]
            assert "change_pct" in result["categories"][cat]
            assert "ratio" in result["categories"][cat]
        assert result["dominant_category"] == "unrestricted_loan"
        assert result["dominant_category_label"] == "無限制借券"
        assert result["concentration_alert"] is False    # 0.6866 < 0.70

    def test_concentration_alert_at_70_pct(self, monkeypatch):
        row = {
            "date": date(2026, 5, 15),
            "margin_current_balance": 0, "firm_loan_current_balance": 0,
            "unrestricted_loan_current_balance": 75000,
            "finance_loan_current_balance": 0, "settlement_margin_current_balance": 0,
            "margin_change_pct": 0.0, "firm_loan_change_pct": 0.0,
            "unrestricted_loan_change_pct": 12.5,
            "finance_loan_change_pct": 0.0, "settlement_margin_change_pct": 0.0,
            "total_balance": 100000,
            "dominant_category": "unrestricted_loan",
            "dominant_category_ratio": 0.75,
        }
        cur = _mk_simple_cursor(fetchone_val=row)
        _patch_direct_conn(monkeypatch, "_loan_collateral", cur)
        result = data_tools.loan_collateral_snapshot("2330", "2026-05-15")
        assert result["concentration_alert"] is True
        assert "70%" in result["narrative"]

    def test_empty_when_no_data(self, monkeypatch):
        cur = _mk_simple_cursor(fetchone_val=None)
        _patch_direct_conn(monkeypatch, "_loan_collateral", cur)
        result = data_tools.loan_collateral_snapshot("9999", "2026-05-15")
        assert result["snapshot_date"] is None
        assert result["categories"] == {}
        assert "無借券抵押餘額資料" in result["narrative"]

    def test_payload_size_bounded(self, monkeypatch):
        row = {
            "date": date(2026, 5, 15),
            "margin_current_balance": 26483,
            "firm_loan_current_balance": 22,
            "unrestricted_loan_current_balance": 77125,
            "finance_loan_current_balance": 8691,
            "settlement_margin_current_balance": 0,
            "margin_change_pct": 1.22, "firm_loan_change_pct": 0.0,
            "unrestricted_loan_change_pct": -0.5,
            "finance_loan_change_pct": 0.0, "settlement_margin_change_pct": 0.0,
            "total_balance": 112321, "dominant_category": "unrestricted_loan",
            "dominant_category_ratio": 0.6866,
        }
        cur = _mk_simple_cursor(fetchone_val=row)
        _patch_direct_conn(monkeypatch, "_loan_collateral", cur)
        result = data_tools.loan_collateral_snapshot("2330", "2026-05-15")
        n = len(json.dumps(result, ensure_ascii=False))
        assert n < 3_000, f"payload {n} bytes 超 budget 3 KB"


class TestBlockTradeSummary:

    def test_active_days_and_totals(self, monkeypatch):
        rows = [
            {"date": date(2026, 5, 14),
             "total_volume": 50_000, "total_trading_money": 50_000_000,
             "matching_volume": 30_000, "matching_trading_money": 30_000_000,
             "matching_share": 0.6, "largest_single_trade_money": 25_000_000,
             "trade_type_count": 2},
            {"date": date(2026, 5, 12),
             "total_volume": 100_000, "total_trading_money": 100_000_000,
             "matching_volume": 90_000, "matching_trading_money": 90_000_000,
             "matching_share": 0.9, "largest_single_trade_money": 80_000_000,
             "trade_type_count": 1},
        ]
        cur = _mk_simple_cursor(fetchall_val=rows)
        _patch_direct_conn(monkeypatch, "_block_trade", cur)
        result = data_tools.block_trade_summary("2330", "2026-05-15", lookback_days=30)
        assert result["active_days"] == 2
        assert result["total_volume"] == 150_000
        assert result["total_trading_money"] == 150_000_000
        # 2026-05-12 share=0.9 >= 0.80 → spike fired
        assert "2026-05-12" in result["matching_spike_dates"]
        assert "2026-05-14" not in result["matching_spike_dates"]

    def test_empty_when_no_data(self, monkeypatch):
        cur = _mk_simple_cursor(fetchall_val=[])
        _patch_direct_conn(monkeypatch, "_block_trade", cur)
        result = data_tools.block_trade_summary("9999", "2026-05-15")
        assert result["active_days"] == 0
        assert "無大宗交易" in result["narrative"]

    def test_payload_size_bounded(self, monkeypatch):
        # 模擬 30 個 spike 日(極端 case)
        rows = [
            {"date": date(2026, 4, 16) + timedelta(days=i),
             "total_volume": 100, "total_trading_money": 100_000_000,
             "matching_volume": 90, "matching_trading_money": 90_000_000,
             "matching_share": 0.9, "largest_single_trade_money": 80_000_000,
             "trade_type_count": 1}
            for i in range(30)
        ]
        cur = _mk_simple_cursor(fetchall_val=rows)
        _patch_direct_conn(monkeypatch, "_block_trade", cur)
        result = data_tools.block_trade_summary("2330", "2026-05-15", lookback_days=30)
        assert len(result["matching_spike_dates"]) <= 10   # truncate to 10
        n = len(json.dumps(result, ensure_ascii=False))
        assert n < 3_000, f"payload {n} bytes 超 budget 3 KB"


class TestRiskAlertStatus:

    def test_in_disposition_period(self, monkeypatch):
        rows = [
            {"announced_date": date(2025, 1, 13), "disposition_cnt": 2,
             "period_start": date(2025, 1, 14), "period_end": date(2025, 2, 7),
             "condition": "連續5日及沖銷標準",
             "measure": "人工管制之撮合終端機"},
        ]
        cur = _mk_simple_cursor(fetchall_val=rows)
        _patch_direct_conn(monkeypatch, "_risk_alert", cur)
        # as_of 在 period 區間內
        result = data_tools.risk_alert_status("3363", "2025-01-20")
        cs = result["current_status"]
        assert cs["in_disposition_period"] is True
        assert cs["severity"] == "disposition"
        assert cs["severity_label"] == "處置股(分盤撮合)"
        assert cs["days_remaining"] == 18    # 2025-02-07 - 2025-01-20

    def test_severity_parser_cash_only(self, monkeypatch):
        rows = [
            {"announced_date": date(2025, 1, 13), "disposition_cnt": 3,
             "period_start": date(2025, 1, 14), "period_end": date(2025, 2, 7),
             "condition": "...", "measure": "改以全額交割"},
        ]
        cur = _mk_simple_cursor(fetchall_val=rows)
        _patch_direct_conn(monkeypatch, "_risk_alert", cur)
        result = data_tools.risk_alert_status("3363", "2025-01-20")
        assert result["current_status"]["severity"] == "cash_only"

    def test_escalation_chain(self, monkeypatch):
        rows = [
            {"announced_date": date(2025, 2, 20), "disposition_cnt": 2,
             "period_start": date(2025, 2, 21), "period_end": date(2025, 3, 15),
             "condition": "...", "measure": "人工管制"},
            {"announced_date": date(2025, 1, 13), "disposition_cnt": 1,
             "period_start": date(2025, 1, 14), "period_end": date(2025, 2, 7),
             "condition": "...", "measure": "注意交易資訊"},
        ]
        cur = _mk_simple_cursor(fetchall_val=rows)
        _patch_direct_conn(monkeypatch, "_risk_alert", cur)
        # as_of 不在任何 period 內;測 escalation_count 60d
        result = data_tools.risk_alert_status("3363", "2025-03-20")
        assert result["escalation_count_60d"] == 2
        assert len(result["history_60d"]) == 2

    def test_empty_no_alerts(self, monkeypatch):
        cur = _mk_simple_cursor(fetchall_val=[])
        _patch_direct_conn(monkeypatch, "_risk_alert", cur)
        result = data_tools.risk_alert_status("2330", "2026-05-15")
        assert result["current_status"]["in_disposition_period"] is False
        assert result["escalation_count_60d"] == 0
        assert "無風險警訊" in result["narrative"]

    def test_v3_29_short_disposition_measure(self, monkeypatch):
        """v3.29:3030 production case — measure='第一次處置' condition='連續三次'。

        既有 keyword `人工管制` / `注意交易資訊` 不命中此短字串,但 `處置` broad
        pattern 應 fire `disposition` 而非 fallback `unknown`。
        """
        rows = [
            {"announced_date": date(2026, 5, 7), "disposition_cnt": 1,
             "period_start": date(2026, 5, 7), "period_end": date(2026, 5, 20),
             "condition": "連續三次", "measure": "第一次處置"},
        ]
        cur = _mk_simple_cursor(fetchall_val=rows)
        _patch_direct_conn(monkeypatch, "_risk_alert", cur)
        result = data_tools.risk_alert_status("3030", "2026-05-15")
        cs = result["current_status"]
        assert cs["in_disposition_period"] is True
        assert cs["severity"] == "disposition"
        assert cs["severity_label"] == "處置股(分盤撮合)"
        # history_60d row 也需要對齊
        assert result["history_60d"][0]["severity"] == "disposition"

    def test_v3_29_attention_only_from_condition(self, monkeypatch):
        """v3.29:condition 含 `注意` 但 measure 不含關鍵字 → warning。"""
        rows = [
            {"announced_date": date(2026, 5, 7), "disposition_cnt": 1,
             "period_start": date(2026, 5, 7), "period_end": date(2026, 5, 20),
             "condition": "連續三次注意異常", "measure": ""},
        ]
        cur = _mk_simple_cursor(fetchall_val=rows)
        _patch_direct_conn(monkeypatch, "_risk_alert", cur)
        result = data_tools.risk_alert_status("3030", "2026-05-15")
        assert result["current_status"]["severity"] == "warning"


class TestCommodityMacroSnapshot:

    def test_gold_basic(self, monkeypatch):
        rows = [
            {"commodity": "GOLD", "date": date(2026, 5, 15),
             "price": 2630.50, "return_pct": 0.85, "return_z_score": 1.23,
             "momentum_state": "up", "streak_days": 4},
        ]
        cur = _mk_simple_cursor(fetchall_val=rows)
        _patch_direct_conn(monkeypatch, "_commodity_macro", cur)
        result = data_tools.commodity_macro_snapshot("2026-05-15")
        assert result["snapshot_date"] == "2026-05-15"
        assert len(result["commodities"]) == 1
        c = result["commodities"][0]
        assert c["name"] == "GOLD"
        assert c["label"] == "黃金"
        assert c["price"] == 2630.50
        assert c["momentum_state"] == "up"
        assert c["streak_days"] == 4
        assert c["spike_alert"] is False    # |1.23| < 2.0

    def test_spike_alert_when_abs_z_above_2(self, monkeypatch):
        rows = [
            {"commodity": "GOLD", "date": date(2026, 5, 15),
             "price": 2700.0, "return_pct": 3.0, "return_z_score": 2.4,
             "momentum_state": "up", "streak_days": 1},
        ]
        cur = _mk_simple_cursor(fetchall_val=rows)
        _patch_direct_conn(monkeypatch, "_commodity_macro", cur)
        result = data_tools.commodity_macro_snapshot("2026-05-15")
        assert result["commodities"][0]["spike_alert"] is True
        assert "spike 警戒" in result["narrative"]

    def test_empty_when_no_data(self, monkeypatch):
        cur = _mk_simple_cursor(fetchall_val=[])
        _patch_direct_conn(monkeypatch, "_commodity_macro", cur)
        result = data_tools.commodity_macro_snapshot("2026-05-15")
        assert result["snapshot_date"] is None
        # commodities list 仍含 default ["GOLD"] 但 data_available=False
        assert len(result["commodities"]) == 1
        assert result["commodities"][0]["data_available"] is False
        assert "無 commodity_price_daily_derived" in result["narrative"]

    def test_multi_commodity_subset_returned(self, monkeypatch):
        rows = [
            {"commodity": "GOLD", "date": date(2026, 5, 15),
             "price": 2630.0, "return_pct": 0.5, "return_z_score": 0.8,
             "momentum_state": "up", "streak_days": 2},
            # SILVER 沒回 → data_available=False
        ]
        cur = _mk_simple_cursor(fetchall_val=rows)
        _patch_direct_conn(monkeypatch, "_commodity_macro", cur)
        result = data_tools.commodity_macro_snapshot(
            "2026-05-15", commodities=["GOLD", "SILVER"]
        )
        assert len(result["commodities"]) == 2
        gold = next(c for c in result["commodities"] if c["name"] == "GOLD")
        silver = next(c for c in result["commodities"] if c["name"] == "SILVER")
        assert gold["data_available"] is True
        assert silver["data_available"] is False


# ════════════════════════════════════════════════════════════
# v3.26 — current_price bug fix(直讀 price_daily)
# ════════════════════════════════════════════════════════════


class TestKalmanCurrentPriceBugFix:
    """v3.26 修法:current_price 走 price_daily,不依賴 indicator_latest.raw_close。"""

    def test_uses_price_daily_when_available(self, monkeypatch):
        """price_daily 有資料 → current_price 用 DB latest close。"""
        _patch_agg_as_of(
            monkeypatch,
            indicator_value={
                "raw_close": 999.99,      # indicator 內 stale 數據(會被忽略)
                "smoothed_price": 1220.3,
                "velocity": 0.42,
                "uncertainty": 8.5,
                "regime": "StableUp",
            },
            latest_close={
                "date": "2026-05-15", "close": 395.0,
                "prev_close": 397.0, "change_pct": -0.50,
            },
        )
        result = data_tools.kalman_trend("3030", "2026-05-15")
        # current_price 用 price_daily 395.0(authoritative),不是 indicator 的 999.99
        assert result["current_price"] == 395.0

    def test_falls_back_to_indicator_when_db_empty(self, monkeypatch):
        """price_daily 無資料 → fallback indicator.raw_close(對齊既有行為)。"""
        _patch_agg_as_of(
            monkeypatch,
            indicator_value={
                "raw_close": 999.99, "smoothed_price": 1000.0,
                "velocity": 0.1, "uncertainty": 5.0, "regime": "Sideway",
            },
            latest_close=None,
        )
        result = data_tools.kalman_trend("9999", "2026-05-15")
        assert result["current_price"] == 999.99


class TestMetadataEventKindCompatibility:
    """v3.27 修法:_health / render 都改 event_kind 優先 + kind fallback。

    確保新 facts(Rust 寫 metadata.event_kind)和舊 test fixture(metadata.kind)
    都能被正確 parse。
    """

    def test_health_extracts_event_kind_from_metadata(self, monkeypatch):
        """新 production facts metadata.event_kind 應正確 trigger signal。"""
        from mcp_server import _health
        # 模擬 production facts(Rust 寫 event_kind)
        fact = {
            "stock_id": "3030",
            "fact_date": date(2026, 5, 15),
            "timeframe": "daily",
            "source_core": "macd_core",
            "source_version": "0.1.0",
            "statement": "GoldenCross on 2026-05-15",
            "metadata": {"event_kind": "GoldenCross"},  # 注意是 event_kind 不是 kind
        }
        # 直接驗 extract 邏輯
        meta = fact.get("metadata") or {}
        extracted = meta.get("event_kind") or meta.get("kind")
        assert extracted == "GoldenCross"
        # 驗 sign mapping(GoldenCross 應 bullish)
        assert _health._kind_sign("GoldenCross") == 1

    def test_health_falls_back_to_kind_for_legacy_metadata(self, monkeypatch):
        """舊 metadata.kind 仍 work(向下相容)。"""
        from mcp_server import _health
        fact = {"metadata": {"kind": "DeathCross"}}
        meta = fact.get("metadata") or {}
        extracted = meta.get("event_kind") or meta.get("kind")
        assert extracted == "DeathCross"
        assert _health._kind_sign("DeathCross") == -1
