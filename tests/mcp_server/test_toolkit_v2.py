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
    from agg import _market

    def fake(_conn, *, as_of, lookback_days, cores=None, apply_lookahead_filter=True):
        return grouped

    monkeypatch.setattr(_market, "fetch_market_facts", fake)


def _patch_get_connection(monkeypatch):
    """Mock agg._db.get_connection 回 MagicMock conn。"""
    from agg import _db

    monkeypatch.setattr(_db, "get_connection", lambda *a, **kw: MagicMock())


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

    def test_returns_6_components(self, monkeypatch):
        _patch_get_connection(monkeypatch)
        _patch_fetch_market_facts(monkeypatch, {})

        result = data_tools.market_context("2026-05-13")
        assert set(result["components"].keys()) == {
            "taiex",
            "us_market",
            "fear_greed",
            "business",
            "exchange_rate",
            "market_margin",
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


def _patch_agg_as_of(monkeypatch, facts: list[dict], indicator_latest: dict | None = None):
    """Mock agg.as_of() 回固定 AsOfSnapshot。"""
    from agg import _types
    import agg

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
):
    """Mock agg.as_of() 回包含 neely structural snapshot 的 AsOfSnapshot。"""
    from agg import _types
    import agg

    def fake_as_of(stock_id, as_of, **kwargs):
        # Wrap scenarios in structural snapshot
        structural = {}
        if scenarios:
            structural["neely_core@daily"] = _types.StructuralRow(
                stock_id=stock_id,
                snapshot_date=as_of,
                timeframe="daily",
                core_name="neely_core",
                source_version="1.0.1",
                snapshot={"scenario_forest": scenarios},
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
        }

    def test_returns_4_timeframes(self, monkeypatch):
        _patch_agg_as_of_with_neely(monkeypatch, scenarios=[_bullish_scenario_fixture()])

        result = data_tools.neely_forecast("2330", "2026-05-13")
        assert set(result["forecasts"].keys()) == {
            "1_month", "1_quarter", "6_month", "1_year",
        }

    def test_prob_up_in_range(self, monkeypatch):
        _patch_agg_as_of_with_neely(monkeypatch, scenarios=[_bullish_scenario_fixture()])

        result = data_tools.neely_forecast("2330", "2026-05-13")
        for tf_key, fc in result["forecasts"].items():
            assert 0.10 <= fc["prob_up"] <= 0.90, f"{tf_key} prob_up={fc['prob_up']} out of range"

    def test_no_neely_data_returns_neutral_forecasts(self, monkeypatch):
        _patch_agg_as_of_with_neely(monkeypatch, scenarios=[])

        result = data_tools.neely_forecast("2330", "2026-05-13")
        for fc in result["forecasts"].values():
            assert fc["prob_up"] == 0.50


class TestNeelyForecastBullish:
    def test_bullish_scenario_lifts_prob(self, monkeypatch):
        """Bullish power_rating → 1_month prob_up > 0.50。"""
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
        assert result["forecasts"]["1_month"]["prob_up"] > 0.50

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
        """同一 bullish scenario,1_year prob_up 應比 1_month 更接近 0.50。"""
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
        prob_1m = result["forecasts"]["1_month"]["prob_up"]
        prob_1y = result["forecasts"]["1_year"]["prob_up"]
        # Bullish → 兩個都 > 0.50;但 1y 比 1m 接近 0.50 因為 base prob 才是 0.62 / 0.50
        assert prob_1m >= prob_1y


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
