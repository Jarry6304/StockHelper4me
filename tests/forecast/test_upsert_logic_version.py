"""B1:upsert_forecast logic_version + idempotent guard contract tests.

對齊 b1-degree-consolidation skill 塊 4:
- INSERT 帶 logic_version 欄
- 預設 'b1' if row 沒給
- ON CONFLICT SET 子句含 CASE 對 resolved_date IS NULL 才覆寫 logic_version
- logic_version **不進**唯一鍵(維持 5-tuple)

無法接 live PG,以 mock cursor 攔 SQL 字串 + payload 做契約測試。
"""

from __future__ import annotations

import sys
from datetime import date
from pathlib import Path
from unittest.mock import MagicMock

_REPO_ROOT = Path(__file__).resolve().parent.parent.parent
_SRC_ROOT = _REPO_ROOT / "src"
for _p in (str(_SRC_ROOT), str(_REPO_ROOT)):
    if _p not in sys.path:
        sys.path.insert(0, _p)

from forecast._db import upsert_forecast  # noqa: E402


def _make_mock_conn():
    """Build mock conn whose .cursor() context manager yields a captured-SQL cursor."""
    cursor = MagicMock()
    cursor.execute = MagicMock()
    cursor.__enter__ = MagicMock(return_value=cursor)
    cursor.__exit__ = MagicMock(return_value=None)

    conn = MagicMock()
    conn.cursor = MagicMock(return_value=cursor)
    return conn, cursor


def _base_row() -> dict:
    return {
        "stock_id": "2330",
        "forecast_date": date(2026, 5, 26),
        "horizon_days": 63,
        "confidence": 0.80,
        "source_core": "baseline",
        "lower": 100.0,
        "upper": 110.0,
        "point": 105.0,
    }


class TestLogicVersionField:
    def test_default_logic_version_is_b1(self):
        """row 沒給 logic_version → payload 帶 'b1'。"""
        conn, cursor = _make_mock_conn()
        upsert_forecast(conn, _base_row())
        # cursor.execute called once with (sql, payload)
        cursor.execute.assert_called_once()
        sql, payload = cursor.execute.call_args[0]
        assert payload["logic_version"] == "b1"

    def test_explicit_logic_version_passed_through(self):
        """row 給 logic_version='pre_b1' → payload 保留。"""
        conn, cursor = _make_mock_conn()
        row = _base_row()
        row["logic_version"] = "pre_b1"
        upsert_forecast(conn, row)
        _sql, payload = cursor.execute.call_args[0]
        assert payload["logic_version"] == "pre_b1"

    def test_custom_logic_version_passed_through(self):
        """支援 future logic version tag(e.g. 'b2' / 'experimental')。"""
        conn, cursor = _make_mock_conn()
        row = _base_row()
        row["logic_version"] = "b2_experimental"
        upsert_forecast(conn, row)
        _sql, payload = cursor.execute.call_args[0]
        assert payload["logic_version"] == "b2_experimental"


class TestSqlContract:
    def test_insert_column_list_includes_logic_version(self):
        conn, cursor = _make_mock_conn()
        upsert_forecast(conn, _base_row())
        sql, _payload = cursor.execute.call_args[0]
        # INSERT 段必須含 logic_version
        assert "logic_version" in sql

    def test_insert_values_includes_logic_version_placeholder(self):
        conn, cursor = _make_mock_conn()
        upsert_forecast(conn, _base_row())
        sql, _payload = cursor.execute.call_args[0]
        # %(logic_version)s 必須出現在 VALUES 段
        assert "%(logic_version)s" in sql

    def test_on_conflict_key_unchanged_5_tuple(self):
        """logic_version 不進 ON CONFLICT 唯一鍵 — 維持既有 5-tuple。"""
        conn, cursor = _make_mock_conn()
        upsert_forecast(conn, _base_row())
        sql, _payload = cursor.execute.call_args[0]
        # 既有 5-tuple 應該完整存在
        assert "ON CONFLICT (stock_id, forecast_date, horizon_days, source_core, confidence)" in sql
        # logic_version 不應出現在 ON CONFLICT 子句的 5-tuple 內
        # 取 ON CONFLICT 行起算的第一個 "DO UPDATE" 之前的片段
        on_conflict_idx = sql.index("ON CONFLICT")
        do_update_idx = sql.index("DO UPDATE", on_conflict_idx)
        on_conflict_clause = sql[on_conflict_idx:do_update_idx]
        assert "logic_version" not in on_conflict_clause

    def test_set_clause_has_case_guard_for_resolved_date(self):
        """SET clause 對 logic_version 必須是 CASE WHEN resolved_date IS NULL pattern。"""
        conn, cursor = _make_mock_conn()
        upsert_forecast(conn, _base_row())
        sql, _payload = cursor.execute.call_args[0]
        # SET 子句必須含 CASE 對 resolved_date IS NULL 做 guard
        set_start = sql.index("DO UPDATE SET")
        set_clause = sql[set_start:]
        # logic_version 出現在 SET 子句
        assert "logic_version" in set_clause
        # CASE / WHEN / resolved_date IS NULL 三要素都在
        assert "CASE" in set_clause
        assert "WHEN" in set_clause
        assert "resolved_date IS NULL" in set_clause
        # EXCLUDED.logic_version 作為新值
        assert "EXCLUDED.logic_version" in set_clause
        # forecast_log.logic_version 作為已 settle row 保留值
        assert "forecast_log.logic_version" in set_clause

    def test_existing_set_fields_still_present(self):
        """既有 SET fields(lower / upper / point / calibrated / internal_only /
        regime_tag / params_hash)未被本 PR 移除。"""
        conn, cursor = _make_mock_conn()
        upsert_forecast(conn, _base_row())
        sql, _payload = cursor.execute.call_args[0]
        set_clause = sql[sql.index("DO UPDATE SET"):]
        for field in ("lower", "upper", "point", "calibrated", "internal_only",
                      "regime_tag", "params_hash"):
            assert f"{field}" in set_clause, f"既有 SET field {field} 不見"
            assert f"EXCLUDED.{field}" in set_clause, f"既有 EXCLUDED.{field} 不見"


class TestPayloadShape:
    def test_required_fields_in_payload(self):
        conn, cursor = _make_mock_conn()
        upsert_forecast(conn, _base_row())
        _sql, payload = cursor.execute.call_args[0]
        for field in ("stock_id", "forecast_date", "horizon_days",
                      "confidence", "source_core", "logic_version"):
            assert field in payload

    def test_internal_only_default_false(self):
        """既有 default 行為(internal_only=False)未變。"""
        conn, cursor = _make_mock_conn()
        upsert_forecast(conn, _base_row())
        _sql, payload = cursor.execute.call_args[0]
        assert payload["internal_only"] is False

    def test_calibrated_default_false(self):
        conn, cursor = _make_mock_conn()
        upsert_forecast(conn, _base_row())
        _sql, payload = cursor.execute.call_args[0]
        assert payload["calibrated"] is False
