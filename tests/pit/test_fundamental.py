"""Tests for src/pit/fundamental.py — Bronze report_date column reading.

Phase 2 alembic b8c9d0e1f2g3 adds `report_date` columns to 3 Bronze tables.
PIT layer reads them, falling back to fact_date + heuristic lag when NULL.

These tests verify the SQL contains COALESCE filter (so PIT respects both
real publish dates and heuristic fallback in a single query).
"""

from __future__ import annotations

import sys
from datetime import date
from pathlib import Path

import pytest

_REPO_ROOT = Path(__file__).resolve().parent.parent.parent
_SRC_ROOT = _REPO_ROOT / "src"
for p in (str(_SRC_ROOT), str(_REPO_ROOT)):
    if p not in sys.path:
        sys.path.insert(0, p)

from pit.fundamental import (  # noqa: E402
    asof_revenue,
    asof_financial,
    asof_business_indicator,
)


class _CaptureCursor:
    """Records the SQL + params of each execute() call, returns canned rows."""

    def __init__(self, calls_log: list, canned_rows: list[dict]):
        self._log = calls_log
        self._canned = canned_rows

    def __enter__(self):
        return self

    def __exit__(self, *exc):
        return False

    def execute(self, sql, params=None):
        self._log.append({"sql": sql, "params": params})
        return self

    def fetchall(self):
        return self._canned


class _CaptureConn:
    def __init__(self, canned_rows=None):
        self._canned = canned_rows or []
        self.calls: list[dict] = []

    def cursor(self):
        return _CaptureCursor(self.calls, self._canned)


def _row_with_report_date(d, **extra):
    return {"date": d, "report_date": extra.pop("report_date", None), **extra}


class TestAsofRevenue:
    def test_sql_uses_coalesce_fallback(self):
        conn = _CaptureConn()
        asof_revenue(conn, "2330", date(2024, 6, 1))
        sql = conn.calls[0]["sql"]
        assert "COALESCE(report_date, date + INTERVAL '11 days')" in sql
        assert "<= %s" in sql

    def test_sql_targets_monthly_revenue_table(self):
        conn = _CaptureConn()
        asof_revenue(conn, "2330", date(2024, 6, 1))
        assert "monthly_revenue" in conn.calls[0]["sql"]

    def test_returns_report_date_in_row(self):
        canned = [
            _row_with_report_date(date(2024, 5, 1), report_date=date(2024, 5, 10),
                                  revenue=1000),
            _row_with_report_date(date(2024, 4, 1), report_date=None, revenue=900),
        ]
        conn = _CaptureConn(canned)
        out = asof_revenue(conn, "2330", date(2024, 6, 1))
        assert len(out) == 2
        assert out[0]["report_date"] == date(2024, 5, 10)
        assert out[1]["report_date"] is None  # caller can detect heuristic fallback

    def test_params_include_market_stock_asof(self):
        conn = _CaptureConn()
        asof_revenue(conn, "2330", date(2024, 6, 1), market="TW")
        params = conn.calls[0]["params"]
        # (market, stock_id, earliest, asof_t)
        assert params[0] == "TW"
        assert params[1] == "2330"
        assert params[3] == date(2024, 6, 1)


class TestAsofFinancial:
    def test_sql_uses_45d_fallback(self):
        conn = _CaptureConn()
        asof_financial(conn, "2330", date(2024, 6, 1))
        sql = conn.calls[0]["sql"]
        assert "COALESCE(report_date, date + INTERVAL '45 days')" in sql

    def test_sql_targets_financial_statement(self):
        conn = _CaptureConn()
        asof_financial(conn, "2330", date(2024, 6, 1))
        assert "financial_statement" in conn.calls[0]["sql"]


class TestAsofBusinessIndicator:
    def test_sql_uses_27d_fallback(self):
        conn = _CaptureConn()
        asof_business_indicator(conn, date(2024, 6, 1))
        sql = conn.calls[0]["sql"]
        assert "COALESCE(report_date, date + INTERVAL '27 days')" in sql

    def test_sql_market_level_no_stock_id_filter(self):
        # business_indicator_tw is market-level (PK = market+date) — no stock_id
        conn = _CaptureConn()
        asof_business_indicator(conn, date(2024, 6, 1))
        sql = conn.calls[0]["sql"]
        assert "stock_id" not in sql

    def test_default_market_is_lowercase_tw(self):
        # business_indicator_tw schema uses default 'tw' (lowercase)
        conn = _CaptureConn()
        asof_business_indicator(conn, date(2024, 6, 1))
        assert conn.calls[0]["params"][0] == "tw"
