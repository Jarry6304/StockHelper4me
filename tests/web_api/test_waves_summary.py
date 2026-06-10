"""GET /waves/summary 端點(FastAPI TestClient + fake sync conn,不打真 DB)。

對齊 test_api.py 慣例:dependency_overrides 注入 fake conn;cursor 依 SQL 參數
core_name 分流 canned rows(本端點 2 條 batch SQL)。
"""

from __future__ import annotations

from datetime import date

from fastapi.testclient import TestClient

from web_api.app import create_app
from web_api.pool import db_conn

AS_OF = date(2026, 6, 11)


class _DispatchCursor:
    def __init__(self, by_core):
        self._by_core = by_core
        self._rows = []
        self.executed = []

    def __enter__(self):
        return self

    def __exit__(self, *a):
        return False

    def execute(self, sql, params=None):
        self.executed.append((sql, params))
        self._rows = self._by_core.get(params[1], [])

    def fetchall(self):
        return list(self._rows)

    def fetchone(self):
        return self._rows[0] if self._rows else None


class DispatchConn:
    def __init__(self, by_core):
        self._by_core = by_core

    def cursor(self):
        return _DispatchCursor(self._by_core)


def _client(by_core) -> TestClient:
    app = create_app()
    app.dependency_overrides[db_conn] = lambda: DispatchConn(by_core)
    return TestClient(app)


def _neely_row(stock_id: str) -> dict:
    return {
        "stock_id": stock_id,
        "snapshot_date": AS_OF,
        "snapshot": {
            "scenario_forest": [{
                "structure_label": "Impulse·W3",
                "pattern_type": "Impulse",
                "power_rating": "Bullish",
                "rules_passed_count": 9,
                "wave_tree": {"end": "2026-06-01"},
                "invalidation_triggers": [],
                "monowave_structure_labels": [{"labels": [{"certainty": "Primary"}]}],
            }],
            "monowave_series": [{"end_price": 100.0}, {"end_price": 120.0}],
            "insufficient_data": False,
        },
    }


def _reso_row(stock_id: str) -> dict:
    return {
        "stock_id": stock_id,
        "snapshot_date": AS_OF,
        "snapshot": {"findings": [{"level": "strong"}], "single_track_mode": False},
    }


class TestWavesSummary:
    def test_200_two_stocks_one_missing(self):
        client = _client({
            "neely_core": [_neely_row("2330")],
            "resonance_fusion": [_reso_row("2330")],
        })
        r = client.get("/waves/summary?stock_ids=2330,9999&date=2026-06-11")
        assert r.status_code == 200
        body = r.json()
        assert body["as_of"] == "2026-06-11" and body["timeframe"] == "daily"
        rows = {row["stock_id"]: row for row in body["rows"]}
        assert rows["2330"]["insufficient"] is False
        assert rows["2330"]["label"] == "Impulse"  # 緊湊標籤(自 pattern_type 組)
        assert rows["2330"]["direction"] == "up"
        assert rows["2330"]["certainty"] == "Primary"
        assert rows["2330"]["resonance"] == "strong"
        assert rows["2330"]["sparkline"] == [0.0, 1.0]
        assert rows["9999"]["insufficient"] is True
        # 輸入順序 = 輸出順序
        assert [row["stock_id"] for row in body["rows"]] == ["2330", "9999"]

    def test_contract_keys_match_wave_summary_row(self):
        """回傳 row 鍵集合與 contracts.WaveSummaryRow 一致(codegen drift 防線)。"""
        from web_api.contracts import WaveSummaryRow

        client = _client({"neely_core": [_neely_row("2330")], "resonance_fusion": []})
        r = client.get("/waves/summary?stock_ids=2330&date=2026-06-11")
        assert set(r.json()["rows"][0]) == set(WaveSummaryRow.model_fields)

    def test_empty_stock_ids_422(self):
        client = _client({})
        assert client.get("/waves/summary?stock_ids=,,&date=2026-06-11").status_code == 422
        # 缺參數 → FastAPI 驗證 422
        assert client.get("/waves/summary?date=2026-06-11").status_code == 422

    def test_over_limit_422(self):
        ids = ",".join(str(1000 + i) for i in range(101))
        client = _client({})
        r = client.get(f"/waves/summary?stock_ids={ids}&date=2026-06-11")
        assert r.status_code == 422
        assert "上限" in r.json()["detail"]

    def test_bad_timeframe_422(self):
        client = _client({})
        r = client.get("/waves/summary?stock_ids=2330&date=2026-06-11&timeframe=hourly")
        assert r.status_code == 422

    def test_timeframe_passthrough(self):
        client = _client({"neely_core": [], "resonance_fusion": []})
        r = client.get("/waves/summary?stock_ids=2330&date=2026-06-11&timeframe=weekly")
        assert r.status_code == 200
        assert r.json()["timeframe"] == "weekly"
