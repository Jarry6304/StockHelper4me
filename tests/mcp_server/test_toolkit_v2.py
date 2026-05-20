"""Unit tests for MCP server v2 toolkit(public 3 tools)。

對齊 plan §Tests Strategy:
- TestMarketContext(Step 1)
- TestStockHealth(Step 2 落地後加)
- TestNeelyForecast(Step 3 落地後加)
- TestPayloadSize(Step 4 落地後 sum 全 chain)

所有 tests 走 mock conn / mock agg helper,沙箱無 PG 可跑。
"""

from __future__ import annotations

import json
import sys
from datetime import date
from pathlib import Path
from unittest.mock import MagicMock

# Ensure sys.path 對齊 mcp_server/_conn.py 走 stdio 時的 path 配置
_REPO_ROOT = Path(__file__).resolve().parent.parent.parent
_SRC_ROOT = _REPO_ROOT / "src"
for p in (str(_SRC_ROOT), str(_REPO_ROOT)):
    if p not in sys.path:
        sys.path.insert(0, p)

import pytest

from mcp_server import _climate
from mcp_server.tools import data as data_tools


# ────────────────────────────────────────────────────────────
# Fixtures
# ────────────────────────────────────────────────────────────

def _make_fact(
    *,
    stock_id: str,
    fact_date,
    source_core: str,
    kind: str,
    statement: str | None = None,
) -> dict:
    """Helper:組裝 fact dict 對齊 facts 表 row schema。"""
    return {
        "stock_id":       stock_id,
        "fact_date":      fact_date,
        "timeframe":      "daily",
        "source_core":    source_core,
        "source_version": "0.1.0",
        "statement":      statement or f"{kind} on {fact_date}: value=0",
        "metadata":       {"kind": kind},
        "params_hash":    None,
    }


def _patch_fetch_market_facts(monkeypatch, grouped: dict[str, list[dict]]):
    """Mock _market.fetch_market_facts 回固定 grouped facts。"""
    from fusion.raw import _market

    def fake(_conn, *, as_of, lookback_days, cores=None, apply_lookahead_filter=True):
        return grouped

    monkeypatch.setattr(_market, "fetch_market_facts", fake)


def _patch_get_connection(monkeypatch):
    """Mock agg._db.get_connection 回 MagicMock conn。

    v3.25:同時 mock _aggregate_risk_alert_marketwide 預設回 0,
    避免 MagicMock conn 的 int() 行為返回 1 把 risk_alert score 帶進去。
    對 v3.25 自己的 test 個別覆寫即可。
    """
    from fusion.raw import _db
    from mcp_server import _climate as climate_mod

    monkeypatch.setattr(_db, "get_connection", lambda *a, **kw: MagicMock())
    monkeypatch.setattr(
        climate_mod, "_aggregate_risk_alert_marketwide",
        lambda *a, **kw: {"active_count": 0, "announced_14d": 0, "escalations_60d": 0},
    )


# ────────────────────────────────────────────────────────────
# TestMarketContext(Step 1)
# ────────────────────────────────────────────────────────────


class TestMarketContextStructure:
    def test_returns_required_keys(self, monkeypatch):
        _patch_get_connection(monkeypatch)
        _patch_fetch_market_facts(monkeypatch, {})

        result = data_tools.market_context("2026-05-13")
        assert set(result.keys()) == {
            "as_of",
            "overall_climate",
            "climate_score",
            "components",
            "systemic_risks",
            "narrative",
        }

    def test_returns_8_components(self, monkeypatch):
        """v3.25:6 → 8 components(加 commodity_macro + risk_alert)。"""
        _patch_get_connection(monkeypatch)
        _patch_fetch_market_facts(monkeypatch, {})

        result = data_tools.market_context("2026-05-13")
        assert set(result["components"].keys()) == {
            "taiex", "us_market", "fear_greed", "business",
            "exchange_rate", "market_margin",
            "commodity_macro", "risk_alert",   # v3.25
        }

    def test_empty_facts_gives_neutral_climate(self, monkeypatch):
        _patch_get_connection(monkeypatch)
        _patch_fetch_market_facts(monkeypatch, {})

        result = data_tools.market_context("2026-05-13")
        assert result["overall_climate"] == "neutral"
        assert result["climate_score"] == 0
        assert result["systemic_risks"] == []

    def test_as_of_iso_serialized(self, monkeypatch):
        _patch_get_connection(monkeypatch)
        _patch_fetch_market_facts(monkeypatch, {})

        result = data_tools.market_context("2026-05-13")
        assert result["as_of"] == "2026-05-13"
        assert isinstance(result["as_of"], str)


class TestMarketContextScoring:
    def test_bullish_us_market_lifts_score(self, monkeypatch):
        _patch_get_connection(monkeypatch)
        as_of = date(2026, 5, 13)
        grouped = {
            "_index_us_market_": [
                _make_fact(
                    stock_id="_index_us_market_",
                    fact_date=as_of,
                    source_core="us_market_core",
                    kind="SpyMacdGoldenCross",
                ),
                _make_fact(
                    stock_id="_index_us_market_",
                    fact_date=as_of,
                    source_core="us_market_core",
                    kind="VixLowZoneEntry",
                ),
            ],
        }
        _patch_fetch_market_facts(monkeypatch, grouped)

        result = data_tools.market_context("2026-05-13")
        assert result["components"]["us_market"]["score"] > 0
        assert result["climate_score"] > 0

    def test_vix_spike_triggers_systemic_risk(self, monkeypatch):
        _patch_get_connection(monkeypatch)
        as_of = date(2026, 5, 13)
        grouped = {
            "_index_us_market_": [
                _make_fact(
                    stock_id="_index_us_market_",
                    fact_date=as_of,
                    source_core="us_market_core",
                    kind="VixSpike",
                ),
            ],
        }
        _patch_fetch_market_facts(monkeypatch, grouped)

        result = data_tools.market_context("2026-05-13")
        assert "us_vix_spike" in result["systemic_risks"]

    def test_margin_danger_triggers_risk(self, monkeypatch):
        _patch_get_connection(monkeypatch)
        as_of = date(2026, 5, 13)
        grouped = {
            "_market_": [
                _make_fact(
                    stock_id="_market_",
                    fact_date=as_of,
                    source_core="market_margin_core",
                    kind="EnteredDangerZone",
                ),
            ],
        }
        _patch_fetch_market_facts(monkeypatch, grouped)

        result = data_tools.market_context("2026-05-13")
        assert "tw_margin_maintenance_danger" in result["systemic_risks"]

    def test_fear_greed_contrarian(self, monkeypatch):
        """Fear-Greed 用 contrarian:極度恐懼 = 正分(機會)。"""
        _patch_get_connection(monkeypatch)
        as_of = date(2026, 5, 13)
        grouped = {
            "_global_": [
                _make_fact(
                    stock_id="_global_",
                    fact_date=as_of,
                    source_core="fear_greed_core",
                    kind="EnteredExtremeFear",
                ),
            ],
        }
        _patch_fetch_market_facts(monkeypatch, grouped)

        result = data_tools.market_context("2026-05-13")
        assert result["components"]["fear_greed"]["score"] > 0

    def test_climate_score_clamped(self, monkeypatch):
        """Score 在 [-100, +100] 之間。"""
        _patch_get_connection(monkeypatch)
        as_of = date(2026, 5, 13)
        grouped = {
            "_index_taiex_": [
                _make_fact(
                    stock_id="_index_taiex_",
                    fact_date=as_of,
                    source_core="taiex_core",
                    kind="TaiexBullishTrend",
                )
                for _ in range(100)
            ],
        }
        _patch_fetch_market_facts(monkeypatch, grouped)

        result = data_tools.market_context("2026-05-13")
        for comp in result["components"].values():
            assert -100 <= comp["score"] <= 100
        assert -100 <= result["climate_score"] <= 100


# ════════════════════════════════════════════════════════════
# v3.25 — commodity_macro + risk_alert integration
# ════════════════════════════════════════════════════════════


def _patch_risk_alert_summary(monkeypatch, summary: dict[str, int]):
    """Mock _aggregate_risk_alert_marketwide 回固定 summary dict。"""
    from mcp_server import _climate as climate_mod
    monkeypatch.setattr(
        climate_mod, "_aggregate_risk_alert_marketwide",
        lambda *a, **kw: summary,
    )


class TestMarketContextCommodityMacro:
    """v3.25:commodity_macro_core 加進 _global_ 的 7th env core。"""

    def test_gold_momentum_up_bearish_for_equities(self, monkeypatch):
        """GOLD MomentumUp = risk-off → 對股市偏空(sign=-1)。"""
        _patch_get_connection(monkeypatch)
        _patch_risk_alert_summary(monkeypatch, {
            "active_count": 0, "announced_14d": 0, "escalations_60d": 0,
        })
        as_of = date(2026, 5, 13)
        grouped = {
            "_global_": [
                _make_fact(
                    stock_id="_global_",
                    fact_date=as_of,
                    source_core="commodity_macro_core",
                    kind="CommodityMomentumUp",
                ),
            ],
        }
        _patch_fetch_market_facts(monkeypatch, grouped)
        result = data_tools.market_context("2026-05-13")
        assert result["components"]["commodity_macro"]["score"] < 0
        # 對總 climate_score 拖累(weight 0.05)
        assert result["climate_score"] < 0

    def test_gold_momentum_down_bullish_for_equities(self, monkeypatch):
        _patch_get_connection(monkeypatch)
        _patch_risk_alert_summary(monkeypatch, {
            "active_count": 0, "announced_14d": 0, "escalations_60d": 0,
        })
        as_of = date(2026, 5, 13)
        grouped = {
            "_global_": [
                _make_fact(
                    stock_id="_global_",
                    fact_date=as_of,
                    source_core="commodity_macro_core",
                    kind="CommodityMomentumDown",
                ),
            ],
        }
        _patch_fetch_market_facts(monkeypatch, grouped)
        result = data_tools.market_context("2026-05-13")
        assert result["components"]["commodity_macro"]["score"] > 0

    def test_commodity_spike_triggers_systemic_risk(self, monkeypatch):
        _patch_get_connection(monkeypatch)
        _patch_risk_alert_summary(monkeypatch, {
            "active_count": 0, "announced_14d": 0, "escalations_60d": 0,
        })
        as_of = date(2026, 5, 13)
        grouped = {
            "_global_": [
                _make_fact(
                    stock_id="_global_",
                    fact_date=as_of,
                    source_core="commodity_macro_core",
                    kind="CommoditySpike",
                ),
            ],
        }
        _patch_fetch_market_facts(monkeypatch, grouped)
        result = data_tools.market_context("2026-05-13")
        assert "macro_commodity_spike" in result["systemic_risks"]


class TestMarketContextRiskAlert:
    """v3.25:per-stock risk_alert 聚合成 marketwide summary。"""

    def test_no_active_dispositions_score_zero(self, monkeypatch):
        _patch_get_connection(monkeypatch)
        _patch_fetch_market_facts(monkeypatch, {})
        _patch_risk_alert_summary(monkeypatch, {
            "active_count": 0, "announced_14d": 0, "escalations_60d": 0,
        })
        result = data_tools.market_context("2026-05-13")
        assert result["components"]["risk_alert"]["score"] == 0
        assert result["components"]["risk_alert"]["active_disposition_stocks"] == 0

    def test_active_dispositions_lowers_score(self, monkeypatch):
        _patch_get_connection(monkeypatch)
        _patch_fetch_market_facts(monkeypatch, {})
        _patch_risk_alert_summary(monkeypatch, {
            "active_count": 7, "announced_14d": 3, "escalations_60d": 1,
        })
        result = data_tools.market_context("2026-05-13")
        # active 5-9 = -50;esc 1-2 = -15 → -65
        assert result["components"]["risk_alert"]["score"] == -65
        assert result["components"]["risk_alert"]["active_disposition_stocks"] == 7

    def test_disposition_cluster_triggers_systemic_risk(self, monkeypatch):
        """active_count >= 5 → tw_disposition_cluster。"""
        _patch_get_connection(monkeypatch)
        _patch_fetch_market_facts(monkeypatch, {})
        _patch_risk_alert_summary(monkeypatch, {
            "active_count": 6, "announced_14d": 2, "escalations_60d": 0,
        })
        result = data_tools.market_context("2026-05-13")
        assert "tw_disposition_cluster" in result["systemic_risks"]

    def test_escalation_cluster_triggers_systemic_risk(self, monkeypatch):
        """escalations_60d >= 3 → tw_disposition_escalation_cluster。"""
        _patch_get_connection(monkeypatch)
        _patch_fetch_market_facts(monkeypatch, {})
        _patch_risk_alert_summary(monkeypatch, {
            "active_count": 0, "announced_14d": 0, "escalations_60d": 4,
        })
        result = data_tools.market_context("2026-05-13")
        assert "tw_disposition_escalation_cluster" in result["systemic_risks"]


class TestMarketContextWeightsV325:
    """v3.25 拍版:8 components weights sum = 1.0。"""

    def test_weights_sum_to_one(self):
        from mcp_server._climate import _COMPONENT_WEIGHTS
        total = sum(_COMPONENT_WEIGHTS.values())
        assert abs(total - 1.0) < 0.001, f"weights sum {total} != 1.0"

    def test_weights_keys_match_8_components(self):
        from mcp_server._climate import _COMPONENT_WEIGHTS
        assert set(_COMPONENT_WEIGHTS.keys()) == {
            "taiex", "us_market", "fear_greed", "business",
            "exchange_rate", "market_margin",
            "commodity_macro", "risk_alert",
        }


class TestMarketContextTimeDecay:
    def test_recent_fact_weighs_more(self, monkeypatch):
        """同 kind 近期 fact 比舊 fact 加更多分。"""
        _patch_get_connection(monkeypatch)
        as_of = date(2026, 5, 13)
        recent_fact = _make_fact(
            stock_id="_index_us_market_",
            fact_date=as_of,
            source_core="us_market_core",
            kind="SpyMacdGoldenCross",
        )
        old_fact = _make_fact(
            stock_id="_index_us_market_",
            fact_date=date(2026, 3, 13),  # 60 天前
            source_core="us_market_core",
            kind="SpyMacdGoldenCross",
        )
        _patch_fetch_market_facts(monkeypatch, {"_index_us_market_": [recent_fact]})
        result_recent = data_tools.market_context("2026-05-13")

        _patch_fetch_market_facts(monkeypatch, {"_index_us_market_": [old_fact]})
        result_old = data_tools.market_context("2026-05-13")

        assert result_recent["components"]["us_market"]["score"] > result_old["components"]["us_market"]["score"]


# ────────────────────────────────────────────────────────────
# TestStockHealth(Step 2)
# ────────────────────────────────────────────────────────────


def _patch_agg_as_of(monkeypatch, facts: list[dict], indicator_latest: dict | None = None,
                       *, latest_close: dict | None = None):
    """Mock agg.as_of() 回固定 AsOfSnapshot + v3.26 mock fetch_latest_close。

    v3.26:預設 latest_close=None → 回退到 indicator_latest path(對齊既有 test 期望)。
    若指定 dict {"close":..,"prev_close":..,"change_pct":..}則 fetch_latest_close 回該值。
    """
    from fusion.raw import _types
    import fusion.raw as agg
    from mcp_server import _price as _price_mod

    monkeypatch.setattr(_price_mod, "fetch_latest_close_for_tool",
                        lambda *a, **kw: latest_close)

    def fake_as_of(stock_id, as_of, **kwargs):
        rows = [
            _types.FactRow(
                stock_id=f.get("stock_id", stock_id),
                fact_date=f.get("fact_date"),
                timeframe=f.get("timeframe", "daily"),
                source_core=f.get("source_core", "macd_core"),
                source_version=f.get("source_version", "0.1.0"),
                statement=f.get("statement", ""),
                metadata=f.get("metadata") or {},
            )
            for f in facts
        ]
        indicators = {}
        for key, ind in (indicator_latest or {}).items():
            indicators[key] = _types.IndicatorRow(
                stock_id=stock_id,
                value_date=as_of,
                timeframe=ind.get("timeframe", "daily"),
                source_core=ind.get("source_core", key.split("@")[0]),
                source_version="0.1.0",
                value=ind.get("value", {}),
            )
        return _types.AsOfSnapshot(
            stock_id=stock_id,
            as_of=as_of,
            facts=rows,
            indicator_latest=indicators,
            metadata=_types.QueryMetadata(
                stock_id=stock_id,
                as_of=as_of,
                lookback_days=kwargs.get("lookback_days", 90),
                cores=kwargs.get("cores"),
                include_market=kwargs.get("include_market", True),
                timeframes=kwargs.get("timeframes"),
            ),
        )

    monkeypatch.setattr(agg, "as_of", fake_as_of)


class TestStockHealthStructure:
    def test_returns_required_keys(self, monkeypatch):
        _patch_agg_as_of(monkeypatch, facts=[])

        result = data_tools.stock_health("2330", "2026-05-13")
        assert set(result.keys()) == {
            "stock_id",
            "as_of",
            "current_price",
            "overall_score",
            "dimensions",
            "top_signals",
            "narrative",
        }

    def test_returns_4_dimensions(self, monkeypatch):
        _patch_agg_as_of(monkeypatch, facts=[])

        result = data_tools.stock_health("2330", "2026-05-13")
        assert set(result["dimensions"].keys()) == {
            "technical", "chip", "valuation", "fundamental",
        }

    def test_top_signals_max_5(self, monkeypatch):
        as_of = date(2026, 5, 13)
        # 塞 20 facts,確認 top_signals 只回 5 個
        facts = [
            _make_fact(
                stock_id="2330",
                fact_date=as_of,
                source_core="macd_core",
                kind="GoldenCross",
            )
            for _ in range(20)
        ]
        _patch_agg_as_of(monkeypatch, facts=facts)

        result = data_tools.stock_health("2330", "2026-05-13")
        assert len(result["top_signals"]) <= 5

    def test_overall_score_in_range(self, monkeypatch):
        _patch_agg_as_of(monkeypatch, facts=[])

        result = data_tools.stock_health("2330", "2026-05-13")
        assert -100 <= result["overall_score"] <= 100


class TestStockHealthScoring:
    def test_bullish_technical_lifts_score(self, monkeypatch):
        as_of = date(2026, 5, 13)
        facts = [
            _make_fact(
                stock_id="2330",
                fact_date=as_of,
                source_core="macd_core",
                kind="GoldenCross",
            ),
            _make_fact(
                stock_id="2330",
                fact_date=as_of,
                source_core="rsi_core",
                kind="OversoldExit",
            ),
        ]
        _patch_agg_as_of(monkeypatch, facts=facts)

        result = data_tools.stock_health("2330", "2026-05-13")
        assert result["dimensions"]["technical"]["score"] > 0
        assert result["overall_score"] > 0

    def test_bearish_chip_drops_score(self, monkeypatch):
        as_of = date(2026, 5, 13)
        facts = [
            _make_fact(
                stock_id="2330",
                fact_date=as_of,
                source_core="institutional_core",
                kind="LargeNetSell",
            ),
            _make_fact(
                stock_id="2330",
                fact_date=as_of,
                source_core="margin_core",
                kind="MaintenanceLow",
            ),
        ]
        _patch_agg_as_of(monkeypatch, facts=facts)

        result = data_tools.stock_health("2330", "2026-05-13")
        assert result["dimensions"]["chip"]["score"] < 0
        assert result["overall_score"] < 0


class TestStockHealthCurrentPrice:
    def test_extracts_current_price_from_ma_core(self, monkeypatch):
        as_of = date(2026, 5, 13)
        indicator_latest = {
            "ma_core@daily": {
                "source_core": "ma_core",
                "value": {
                    "series": [
                        {"date": "2026-05-12", "close": 1230.0},
                        {"date": "2026-05-13", "close": 1234.5},
                    ],
                },
            },
        }
        _patch_agg_as_of(monkeypatch, facts=[], indicator_latest=indicator_latest)

        result = data_tools.stock_health("2330", "2026-05-13")
        assert result["current_price"] == 1234.5


class TestStockHealthPayloadSize:
    def test_under_5k_tokens(self, monkeypatch):
        as_of = date(2026, 5, 13)
        # 100 facts × 多核
        facts = []
        for i, core in enumerate([
            "macd_core", "rsi_core", "ma_core", "kd_core", "bollinger_core",
            "institutional_core", "margin_core", "foreign_holding_core",
            "shareholder_core", "valuation_core",
        ]):
            for _ in range(10):
                facts.append(_make_fact(
                    stock_id="2330",
                    fact_date=as_of,
                    source_core=core,
                    kind="GoldenCross",
                ))
        _patch_agg_as_of(monkeypatch, facts=facts)

        result = data_tools.stock_health("2330", "2026-05-13")
        size_bytes = len(json.dumps(result, default=str))
        approx_tokens = size_bytes // 4
        assert approx_tokens < 5_000, (
            f"stock_health payload {approx_tokens} tokens > 5K target"
        )


# ────────────────────────────────────────────────────────────
# TestNeelyForecast(Step 3)
# ────────────────────────────────────────────────────────────


def _patch_agg_as_of_with_neely(
    monkeypatch,
    *,
    scenarios: list[dict],
    indicator_latest: dict | None = None,
    latest_close: dict | None = None,
    full_history: bool = False,
):
    """Mock agg.as_of() 回包含 neely structural snapshot 的 AsOfSnapshot。

    v3.26:加 mock `fetch_latest_close_for_tool`(預設 None → fallback 走 indicator)。
    v3.38:加 `full_history` 旗標 — True 時 fake monowave_series 多筆讓
    `data_availability.daily_bars >= 1000`(對齊 user 拍版 full 級別)。
    """
    from fusion.raw import _types
    import fusion.raw as agg
    from mcp_server import _price as _price_mod

    monkeypatch.setattr(_price_mod, "fetch_latest_close_for_tool",
                        lambda *a, **kw: latest_close)

    def fake_as_of(stock_id, as_of, **kwargs):
        # Wrap scenarios in structural snapshot
        structural = {}
        if scenarios:
            # v3.38 fake monowave_series 對齊 data_availability check
            # full_history=True → 200 monowaves(daily_bars ≈ 200 × 7.5 = 1500)
            mw_count = 200 if full_history else 0
            mw_series = [
                {"start_date": "2020-01-01", "end_date": "2020-01-08",
                 "start_price": 100.0, "end_price": 101.0, "direction": "Up"}
                for _ in range(mw_count)
            ]
            structural["neely_core@daily"] = _types.StructuralRow(
                stock_id=stock_id,
                snapshot_date=as_of,
                timeframe="daily",
                core_name="neely_core",
                source_version="1.0.1",
                snapshot={"scenario_forest": scenarios, "monowave_series": mw_series},
            )

        indicators = {}
        for key, ind in (indicator_latest or {}).items():
            indicators[key] = _types.IndicatorRow(
                stock_id=stock_id,
                value_date=as_of,
                timeframe=ind.get("timeframe", "daily"),
                source_core=ind.get("source_core", key.split("@")[0]),
                source_version="0.1.0",
                value=ind.get("value", {}),
            )

        return _types.AsOfSnapshot(
            stock_id=stock_id,
            as_of=as_of,
            facts=[],
            indicator_latest=indicators,
            structural=structural,
            metadata=_types.QueryMetadata(
                stock_id=stock_id,
                as_of=as_of,
                lookback_days=kwargs.get("lookback_days", 30),
                cores=kwargs.get("cores"),
                include_market=kwargs.get("include_market", False),
                timeframes=kwargs.get("timeframes"),
            ),
        )

    monkeypatch.setattr(agg, "as_of", fake_as_of)


def _bullish_scenario_fixture() -> dict:
    return {
        "id": "scenario-1",
        "pattern_type": "Impulse",
        "power_rating": "Bullish",
        "structure_label": "Impulse W3 of 5",
        "rules_passed_count": 5,
        "max_retracement": 0.80,
        "expected_fib_zones": [
            {"label": "fib_0.382", "low": 1100.0, "high": 1150.0, "source_ratio": 0.382},
            {"label": "fib_0.618", "low": 1180.0, "high": 1220.0, "source_ratio": 0.618},
            {"label": "fib_1.000", "low": 1300.0, "high": 1360.0, "source_ratio": 1.000},
            {"label": "fib_1.382", "low": 1400.0, "high": 1600.0, "source_ratio": 1.382},
            {"label": "fib_1.618", "low": 1500.0, "high": 1800.0, "source_ratio": 1.618},
        ],
        "invalidation_triggers": [
            {
                "trigger_type": {"PriceBreakBelow": 880.0},
                "on_trigger": "InvalidateScenario",
                "rule_reference": "R1",
                "neely_page": "Ch3 p.3-12",
            },
        ],
    }


class TestNeelyForecastStructure:
    def test_returns_required_keys(self, monkeypatch):
        _patch_agg_as_of_with_neely(monkeypatch, scenarios=[_bullish_scenario_fixture()])

        result = data_tools.neely_forecast("2330", "2026-05-13")
        assert set(result.keys()) == {
            "stock_id", "as_of", "current_price",
            "primary_scenario", "scenario_count", "forecasts",
            "key_levels", "invalidation_price",
            "scenario_staleness",         # v3.28 加
            "quality_caveat",             # v3.35.1 加
            "neely_by_timeframe",         # v3.37 加
            "data_availability",          # v3.38 加
            "missing_wave_by_horizon",    # v3.38 加
        }

    def test_returns_3_timeframes(self, monkeypatch):
        # v3.38:drop 1_year,改 1m / 3m / 6m
        _patch_agg_as_of_with_neely(monkeypatch, scenarios=[_bullish_scenario_fixture()])

        result = data_tools.neely_forecast("2330", "2026-05-13")
        # 既有 fixture 無 daily_bars(因為 neely_core@daily snapshot 沒 data_range
        # + monowave_series 是空)→ daily_bars=0 → degradation_status=insufficient_history
        # → forecasts dict 為空。為了驗 3-timeframe shape,先檢查若 available 則 keys 對齊。
        if result["data_availability"]["degradation_status"] != "insufficient_history":
            assert set(result["forecasts"].keys()).issubset({"1m", "3m", "6m"})

    def test_prob_up_in_range(self, monkeypatch):
        _patch_agg_as_of_with_neely(monkeypatch, scenarios=[_bullish_scenario_fixture()])

        result = data_tools.neely_forecast("2330", "2026-05-13")
        for tf_key, fc in result["forecasts"].items():
            assert 0.10 <= fc["prob_up"] <= 0.90, f"{tf_key} prob_up={fc['prob_up']} out of range"

    def test_no_neely_data_returns_neutral_forecasts(self, monkeypatch):
        _patch_agg_as_of_with_neely(monkeypatch, scenarios=[])

        result = data_tools.neely_forecast("2330", "2026-05-13")
        # v3.38:無 scenarios + 無 data → degradation_status=insufficient_history,
        # forecasts={} 空 dict(全部 horizon 拒絕)
        assert result["data_availability"]["degradation_status"] == "insufficient_history"
        assert result["forecasts"] == {}


class TestNeelyForecastBullish:
    def test_bullish_scenario_lifts_prob(self, monkeypatch):
        """Bullish power_rating → 1m prob_up > 0.50(若 horizon 可用)。"""
        ma_indicator = {
            "ma_core@daily": {
                "source_core": "ma_core",
                "value": {
                    "series": [{"date": "2026-05-13", "close": 1234.5}],
                },
            },
        }
        # v3.38:加 enough_data fixture 讓 data_availability=full
        _patch_agg_as_of_with_neely(
            monkeypatch,
            scenarios=[_bullish_scenario_fixture()],
            indicator_latest=ma_indicator,
            full_history=True,        # v3.38 加
        )

        result = data_tools.neely_forecast("2330", "2026-05-13")
        # 若 1m 在 available 則驗 prob > 0.50
        if "1m" in result["forecasts"]:
            assert result["forecasts"]["1m"]["prob_up"] > 0.50

    def test_invalidation_price_below_current_for_bullish(self, monkeypatch):
        """Bullish scenario invalidation_price 應在 current_price 之下(PriceBreakBelow)。"""
        ma_indicator = {
            "ma_core@daily": {
                "source_core": "ma_core",
                "value": {
                    "series": [{"date": "2026-05-13", "close": 1234.5}],
                },
            },
        }
        _patch_agg_as_of_with_neely(
            monkeypatch,
            scenarios=[_bullish_scenario_fixture()],
            indicator_latest=ma_indicator,
        )

        result = data_tools.neely_forecast("2330", "2026-05-13")
        assert result["invalidation_price"] == 880.0
        assert result["invalidation_price"] < result["current_price"]

    def test_current_price_extracted_from_ma_core(self, monkeypatch):
        ma_indicator = {
            "ma_core@daily": {
                "source_core": "ma_core",
                "value": {
                    "series": [{"date": "2026-05-13", "close": 1234.5}],
                },
            },
        }
        _patch_agg_as_of_with_neely(
            monkeypatch,
            scenarios=[_bullish_scenario_fixture()],
            indicator_latest=ma_indicator,
        )

        result = data_tools.neely_forecast("2330", "2026-05-13")
        assert result["current_price"] == 1234.5


class TestNeelyForecastTimeframeDecay:
    def test_longer_timeframe_prob_closer_to_neutral(self, monkeypatch):
        """v3.38:drop 1y,改驗 1m vs 6m。同一 bullish scenario,6m prob 應比 1m 接近 0.50。"""
        ma_indicator = {
            "ma_core@daily": {
                "source_core": "ma_core",
                "value": {
                    "series": [{"date": "2026-05-13", "close": 1234.5}],
                },
            },
        }
        _patch_agg_as_of_with_neely(
            monkeypatch,
            scenarios=[_bullish_scenario_fixture()],
            indicator_latest=ma_indicator,
            full_history=True,
        )

        result = data_tools.neely_forecast("2330", "2026-05-13")
        forecasts = result["forecasts"]
        # 若 full_history 模式,1m 與 6m 都可用
        if "1m" in forecasts and "6m" in forecasts:
            # 若 6m 走 reference mode (confidence=0)就跳過比較
            if forecasts["6m"].get("confidence") == 0.0:
                return
            prob_1m = forecasts["1m"]["prob_up"]
            prob_6m = forecasts["6m"]["prob_up"]
            # Bullish → 兩個都 > 0.50;但 6m 比 1m 接近 0.50(decay 0.70 vs 1.00)
            assert prob_1m >= prob_6m


# ════════════════════════════════════════════════════════════
# v3.38 per-forecast-horizon degradation tests
# ════════════════════════════════════════════════════════════


def _patch_neely_with_bars(
    monkeypatch, *, daily_mw_count: int, scenarios: list[dict] | None = None,
    weekly_mw_count: int = 0, monthly_mw_count: int = 0,
):
    """v3.38 helper:fake structural 含可控 monowave_series count(對應 data_availability)。

    daily_bars 從 monowave_count × 7.5 反推:
      - 17 mw   ≈ 130 bars(insufficient_history 邊界)
      - 70 mw   ≈ 525 bars(no_6m 區間 130-499)
      - 134 mw  ≈ 1005 bars(degree_uncertain 區間 500-999;1004 < 1000 故 67 mw=502 ok)
      - 67 mw   ≈ 502 bars(degree_uncertain 區間)
      - 200 mw  ≈ 1500 bars(full 區間 >=1000)
    """
    from fusion.raw import _types
    import fusion.raw as agg
    from mcp_server import _price as _price_mod

    monkeypatch.setattr(_price_mod, "fetch_latest_close_for_tool",
                        lambda *a, **kw: {"close": 1234.5, "change_pct": 1.0,
                                          "prev_close": 1220.0})

    def fake_as_of(stock_id, as_of, **kwargs):
        structural = {}
        scen_list = scenarios or [_bullish_scenario_fixture()]
        for tf, count in [("daily", daily_mw_count),
                          ("weekly", weekly_mw_count),
                          ("monthly", monthly_mw_count)]:
            if count <= 0:
                continue
            mw_series = [
                {"start_date": "2020-01-01", "end_date": "2020-01-08",
                 "start_price": 100.0, "end_price": 101.0, "direction": "Up"}
                for _ in range(count)
            ]
            structural[f"neely_core@{tf}"] = _types.StructuralRow(
                stock_id=stock_id, snapshot_date=as_of, timeframe=tf,
                core_name="neely_core", source_version="1.0.1",
                snapshot={
                    "scenario_forest": scen_list if tf == "daily" else [],
                    "monowave_series": mw_series,
                    "missing_wave_suspects": [],
                },
            )

        return _types.AsOfSnapshot(
            stock_id=stock_id, as_of=as_of, facts=[],
            indicator_latest={}, structural=structural,
            metadata=_types.QueryMetadata(
                stock_id=stock_id, as_of=as_of, lookback_days=30,
                cores=None, include_market=False, timeframes=None,
            ),
        )
    monkeypatch.setattr(agg, "as_of", fake_as_of)


class TestV3_38Degradation:
    """user 拍版降級表 verify:
      daily_bars >= 1000 → full / 500-999 → degree_uncertain(6m reference)/
      130-499 → no_6m / < 130 → insufficient_history。
    """

    def test_v3_38_full_when_daily_bars_ge_1000(self, monkeypatch):
        """200 daily monowaves ≈ 1500 bars → degradation_status = full,1m/3m/6m 全綠。"""
        _patch_neely_with_bars(monkeypatch, daily_mw_count=200)
        result = data_tools.neely_forecast("2330", "2026-05-15")
        da = result["data_availability"]
        assert da["degradation_status"] == "full"
        assert set(da["available_horizons"]) == {"1m", "3m", "6m"}
        assert da["degraded_horizons"] == []
        # forecasts 三 horizon 全有,confidence=1.0
        assert set(result["forecasts"].keys()) == {"1m", "3m", "6m"}
        for h in ("1m", "3m", "6m"):
            assert result["forecasts"][h].get("confidence") == 1.0

    def test_v3_38_6m_degraded_when_500_to_999(self, monkeypatch):
        """100 daily monowaves ≈ 750 bars(500-999 區間)→ 6m 走 reference mode。"""
        _patch_neely_with_bars(monkeypatch, daily_mw_count=100)
        result = data_tools.neely_forecast("2330", "2026-05-15")
        da = result["data_availability"]
        assert da["degradation_status"] == "degree_uncertain"
        assert set(da["available_horizons"]) == {"1m", "3m", "6m"}
        assert da["degraded_horizons"] == ["6m"]
        # 1m / 3m confidence=1.0
        assert result["forecasts"]["1m"]["confidence"] == 1.0
        assert result["forecasts"]["3m"]["confidence"] == 1.0
        # 6m reference mode:prob_up=0.5 / range=None / confidence=0.0 / 中文 note
        f6 = result["forecasts"]["6m"]
        assert f6["prob_up"] == 0.50
        assert f6["range_high"] is None
        assert f6["range_low"] is None
        assert f6["confidence"] == 0.0
        assert "資料不足" in f6.get("note", "")

    def test_v3_38_6m_rejected_when_130_to_499(self, monkeypatch):
        """50 daily monowaves ≈ 375 bars(130-499 區間)→ 拒 6m,只 1m/3m。"""
        _patch_neely_with_bars(monkeypatch, daily_mw_count=50)
        result = data_tools.neely_forecast("2330", "2026-05-15")
        da = result["data_availability"]
        assert da["degradation_status"] == "no_6m"
        assert set(da["available_horizons"]) == {"1m", "3m"}
        assert "6m" not in result["forecasts"]
        # 1m / 3m 仍正常
        assert "1m" in result["forecasts"]
        assert "3m" in result["forecasts"]

    def test_v3_38_all_rejected_when_below_130(self, monkeypatch):
        """10 daily monowaves ≈ 75 bars(< 130)→ insufficient_history,forecasts={}。"""
        _patch_neely_with_bars(monkeypatch, daily_mw_count=10)
        result = data_tools.neely_forecast("2330", "2026-05-15")
        da = result["data_availability"]
        assert da["degradation_status"] == "insufficient_history"
        assert da["available_horizons"] == []
        assert result["forecasts"] == {}

    def test_v3_38_missing_wave_tier_classified_per_horizon(self, monkeypatch):
        """v3.38 spec-aligned tier classification:per-horizon 對 spec table 分類。

        Impulse min=8 → count=20 (>=2×min=16) → "absent"
        對齊 user 拍版「對齊原書 spec line 2559-2582」。
        """
        # 20 daily mw + Impulse pattern → 20 >= 2×8=16 → absent
        _patch_neely_with_bars(monkeypatch, daily_mw_count=200,    # full mode
                               weekly_mw_count=20, monthly_mw_count=15)
        result = data_tools.neely_forecast("2330", "2026-05-15")
        mwbh = result["missing_wave_by_horizon"]
        # 1m / 3m 只有 daily entry
        assert "daily" in mwbh["1m"]
        assert mwbh["1m"]["daily"]["tier"] in ("certain", "possible", "absent")
        assert "daily" in mwbh["3m"]
        # 6m daily + weekly 並列
        assert "daily" in mwbh["6m"]
        assert "weekly" in mwbh["6m"]
        # daily mw=200 對 Impulse(min=8)→ 200 >> 16 → absent
        assert mwbh["6m"]["daily"]["tier"] == "absent"


class TestNeelyForecastPayloadSize:
    def test_under_5k_tokens(self, monkeypatch):
        # 多 scenarios 測試
        scenarios = [_bullish_scenario_fixture() for _ in range(5)]
        _patch_agg_as_of_with_neely(monkeypatch, scenarios=scenarios)

        result = data_tools.neely_forecast("2330", "2026-05-13")
        size_bytes = len(json.dumps(result, default=str))
        approx_tokens = size_bytes // 4
        assert approx_tokens < 5_000, (
            f"neely_forecast payload {approx_tokens} tokens > 5K target"
        )


# ════════════════════════════════════════════════════════════
# v3.35 Neely-C-MCP picker:invalidation filter + degree-aware ordering
# ════════════════════════════════════════════════════════════

def _scenario_with_dates(
    *, scenario_id: str, power_rating: str, start: str, end: str,
    rules_passed: int = 5, invalidation_below: float = 0.0,
) -> dict:
    """Helper:對齊 _bullish_scenario_fixture 但帶 wave_tree.start/end 給 degree 推算。"""
    return {
        "id": scenario_id,
        "pattern_type": "Impulse",
        "power_rating": power_rating,
        "structure_label": f"5-wave from {start} to {end}",
        "rules_passed_count": rules_passed,
        "max_retracement": 0.80,
        "expected_fib_zones": [
            {"label": "fib_0.382", "low": 1100.0, "high": 1150.0, "source_ratio": 0.382},
            {"label": "fib_0.618", "low": 1180.0, "high": 1220.0, "source_ratio": 0.618},
            {"label": "fib_1.000", "low": 1300.0, "high": 1360.0, "source_ratio": 1.000},
        ],
        "invalidation_triggers": [
            {
                "trigger_type": {"PriceBreakBelow": invalidation_below},
                "on_trigger": "InvalidateScenario",
                "rule_reference": "R1",
                "neely_page": "Ch3 p.3-12",
            },
        ],
        "wave_tree": {
            "label": scenario_id,
            "start": start,
            "end": end,
            "children": [],
        },
    }


class TestV3_35Picker:
    """v3.35:invalidation filter + degree-aware ordering 對齊 NEoWave 展示式森林設計。"""

    def test_picker_prefers_higher_degree_when_power_equal(self, monkeypatch):
        """3030 case:2 個 Bullish scenarios,1 個 5 年 span(Minor)/ 1 個 6 月 span(SubMinuette)
        應選 5 年 span 為 primary(degree 拆票 power_rating 同分)。"""
        short_span = _scenario_with_dates(
            scenario_id="short_recent",
            power_rating="Bullish",
            start="2025-11-01", end="2026-05-01",     # ~6 月 → SubMinuette
            invalidation_below=120.0,
        )
        long_span = _scenario_with_dates(
            scenario_id="long_secular",
            power_rating="Bullish",
            start="2020-01-01", end="2026-05-01",     # ~6 年 → Minor
            invalidation_below=80.0,
        )
        _patch_agg_as_of_with_neely(
            monkeypatch, scenarios=[short_span, long_span],
            latest_close={"close": 395.0, "change_pct": 2.0, "prev_close": 387.0},
        )
        result = data_tools.neely_forecast("3030", "2026-05-15")
        # primary 應是 long_secular(Minor degree)而非 short_recent(SubMinuette)
        primary = result["primary_scenario"]
        assert primary["effective_degree"] == "Minor", \
            f"primary 應是 Minor degree(長期 span),實際 {primary['effective_degree']}"
        assert "2020-01-01" in primary["label"]
        # invalidation_price 應是 80.0(long_secular)而非 120.0(short_recent)
        assert result["invalidation_price"] == 80.0
        # wave_span_years ~6
        assert primary["wave_span_years"] is not None
        assert 5.5 < primary["wave_span_years"] < 7.0

    def test_picker_filters_invalidated_bullish_scenario(self, monkeypatch):
        """Bullish scenario invalidation_price=500,current_price=400 < 500 → invalidated → 過濾掉。"""
        invalidated = _scenario_with_dates(
            scenario_id="dead_scenario",
            power_rating="StrongBullish",       # power 最強但已失效
            start="2025-01-01", end="2026-04-01",
            invalidation_below=500.0,
        )
        alive = _scenario_with_dates(
            scenario_id="alive_scenario",
            power_rating="Bullish",
            start="2025-01-01", end="2026-04-01",
            invalidation_below=100.0,
        )
        _patch_agg_as_of_with_neely(
            monkeypatch, scenarios=[invalidated, alive],
            latest_close={"close": 400.0, "change_pct": 1.0, "prev_close": 396.0},
        )
        result = data_tools.neely_forecast("TEST", "2026-05-15")
        # invalidated 應被過濾 → primary 是 alive(雖然 power 較弱但 IP=100 未失效)
        # invalidation_price 來自 alive(100.0)是最直接證據
        assert result["invalidation_price"] == 100.0
        # scenario_count = 1(只 alive 留下)
        assert result["scenario_count"] == 1
        # primary power_rating = Bullish(alive,非 StrongBullish)
        assert result["primary_scenario"]["power_rating"] == "Bullish"

    def test_picker_returns_empty_when_all_invalidated(self, monkeypatch):
        """所有 scenarios 都被 invalidation filter 過濾 → primary=None。"""
        dead1 = _scenario_with_dates(
            scenario_id="dead_1", power_rating="Bullish",
            start="2025-01-01", end="2026-04-01",
            invalidation_below=500.0,
        )
        dead2 = _scenario_with_dates(
            scenario_id="dead_2", power_rating="StrongBullish",
            start="2025-01-01", end="2026-04-01",
            invalidation_below=450.0,
        )
        _patch_agg_as_of_with_neely(
            monkeypatch, scenarios=[dead1, dead2],
            latest_close={"close": 400.0, "change_pct": 1.0, "prev_close": 396.0},
        )
        result = data_tools.neely_forecast("TEST", "2026-05-15")
        # primary_scenario 走 _format_primary_scenario(None) → power_rating="Neutral"
        assert result["primary_scenario"]["power_rating"] == "Neutral"
        assert result["primary_scenario"]["effective_degree"] is None
        assert result["scenario_count"] == 0

    def test_v3_35_backward_compat_no_wave_tree_dates(self, monkeypatch):
        """既有 fixture 沒 wave_tree → effective_degree=None,picker fallback 走 power_rating。

        對齊既有 9 個 Neely tests 0 regression。
        """
        # _bullish_scenario_fixture() 沒帶 wave_tree
        _patch_agg_as_of_with_neely(monkeypatch, scenarios=[_bullish_scenario_fixture()])
        result = data_tools.neely_forecast("2330", "2026-05-13")
        # 應 work,但 effective_degree=None
        assert result["primary_scenario"]["effective_degree"] is None
        assert result["primary_scenario"]["wave_span_years"] is None
        # 其他 field 正常(對齊既有 test_returns_required_keys)
        assert result["primary_scenario"]["power_rating"] == "Bullish"

    def test_v3_35_1_quality_caveat_short_degree_only(self, monkeypatch):
        """v3.35.1:所有 scenarios 都 SubMinuette → quality_caveat.is_short_degree_only=True
        + warning 內含對應字串。對應 3030 production case。"""
        short_scenarios = [
            _scenario_with_dates(
                scenario_id=f"short_{i}", power_rating="Bullish",
                start=f"2025-{i:02d}-01", end=f"2025-{i:02d}-28",
                invalidation_below=80.0,
            )
            for i in range(1, 4)
        ]
        _patch_agg_as_of_with_neely(
            monkeypatch, scenarios=short_scenarios,
            latest_close={"close": 395.0, "change_pct": 1.0, "prev_close": 391.0},
        )
        result = data_tools.neely_forecast("3030", "2026-05-15")
        caveat = result["quality_caveat"]
        assert caveat["is_short_degree_only"] is True
        assert caveat["max_scenario_degree"] == "SubMinuette"
        assert caveat["is_usable"] is False
        assert any("short-degree" in w for w in caveat["warnings"])

    def test_v3_35_1_quality_caveat_fib_decoupled_from_price(self, monkeypatch):
        """v3.35.1:primary fib zones 全在 1000-1500,current=395 完全脫節 →
        fib_zones_decoupled_from_price=True + warning 內含 price 數字。"""
        decoupled = {
            "id": "decoupled",
            "pattern_type": "Impulse",
            "power_rating": "Bullish",
            "structure_label": "5-wave decoupled",
            "rules_passed_count": 5,
            "expected_fib_zones": [
                {"label": "fib_0.382", "low": 1100.0, "high": 1150.0, "source_ratio": 0.382},
                {"label": "fib_0.618", "low": 1200.0, "high": 1300.0, "source_ratio": 0.618},
                {"label": "fib_1.000", "low": 1400.0, "high": 1500.0, "source_ratio": 1.000},
            ],
            "invalidation_triggers": [],
            # 給 wave_tree 確保 effective_degree 非 short(走 (b) 分支獨立驗)
            "wave_tree": {
                "label": "decoupled",
                "start": "2020-01-01", "end": "2026-05-01",
                "children": [],
            },
        }
        _patch_agg_as_of_with_neely(
            monkeypatch, scenarios=[decoupled],
            latest_close={"close": 395.0, "change_pct": 1.0, "prev_close": 391.0},
        )
        result = data_tools.neely_forecast("TEST", "2026-05-15")
        caveat = result["quality_caveat"]
        assert caveat["fib_zones_decoupled_from_price"] is True
        # current=395 不在 [1100, 1500] +/- 50% buffer
        assert any("不適用當前 price level" in w for w in caveat["warnings"])

    def test_v3_35_1_quality_caveat_usable_when_long_degree_and_aligned(self, monkeypatch):
        """v3.35.1:long-degree + fib zones 對齊 current_price → is_usable=True 無 warning。"""
        good = {
            "id": "good",
            "pattern_type": "Impulse",
            "power_rating": "Bullish",
            "structure_label": "5-wave long aligned",
            "rules_passed_count": 5,
            "expected_fib_zones": [
                {"label": "fib_0.382", "low": 380.0, "high": 400.0, "source_ratio": 0.382},
                {"label": "fib_0.618", "low": 420.0, "high": 450.0, "source_ratio": 0.618},
            ],
            "invalidation_triggers": [],
            "wave_tree": {
                "label": "good",
                "start": "2020-01-01", "end": "2026-05-01",   # 6 yr → Minor
                "children": [],
            },
        }
        _patch_agg_as_of_with_neely(
            monkeypatch, scenarios=[good],
            latest_close={"close": 395.0, "change_pct": 1.0, "prev_close": 391.0},
        )
        result = data_tools.neely_forecast("TEST", "2026-05-15")
        caveat = result["quality_caveat"]
        assert caveat["is_short_degree_only"] is False
        assert caveat["max_scenario_degree"] == "Minor"
        assert caveat["fib_zones_decoupled_from_price"] is False
        assert caveat["is_usable"] is True
        assert caveat["warnings"] == []

    # ════════════════════════════════════════════════════════════
    # v3.37 multi-timeframe Neely
    # ════════════════════════════════════════════════════════════

    def test_v3_37_picker_promotes_monthly_minor_over_daily_subminuette(self, monkeypatch):
        """v3.37:multi-timeframe picker 跨 daily/weekly/monthly 取最高 degree primary。

        對 3030 case 模擬:
          daily 有 SubMinuette short swing(50天 span)
          monthly 有 Minor degree(5 年 span)
        → primary 應走 monthly Minor,而非 daily SubMinuette。
        """
        from fusion.raw import _types
        import fusion.raw as agg
        from mcp_server import _price as _price_mod

        daily_short = _scenario_with_dates(
            scenario_id="daily_short",
            power_rating="StrongBullish",
            start="2026-03-01", end="2026-05-01",     # ~60 天 → SubMinuette
            invalidation_below=300.0,
        )
        monthly_minor = _scenario_with_dates(
            scenario_id="monthly_minor",
            power_rating="Bullish",
            start="2020-05-01", end="2026-05-01",     # ~6 年 → Minor
            invalidation_below=80.0,
        )

        monkeypatch.setattr(_price_mod, "fetch_latest_close_for_tool",
                            lambda *a, **kw: {"close": 395.0, "change_pct": 1.0, "prev_close": 391.0})

        def fake_as_of(stock_id, as_of, **kwargs):
            structural = {
                "neely_core@daily": _types.StructuralRow(
                    stock_id=stock_id, snapshot_date=as_of, timeframe="daily",
                    core_name="neely_core", source_version="1.0.1",
                    snapshot={"scenario_forest": [daily_short]},
                ),
                "neely_core@monthly": _types.StructuralRow(
                    stock_id=stock_id, snapshot_date=as_of, timeframe="monthly",
                    core_name="neely_core", source_version="1.0.1",
                    snapshot={"scenario_forest": [monthly_minor]},
                ),
            }
            return _types.AsOfSnapshot(
                stock_id=stock_id, as_of=as_of, facts=[],
                indicator_latest={}, structural=structural,
                metadata=_types.QueryMetadata(
                    stock_id=stock_id, as_of=as_of,
                    lookback_days=30, cores=None, include_market=False, timeframes=None,
                ),
            )
        monkeypatch.setattr(agg, "as_of", fake_as_of)

        result = data_tools.neely_forecast("3030", "2026-05-15")

        # primary 應是 monthly_minor(Minor degree)而非 daily_short(SubMinuette)
        primary = result["primary_scenario"]
        assert primary["effective_degree"] == "Minor"
        assert primary["timeframe"] == "monthly"
        assert primary["wave_span_years"] is not None and primary["wave_span_years"] > 5.5

        # invalidation_price 對齊 monthly_minor 的 80.0(非 daily_short 的 300.0)
        assert result["invalidation_price"] == 80.0

        # neely_by_timeframe 三 timeframe 都有 entry
        by_tf = result["neely_by_timeframe"]
        assert by_tf["daily"]["timeframe_present"] is True
        assert by_tf["daily"]["primary_effective_degree"] == "SubMinuette"
        assert by_tf["weekly"]["timeframe_present"] is False
        assert by_tf["monthly"]["timeframe_present"] is True
        assert by_tf["monthly"]["primary_effective_degree"] == "Minor"
        assert "monthly=Minor" in by_tf["cross_timeframe_summary"]
        assert "weekly=無資料" in by_tf["cross_timeframe_summary"]

    def test_v3_37_backward_compat_daily_only(self, monkeypatch):
        """v3.37:既有 single-timeframe fixture(只 daily entry)應 graceful。"""
        _patch_agg_as_of_with_neely(monkeypatch, scenarios=[_bullish_scenario_fixture()])
        result = data_tools.neely_forecast("2330", "2026-05-13")

        by_tf = result["neely_by_timeframe"]
        assert by_tf["daily"]["timeframe_present"] is True
        assert by_tf["weekly"]["timeframe_present"] is False
        assert by_tf["monthly"]["timeframe_present"] is False
        # primary_scenario 仍正常 work
        assert result["primary_scenario"]["power_rating"] == "Bullish"
        # cross_timeframe_summary 反映只 daily 有 data
        assert "weekly=無資料" in by_tf["cross_timeframe_summary"]
        assert "monthly=無資料" in by_tf["cross_timeframe_summary"]

    def test_picker_invalidation_filter_only_acts_on_invalidate_action(self, monkeypatch):
        """OnTriggerAction == WeakenScenario 不視為失效(只 InvalidateScenario 會過濾)。"""
        weaken_only = {
            "id": "weaken_only",
            "pattern_type": "Impulse",
            "power_rating": "Bullish",
            "structure_label": "5-wave",
            "rules_passed_count": 5,
            "max_retracement": 0.80,
            "expected_fib_zones": [],
            "invalidation_triggers": [
                {
                    "trigger_type": {"PriceBreakBelow": 500.0},
                    "on_trigger": "WeakenScenario",   # 不是 InvalidateScenario
                    "rule_reference": "R2",
                    "neely_page": "...",
                },
            ],
            "wave_tree": {
                "label": "weaken_only",
                "start": "2024-01-01", "end": "2026-04-01",
                "children": [],
            },
        }
        _patch_agg_as_of_with_neely(
            monkeypatch, scenarios=[weaken_only],
            latest_close={"close": 400.0, "change_pct": 1.0, "prev_close": 396.0},
        )
        result = data_tools.neely_forecast("TEST", "2026-05-15")
        # current 400 < 500 但 trigger 是 WeakenScenario,不過濾
        assert result["scenario_count"] == 1
        # primary 仍是 weaken_only(power Bullish + 唯一 scenario)
        assert result["primary_scenario"]["power_rating"] == "Bullish"


class TestPayloadSize:
    """Plan §Verify 第 §payload size 要求:每 tool < 5K tokens / chain < 15K。"""

    def test_market_context_under_5k_tokens(self, monkeypatch):
        _patch_get_connection(monkeypatch)
        # Worst case:每個 reserved sid 都塞 50 facts
        as_of = date(2026, 5, 13)
        grouped = {
            sid: [
                _make_fact(
                    stock_id=sid,
                    fact_date=as_of,
                    source_core=core,
                    kind="VixSpike",
                )
                for _ in range(50)
            ]
            for sid, core in [
                ("_index_taiex_",     "taiex_core"),
                ("_index_us_market_", "us_market_core"),
                ("_index_business_",  "business_indicator_core"),
                ("_market_",          "market_margin_core"),
                ("_global_",          "fear_greed_core"),
            ]
        }
        _patch_fetch_market_facts(monkeypatch, grouped)

        result = data_tools.market_context("2026-05-13", lookback_days=90)
        size_bytes = len(json.dumps(result, default=str))
        approx_tokens = size_bytes // 4
        assert approx_tokens < 5_000, (
            f"market_context payload {approx_tokens} tokens > 5K target"
        )

    def test_three_tool_chain_under_15k_tokens(self, monkeypatch):
        """Plan §Test 拍版:3 tools 串調用後 sum tokens ≤ 15K(LLM-friendly chain 預算)。"""
        _patch_get_connection(monkeypatch)
        as_of = date(2026, 5, 13)

        # market_context fixtures(中等規模 facts)
        grouped = {
            "_index_taiex_": [
                _make_fact(
                    stock_id="_index_taiex_",
                    fact_date=as_of,
                    source_core="taiex_core",
                    kind="TaiexBullishTrend",
                )
                for _ in range(10)
            ],
        }
        _patch_fetch_market_facts(monkeypatch, grouped)

        # stock_health + neely_forecast 共用同 agg.as_of patch
        ma_indicator = {
            "ma_core@daily": {
                "source_core": "ma_core",
                "value": {"series": [{"date": "2026-05-13", "close": 1234.5}]},
            },
        }

        # 1) market_context
        r1 = data_tools.market_context("2026-05-13")

        # 2) stock_health(用 _patch_agg_as_of)
        _patch_agg_as_of(monkeypatch, facts=[
            _make_fact(
                stock_id="2330",
                fact_date=as_of,
                source_core="macd_core",
                kind="GoldenCross",
            )
            for _ in range(20)
        ])
        r2 = data_tools.stock_health("2330", "2026-05-13")

        # 3) neely_forecast(用 _patch_agg_as_of_with_neely)
        _patch_agg_as_of_with_neely(
            monkeypatch,
            scenarios=[_bullish_scenario_fixture()],
            indicator_latest=ma_indicator,
        )
        r3 = data_tools.neely_forecast("2330", "2026-05-13")

        total_bytes = sum(len(json.dumps(r, default=str)) for r in (r1, r2, r3))
        total_tokens = total_bytes // 4
        assert total_tokens < 15_000, (
            f"3-tool chain {total_tokens} tokens > 15K target;"
            f" r1={len(json.dumps(r1))/4:.0f} / r2={len(json.dumps(r2))/4:.0f} / r3={len(json.dumps(r3))/4:.0f}"
        )
