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
