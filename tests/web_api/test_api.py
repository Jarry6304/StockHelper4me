"""v4.32 Golden L3 唯讀 Web API 測試(FastAPI TestClient + fake sync conn)。

不依賴真實 PG:dependency_overrides 注入 FakeConn(每請求 sync conn),sync cursor 回
canned rows。handler 為 sync(FastAPI threadpool)+ 每請求 sync conn → 不碰 event loop
(Windows ProactorEventLoop / Python 3.14 安全)。
"""

from datetime import date

from fastapi.testclient import TestClient

from web_api.app import create_app
from web_api.pool import db_conn


# ── fake sync conn ──────────────────────────────────────────────────────────
class _FakeCursor:
    def __init__(self, rows):
        self._rows = rows
        self.executed = []

    def __enter__(self):
        return self

    def __exit__(self, *a):
        return False

    def execute(self, sql, params=None):
        self.executed.append((sql, params))

    def fetchone(self):
        return self._rows[0] if self._rows else None

    def fetchall(self):
        return list(self._rows)


class FakeConn:
    def __init__(self, rows):
        self._rows = rows

    def cursor(self):
        return _FakeCursor(self._rows)


def _client(rows):
    app = create_app()
    app.dependency_overrides[db_conn] = lambda: FakeConn(rows)
    return TestClient(app)


# ── meta ────────────────────────────────────────────────────────────────────
def test_health():
    c = _client([])
    r = c.get("/health")
    assert r.status_code == 200 and r.json()["status"] == "ok"


# ── neely forest passthrough + 保險絲 ──────────────────────────────────────
def test_neely_forest_passthrough_ok():
    c = _client([{"n": 5, "j": '{"scenario_forest": [], "stock_id": "2330"}'}])
    r = c.get("/stocks/2330/neely/forest?as_of=2026-05-28")
    assert r.status_code == 200
    assert r.headers["content-type"].startswith("application/json")
    assert r.json() == {"scenario_forest": [], "stock_id": "2330"}


def test_neely_forest_422_overflow():
    c = _client([{"n": 300, "j": "{}"}])
    r = c.get("/stocks/2330/neely/forest?as_of=2026-05-28")
    assert r.status_code == 422
    assert "forest_overflow" in r.json()["detail"]


def test_neely_forest_404_when_missing():
    c = _client([])  # 無 row → forest_len None(放行)→ snapshot None → 404
    r = c.get("/stocks/2330/neely/forest?as_of=2026-05-28")
    assert r.status_code == 404


# ── levels / resonance / climate / generic snapshot ─────────────────────────
def test_levels():
    c = _client([{"j": '{"levels": [], "level_count": 0}'}])
    r = c.get("/stocks/2330/levels?as_of=2026-05-28")
    assert r.status_code == 200 and r.json()["level_count"] == 0


def test_resonance():
    c = _client([{"j": '{"single_track_mode": false, "findings": []}'}])
    r = c.get("/stocks/2330/resonance?as_of=2026-05-28&timeframe=daily")
    assert r.status_code == 200 and r.json()["single_track_mode"] is False


def test_market_climate():
    c = _client([{"j": '{"overall_climate": "bullish"}'}])
    r = c.get("/market/climate?as_of=2026-05-28")
    assert r.status_code == 200 and r.json()["overall_climate"] == "bullish"


def test_generic_snapshot():
    c = _client([{"j": '{"trendlines": []}'}])
    r = c.get("/stocks/2330/snapshot/trendline_core?as_of=2026-05-28")
    assert r.status_code == 200 and r.json() == {"trendlines": []}


def test_generic_snapshot_404():
    c = _client([])
    r = c.get("/stocks/2330/snapshot/support_resistance_core?as_of=2026-05-28")
    assert r.status_code == 404


# ── ohlc 切片(jsonable_encoder 處理 date/Decimal)──────────────────────────
def test_ohlc():
    c = _client([
        {"date": date(2026, 1, 2), "open": 100.0, "high": 102.0,
         "low": 99.0, "close": 101.0, "volume": 12345},
    ])
    r = c.get("/stocks/2330/ohlc?from=2026-01-01&to=2026-05-28")
    assert r.status_code == 200
    body = r.json()
    assert body["stock_id"] == "2330"
    assert body["rows"][0]["date"] == "2026-01-02"
    assert body["rows"][0]["close"] == 101.0


# ── screens 白名單 ───────────────────────────────────────────────────────────
def test_screens_unknown_toolkit_404():
    c = _client([])
    r = c.get("/screens/bogus?date=2026-05-28")
    assert r.status_code == 404
    assert "unknown screen toolkit" in r.json()["detail"]


# ── screens 多查詢 fake conn(date lookup + 排名 rows 分兩輪)──────────────
class _MultiRoundFakeConn:
    """每次 conn.cursor() 取 row-set 序列下一筆;對 fetch_cross_stock_ranked
    2 個 query(MAX(date) → top rows)準確切片。
    """

    def __init__(self, row_sets: list[list[dict]]):
        self._sets = list(row_sets)

    def cursor(self):
        rows = self._sets.pop(0) if self._sets else []
        return _FakeCursor(rows)


def _client_multi(row_sets):
    app = create_app()
    app.dependency_overrides[db_conn] = lambda: _MultiRoundFakeConn(row_sets)
    return TestClient(app)


def test_screens_f_score_happy_path():
    """v4.x #1:非-magic_formula toolkit happy-path(原 500 → fixed)。
    驗:rank 正規化(score_rank → rank)+ denylist 砍三欄 + ranking_date 傳遞。
    """
    ranking_date = date(2026, 5, 28)
    raw_row = {
        "market": "TW", "stock_id": "2330", "date": ranking_date,
        "f_score": 8, "profitability": 4, "leverage": 2, "efficiency": 2,
        "score_rank": 1, "universe_size": 1200,
        "is_top_n": True, "excluded_reason": None,
        # 應被 denylist 砍掉的三欄:
        "detail": {"raw": "should_not_leak"},
        "is_dirty": False, "dirty_at": None,
        # LEFT JOIN stock_info_ref:
        "stock_name": "台積電", "industry_category": "半導體業",
    }
    c = _client_multi([[{"d": ranking_date}], [raw_row]])
    r = c.get("/screens/f_score?date=2026-05-28&top_n=10")
    assert r.status_code == 200
    body = r.json()
    assert body["toolkit"] == "f_score"
    assert body["ranking_date"] == "2026-05-28"
    assert body["top_n"] == 10
    assert len(body["rows"]) == 1
    out = body["rows"][0]
    # rank 正規化(來自 score_rank)
    assert out["rank"] == 1
    assert out["score_rank"] == 1  # 原欄保留(spec:不破壞 backward compat)
    # 原有 metric 欄保留
    assert out["f_score"] == 8 and out["profitability"] == 4
    # LEFT JOIN 欄
    assert out["stock_name"] == "台積電"
    assert out["industry_category"] == "半導體業"
    # denylist 砍三欄
    assert "detail" not in out
    assert "is_dirty" not in out
    assert "dirty_at" not in out


def test_screens_empty_when_no_ranking_date():
    """無 ranking_date → ranking_date=None + rows=[](對齊 CardState.empty)。"""
    c = _client_multi([[{"d": None}]])  # MAX(date) 回 None
    r = c.get("/screens/persistent_momentum?date=2026-05-28")
    assert r.status_code == 200
    body = r.json()
    assert body["ranking_date"] is None
    assert body["rows"] == []


def test_screens_all_10_toolkits_dispatch_with_correct_rank_col():
    """v4.x #1 acceptance:10 toolkits 全不 500。SQL 中含 per-toolkit rank_col。"""
    from web_api.routers.screens import _ALLOWED

    # 對齊 spec:per-toolkit (table, rank_col)
    expected = {
        "magic_formula":         ("magic_formula_ranked_derived",         "combined_rank"),
        "persistent_momentum":   ("persistent_momentum_ranked_derived",   "momentum_rank"),
        "revenue_momentum":      ("revenue_momentum_ranked_derived",      "revenue_rank"),
        "institutional_concert": ("institutional_concert_ranked_derived", "concert_rank"),
        "f_score":               ("f_score_ranked_derived",               "score_rank"),
        "low_volatility":        ("low_volatility_ranked_derived",        "vol_rank"),
        "industry_adj_gp":       ("industry_adj_gp_ranked_derived",       "gp_rank"),
        "long_term_low_vol":     ("long_term_low_vol_ranked_derived",     "vol_rank"),
        "dividend_yield":        ("dividend_yield_ranked_derived",        "yield_rank"),
        "mom_12_1":              ("mom_12_1_ranked_derived",              "mom_rank"),
    }
    assert _ALLOWED == expected
    # 每個 toolkit 走 happy-path(empty ranking_date 即可,證明 dispatch + arg 傳遞無誤)
    for toolkit in expected:
        c = _client_multi([[{"d": None}]])
        r = c.get(f"/screens/{toolkit}?date=2026-05-28")
        assert r.status_code == 200, f"toolkit {toolkit} 500'd: {r.text}"
        assert r.json()["toolkit"] == toolkit
