"""v4.32 Golden L3 — pydantic 契約 vs 真實 .to_dict() wire shape 對齊測試。

確保 web_api/contracts.py 的 model 鏡射真實序列化形狀(drift → validation 失敗)。
用真實 dataclass 的 .to_dict() 餵 pydantic model_validate,接住欄位 / 型別漂移。
"""

from datetime import date

from web_api.contracts import ClimateFusion, LevelsFusion, ResonanceFusion


def test_levels_contract_matches_key_levels_output():
    # 對齊 src/fusion/key_levels.py::key_levels() 回傳 dict
    doc = {
        "stock_id": "2330", "as_of": "2026-05-28",
        "source_point_count": 4, "level_count_total": 2, "level_count": 2,
        "levels": [
            {"price": 100.0, "low": 99.0, "high": 101.0,
             "sources": ["sr_support", "neely_fib_daily"], "strength": 2, "member_count": 2},
        ],
    }
    m = LevelsFusion.model_validate(doc)
    assert m.stock_id == "2330" and m.levels[0].strength == 2


def test_resonance_contract_matches_dualtrackresult_to_dict():
    # 用真實 dataclass 建 DualTrackResult → .to_dict() → 契約驗證(欄位 / 型別漂移會被抓)
    from fusion.dual_track._shared import (
        DualTrackResult, FibLine, FibLineResonance, Track1View, Track2Band, Track2View,
    )

    fib = FibLine(price=100.0, low=99.0, high=101.0, label="0.618", source_ratio=0.618)
    t1 = Track1View(
        stock_id="2330", as_of=date(2026, 5, 28), snapshot_date=date(2026, 5, 27),
        has_snapshot=True, pattern_type="Impulse", power_rating="Bullish",
        direction="bullish", effective_degree="Minor", wave_count=5, fib_lines=[fib],
        invalidation_price=80.0, invalidated=False, fallback_to_flat_union=False, notes=[],
    )
    band = Track2Band(horizon_days=63, confidence=0.80, lower=95.0, upper=120.0,
                      point=107.0, source_core="fusion", width_ratio=0.23, is_overly_wide=False)
    t2 = Track2View(
        stock_id="2330", as_of=date(2026, 5, 28), current_price=105.0,
        primary_horizon=63, primary_confidence=0.80, primary_band=band,
        horizons={63: band}, notes=[],
    )
    finding = FibLineResonance(
        fib_line=fib, level="basic", band_covers=True, median_close=False,
        cross_stock_boost=False, t1_horizon=63, t2_profile={63: "basic"}, notes=[],
    )
    res = DualTrackResult(
        stock_id="2330", as_of=date(2026, 5, 28), track1=t1, track2=t2,
        is_top_30=True, is_top_30_source="magic_formula_ranked_derived",
        is_top_30_date=date(2026, 5, 27), findings=[finding],
        single_track_mode=False, notes=[],
    )
    # 真實 wire shape:.to_dict() 經 JSON/jsonb 序列化(int key → str key)才是 API/MCP 回傳形狀
    import json
    wire = json.loads(json.dumps(res.to_dict()))
    m = ResonanceFusion.model_validate(wire)
    assert m.track1.wave_count == 5
    # horizons / t2_profile JSON 鍵為字串(契約 dict[str, ...])
    assert "63" in m.track2.horizons
    assert m.findings[0].t2_profile["63"] == "basic"
    assert m.is_top_30_date == "2026-05-27"


def test_climate_contract_matches_market_context_shape():
    # 對齊 mcp_server/_climate.py::compute_market_context() 回傳 dict(7 env + risk_alert)
    doc = {
        "as_of": "2026-05-28", "overall_climate": "bullish", "climate_score": 12.3,
        "components": {
            "taiex": {"score": 10, "fact_count": 5},
            "risk_alert": {"score": -15, "active_disposition_stocks": 3,
                           "escalations_60d": 1, "announced_14d": 2},
        },
        "systemic_risks": ["tw_disposition_cluster"],
        "narrative": "偏多",
    }
    m = ClimateFusion.model_validate(doc)
    assert m.components["taiex"].fact_count == 5
    assert m.components["risk_alert"].active_disposition_stocks == 3


# ── screens 契約對齊 #1 後的真實 wire shape ─────────────────────────────────
def test_screen_response_base_matches_wire():
    """#2:對齊 #1 後 /screens/{toolkit} JSON 形狀(rank 正規化 + denylist)。"""
    from web_api.contracts import ScreenResponse

    doc = {
        "toolkit": "f_score",
        "ranking_date": "2026-05-28",
        "top_n": 30,
        "offset": 0,
        "rows": [
            {
                "stock_id": "2330", "market": "TW", "date": "2026-05-28",
                "stock_name": "台積電", "industry_category": "半導體業",
                "universe_size": 1200, "excluded_reason": None,
                "rank": 1, "is_top_n": True,
            },
        ],
    }
    m = ScreenResponse.model_validate(doc)
    assert m.toolkit == "f_score" and m.ranking_date == "2026-05-28"
    assert m.rows[0].rank == 1 and m.rows[0].is_top_n is True


def test_screen_row_f_score_metric_extension():
    """f_score 4 個 metric 欄(全 int):f_score / profitability / leverage / efficiency。"""
    from web_api.contracts import ScreenRowFScore

    doc = {
        "stock_id": "2330", "market": "TW", "date": "2026-05-28",
        "is_top_n": True, "rank": 1,
        "f_score": 8, "profitability": 4, "leverage": 2, "efficiency": 2,
    }
    m = ScreenRowFScore.model_validate(doc)
    assert m.f_score == 8 and m.profitability == 4
    # base 欄(default None)亦正確
    assert m.stock_name is None and m.universe_size is None


def test_screen_row_magic_formula_metric_extension():
    """magic_formula 10 個 metric 欄(8 float + 2 int rank)。"""
    from web_api.contracts import ScreenRowMagicFormula

    doc = {
        "stock_id": "2330", "market": "TW", "date": "2026-05-28",
        "is_top_n": True, "rank": 1,
        "ebit_ttm": 1.234e12, "market_cap": 1.5e13,
        "total_debt": 5.0e11, "cash": 2.0e12,
        "enterprise_value": 1.4e13, "invested_capital": 8.0e12,
        "earnings_yield": 0.088, "roic": 0.154,
        "ey_rank": 10, "roic_rank": 5,
    }
    m = ScreenRowMagicFormula.model_validate(doc)
    assert m.earnings_yield == 0.088 and m.ey_rank == 10


def test_price_series_contract_matches_ohlc_wire_shape():
    """#4:對齊 /stocks/{id}/ohlc 回傳 dict 形狀(date ISO str / Decimal→float / BIGINT→int)。"""
    from web_api.contracts import PriceSeries

    # 對應 series.py:ohlc() 的回傳 — jsonable_encoder 後形狀
    doc = {
        "stock_id": "2330",
        "rows": [
            {"date": "2026-01-02", "open": 100.0, "high": 102.0,
             "low": 99.0, "close": 101.0, "volume": 12345},
            {"date": "2026-01-03", "open": 101.0, "high": 103.0,
             "low": 100.0, "close": 102.5, "volume": 23456},
        ],
    }
    m = PriceSeries.model_validate(doc)
    assert m.stock_id == "2330" and len(m.rows) == 2
    assert m.rows[0].date == "2026-01-02"
    assert m.rows[0].close == 101.0
    assert m.rows[0].volume == 12345


def test_price_bar_all_nullable_metrics():
    """OHLCV 各欄皆 nullable(對齊 schema 允許 NULL — 停牌等場景)。"""
    from web_api.contracts import PriceBar

    m = PriceBar.model_validate({"date": "2026-01-02"})
    assert m.date == "2026-01-02"
    assert m.open is None and m.close is None and m.volume is None


def test_screen_row_subclasses_have_required_extras():
    """10 subclass 各帶正確 metric 欄(spec table 對齊;regression-lock)。"""
    from web_api import contracts as C

    expected_extras: dict[type, set[str]] = {
        C.ScreenRowMagicFormula: {
            "ebit_ttm", "market_cap", "total_debt", "cash",
            "enterprise_value", "invested_capital",
            "earnings_yield", "roic", "ey_rank", "roic_rank",
        },
        C.ScreenRowPersistentMomentum: {
            "return_6m", "return_12m_1m", "persistent_months",
        },
        C.ScreenRowRevenueMomentum: {"revenue_yoy_latest", "consecutive_positive"},
        C.ScreenRowInstitutionalConcert: {
            "concert_days", "foreign_cumulative_20d",
            "shares_outstanding", "cumulative_pct",
        },
        C.ScreenRowFScore: {"f_score", "profitability", "leverage", "efficiency"},
        C.ScreenRowLowVolatility: {"std_252d"},
        C.ScreenRowIndustryAdjGp: {
            "gross_profitability", "industry",
            "industry_median_gp", "industry_adj_gp",
        },
        C.ScreenRowLongTermLowVol: {"std_36m"},
        C.ScreenRowDividendYield: {
            "dividend_yield_pct", "return_12m_pct", "payout_years_5y",
        },
        C.ScreenRowMom12_1: {"return_12m_1m"},
    }
    base_fields = set(C.ScreenRowBase.model_fields.keys())
    for cls, extras in expected_extras.items():
        cls_fields = set(cls.model_fields.keys())
        diff = cls_fields - base_fields
        assert diff == extras, f"{cls.__name__} extras mismatch: got {diff}, want {extras}"
