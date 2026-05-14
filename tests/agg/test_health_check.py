"""health_check() 行為測試 — 走 mock connection 不打真 PG。"""

from __future__ import annotations

from typing import Any
from unittest.mock import MagicMock

from agg.query import health_check


class _FakeCursor:
    """簡單 mock,對應 .execute() / .fetchone() / context manager。"""

    def __init__(self, responses: list[dict[str, Any]]):
        self._responses = list(responses)

    def __enter__(self):
        return self

    def __exit__(self, *exc):
        return False

    def execute(self, sql: str, params=None):
        self._last_sql = sql

    def fetchone(self):
        return self._responses.pop(0) if self._responses else None


def _make_conn(cursor_responses: list[dict[str, Any]]) -> Any:
    conn = MagicMock()
    conn.cursor.return_value = _FakeCursor(cursor_responses)
    conn.info.host = "localhost"
    conn.info.port = 5432
    conn.info.dbname = "twstock"
    return conn


class TestHealthCheck:
    def test_all_tables_exist(self):
        # 對每張表回:1) exists_flag=True 2) count
        responses = [
            {"exists_flag": True},  {"n": 100},   # facts
            {"exists_flag": True},  {"n": 50},    # indicator_values
            {"exists_flag": True},  {"n": 25},    # structural_snapshots
        ]
        conn = _make_conn(responses)
        result = health_check(conn=conn)
        assert result["ok"] is True
        assert result["tables"]["facts"]["exists"] is True
        assert result["tables"]["facts"]["row_count"] == 100
        assert result["tables"]["indicator_values"]["row_count"] == 50
        assert result["tables"]["structural_snapshots"]["row_count"] == 25
        assert result["errors"] == []
        assert "localhost:5432/twstock" in result["database_url"]

    def test_missing_table_marks_not_ok(self):
        # facts 存在,indicator_values 缺,structural_snapshots 存在
        responses = [
            {"exists_flag": True},  {"n": 100},   # facts
            {"exists_flag": False},               # indicator_values 缺 → 不再撈 count
            {"exists_flag": True},  {"n": 25},    # structural_snapshots
        ]
        conn = _make_conn(responses)
        result = health_check(conn=conn)
        assert result["ok"] is False
        assert result["tables"]["indicator_values"]["exists"] is False
        assert result["tables"]["indicator_values"]["row_count"] is None

    def test_connect_failure(self, monkeypatch):
        # 模擬 get_connection 在 health_check 內 raise
        from agg import query as q

        def _raise(*a, **kw):
            raise RuntimeError("PG down")

        monkeypatch.setattr(q, "get_connection", _raise)
        result = health_check()  # no conn injected → walks get_connection path
        assert result["ok"] is False
        assert any("connect_failed" in e for e in result["errors"])
