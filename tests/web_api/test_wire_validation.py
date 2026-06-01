"""#6 wire validation:對每個 Pydantic-契約 endpoint,跑 TestClient → resp.json()
→ Model.model_validate(...) 驗證真實 wire shape 不漂移。

不只是 model.dict_literal 對齊(那是契約自洽 unit test),而是 router → JSONResponse →
jsonable_encoder → HTTP body → resp.json() 跑完整鏈,catch:
- 任何 jsonable_encoder Decimal→float / date→ISO 漂移
- router 加 / 改欄沒同步契約
- 契約欄位 strict 要求但 router 可能漏帶

對 Rust ts-rs 來源(kalman / neely)走 tsc strict 編譯;runtime JSON Schema 驗證走獨立
sprint(spec §6 acceptance)。
"""

from __future__ import annotations

from datetime import date

from fastapi.testclient import TestClient

from web_api.app import create_app
from web_api.pool import db_conn

# 重用 test_api.py 的 fake 連線(避免 dup)
from tests.web_api.test_api import FakeConn, _MultiRoundFakeConn  # noqa: E402


def _client(rows):
    app = create_app()
    app.dependency_overrides[db_conn] = lambda: FakeConn(rows)
    return TestClient(app)


def _client_multi(row_sets):
    app = create_app()
    app.dependency_overrides[db_conn] = lambda: _MultiRoundFakeConn(row_sets)
    return TestClient(app)


# ── PriceSeries(/stocks/{id}/ohlc)─────────────────────────────────────────
def test_wire_ohlc_validates_as_PriceSeries():
    from web_api.contracts import PriceSeries

    c = _client([
        {"date": date(2026, 1, 2), "open": 100.0, "high": 102.0,
         "low": 99.0, "close": 101.0, "volume": 12345},
        {"date": date(2026, 1, 3), "open": 101.0, "high": 103.0,
         "low": 100.0, "close": 102.5, "volume": 23456},
    ])
    r = c.get("/stocks/2330/ohlc?from=2026-01-01&to=2026-05-28")
    assert r.status_code == 200
    m = PriceSeries.model_validate(r.json())
    assert m.stock_id == "2330"
    assert len(m.rows) == 2
    # jsonable_encoder 把 date 轉 ISO 字串
    assert m.rows[0].date == "2026-01-02"


def test_wire_ohlc_decimal_floats_through_jsonable():
    """schema 為 NUMERIC,row 可能拿到 Decimal,jsonable_encoder 應正確轉 float。"""
    from decimal import Decimal

    from web_api.contracts import PriceSeries

    c = _client([
        {"date": date(2026, 1, 2),
         "open": Decimal("100.50"), "high": Decimal("102.75"),
         "low": Decimal("99.25"), "close": Decimal("101.00"),
         "volume": 12345},
    ])
    r = c.get("/stocks/2330/ohlc?from=2026-01-01&to=2026-05-28")
    m = PriceSeries.model_validate(r.json())
    # Decimal("100.50") → "100.5" 字串 → pydantic float-coerce → 100.5
    assert m.rows[0].open == 100.5
    assert m.rows[0].close == 101.0


# ── ScreenResponse(/screens/f_score)──────────────────────────────────────
def test_wire_screens_validates_as_ScreenResponse():
    from web_api.contracts import ScreenResponse

    ranking_date = date(2026, 5, 28)
    raw_row = {
        "market": "TW", "stock_id": "2330", "date": ranking_date,
        "f_score": 8, "profitability": 4, "leverage": 2, "efficiency": 2,
        "score_rank": 1, "universe_size": 1200,
        "is_top_n": True, "excluded_reason": None,
        # 應被 denylist 砍
        "detail": {"raw": "should_not_leak"}, "is_dirty": False, "dirty_at": None,
        # LEFT JOIN
        "stock_name": "台積電", "industry_category": "半導體業",
    }
    c = _client_multi([[{"d": ranking_date}], [raw_row]])
    r = c.get("/screens/f_score?date=2026-05-28&top_n=10")
    assert r.status_code == 200
    m = ScreenResponse.model_validate(r.json())
    assert m.toolkit == "f_score"
    assert m.ranking_date == "2026-05-28"
    assert len(m.rows) == 1
    assert m.rows[0].rank == 1


def test_wire_screens_row_passes_per_toolkit_subtype():
    """ScreenResponse.rows 用 base 型;對 f_score 可進一步 narrow 到 ScreenRowFScore。"""
    from web_api.contracts import ScreenRowFScore

    ranking_date = date(2026, 5, 28)
    raw_row = {
        "market": "TW", "stock_id": "2330", "date": ranking_date,
        "f_score": 8, "profitability": 4, "leverage": 2, "efficiency": 2,
        "score_rank": 1, "universe_size": 1200, "is_top_n": True,
        "excluded_reason": None,
        "stock_name": "台積電", "industry_category": "半導體業",
    }
    c = _client_multi([[{"d": ranking_date}], [raw_row]])
    r = c.get("/screens/f_score?date=2026-05-28&top_n=10")
    # 第一筆 row 應可被 ScreenRowFScore 接住(含所有 4 個 f_score metric 欄)
    row = r.json()["rows"][0]
    m = ScreenRowFScore.model_validate(row)
    assert m.f_score == 8 and m.profitability == 4


# ── StockRef(/stocks?q=)──────────────────────────────────────────────────
def test_wire_search_validates_as_StockRef_list():
    from web_api.contracts import StockRef

    c = _client([
        {"stock_id": "2330", "stock_name": "台積電", "industry_category": "半導體業"},
        {"stock_id": "2317", "stock_name": "鴻海",   "industry_category": "其他電子業"},
    ])
    r = c.get("/stocks?q=23")
    body = r.json()
    assert isinstance(body, list)
    refs = [StockRef.model_validate(d) for d in body]
    assert refs[0].stock_id == "2330"
    assert refs[1].stock_name == "鴻海"


# ── LevelsFusion(/stocks/{id}/levels)─────────────────────────────────────
def test_wire_levels_validates_as_LevelsFusion():
    from web_api.contracts import LevelsFusion

    doc = {
        "stock_id": "2330", "as_of": "2026-05-28",
        "source_point_count": 4, "level_count_total": 1, "level_count": 1,
        "levels": [{"price": 100.0, "low": 99.0, "high": 101.0,
                    "sources": ["sr_support"], "strength": 2, "member_count": 2}],
    }
    import json

    c = _client([{"j": json.dumps(doc)}])
    r = c.get("/stocks/2330/levels?as_of=2026-05-28")
    assert r.status_code == 200
    m = LevelsFusion.model_validate(r.json())
    assert m.levels[0].price == 100.0


# ── ResonanceFusion(/stocks/{id}/resonance)───────────────────────────────
def test_wire_resonance_validates_as_ResonanceFusion():
    from web_api.contracts import ResonanceFusion

    doc = {
        "stock_id": "2330", "as_of": "2026-05-28",
        "track1": {
            "stock_id": "2330", "as_of": "2026-05-28", "snapshot_date": "2026-05-27",
            "has_snapshot": True, "pattern_type": "Impulse", "power_rating": "Bullish",
            "direction": "bullish", "effective_degree": "Minor", "wave_count": 5,
            "fib_lines": [], "invalidation_price": 80.0, "invalidated": False,
            "fallback_to_flat_union": False, "notes": [],
        },
        "track2": {
            "stock_id": "2330", "as_of": "2026-05-28", "current_price": 105.0,
            "primary_horizon": 63, "primary_confidence": 0.80, "primary_band": None,
            "horizons": {}, "notes": [],
        },
        "is_top_30": False, "is_top_30_source": None, "is_top_30_date": None,
        "findings": [], "single_track_mode": False, "notes": [],
    }
    import json

    c = _client([{"j": json.dumps(doc)}])
    r = c.get("/stocks/2330/resonance?as_of=2026-05-28&timeframe=daily")
    assert r.status_code == 200
    m = ResonanceFusion.model_validate(r.json())
    assert m.track1.wave_count == 5
    assert m.single_track_mode is False


# ── ClimateFusion(/market/climate)────────────────────────────────────────
def test_wire_climate_validates_as_ClimateFusion():
    from web_api.contracts import ClimateFusion

    doc = {
        "as_of": "2026-05-28", "overall_climate": "bullish", "climate_score": 12.3,
        "components": {"taiex": {"score": 10, "fact_count": 5}},
        "systemic_risks": [], "narrative": "偏多",
    }
    import json

    c = _client([{"j": json.dumps(doc)}])
    r = c.get("/market/climate?as_of=2026-05-28")
    assert r.status_code == 200
    m = ClimateFusion.model_validate(r.json())
    assert m.overall_climate == "bullish"
