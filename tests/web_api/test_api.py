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


def test_screens_top_n_over_limit_422():
    """top_n 超上界 → FastAPI Query(le=500) 驗證 422(防 LIMIT 百萬 DoS)。"""
    c = _client([])
    r = c.get("/screens/magic_formula?date=2026-05-28&top_n=999999")
    assert r.status_code == 422


def test_screens_offset_negative_422():
    c = _client([])
    r = c.get("/screens/magic_formula?date=2026-05-28&offset=-1")
    assert r.status_code == 422


def test_screens_valid_top_n_ok():
    """合法 top_n 仍正常(MAX(date) 回 None → 空結果 200)。"""
    c = _client([{"d": None}])
    r = c.get("/screens/magic_formula?date=2026-05-28&top_n=30")
    assert r.status_code == 200
    assert r.json()["rows"] == []


# ── Traditional Core(獨立 vertical)forest passthrough + /waves 邊緣組裝 ──────
def test_traditional_forest_passthrough_ok():
    c = _client([{"j": '{"scenario_forest": [], "stock_id": "3363"}'}])
    r = c.get("/stocks/3363/traditional/forest?timeframe=daily")
    assert r.status_code == 200
    assert r.headers["content-type"].startswith("application/json")
    assert r.json() == {"scenario_forest": [], "stock_id": "3363"}


def test_traditional_forest_404_when_missing():
    c = _client([])
    r = c.get("/stocks/3363/traditional/forest")
    assert r.status_code == 404


def test_waves_composes_neely_and_traditional():
    # FakeConn 對 neely + traditional 兩個 query 都回同一 canned row(key "j")
    c = _client([{"j": '{"k": 1}'}])
    r = c.get("/stocks/3363/waves?as_of=2026-05-28")
    assert r.status_code == 200
    assert r.json() == {"neely": {"k": 1}, "traditional": {"k": 1}}  # 並排,不合併


def test_waves_both_null_when_missing():
    c = _client([])
    r = c.get("/stocks/9999/waves?as_of=2026-05-28")
    assert r.status_code == 200
    assert r.json() == {"neely": None, "traditional": None}
