"""Tests for B-4 機制丙 — forecast_log.internal_only filter integrity.

對齊 m3Spec/dual_track_resonance.md §七:
- upsert_forecast 透傳 internal_only(預設 False)
- fetch_resolved 預設 include_internal=False(scorer / 對外路徑不 leak)
- fetch_unresolved 預設 include_internal=True(settlement 仍處理所有 row)
- fusion eligible_cores / _fetch_eligible_forecasts SQL 含 internal_only=FALSE
- calibration _fetch_calibration_set / _fetch_raw_forecast SQL 含 internal_only=FALSE
"""

from __future__ import annotations

import re
import sys
from datetime import date
from pathlib import Path
from unittest.mock import MagicMock

import pytest

_REPO_ROOT = Path(__file__).resolve().parent.parent.parent
_SRC_ROOT = _REPO_ROOT / "src"
for p in (str(_SRC_ROOT), str(_REPO_ROOT)):
    if p not in sys.path:
        sys.path.insert(0, p)


# ── upsert_forecast 透傳 internal_only ────────────────────────────────────────


class _CursorMock:
    """Cursor mock that captures executed SQL + params."""

    def __init__(self):
        self.calls: list[tuple[str, object]] = []

    def __enter__(self):
        return self

    def __exit__(self, exc_type, exc, tb):
        return False

    def execute(self, sql, params=None):
        self.calls.append((sql, params))

    def fetchall(self):
        return []

    def fetchone(self):
        return None


class _ConnMock:
    def __init__(self):
        self.cur = _CursorMock()

    def cursor(self, *a, **kw):
        return self.cur


class TestUpsertInternalOnly:
    def test_defaults_to_false(self):
        from forecast._db import upsert_forecast

        conn = _ConnMock()
        upsert_forecast(conn, {
            "stock_id": "2330",
            "forecast_date": date(2024, 6, 1),
            "horizon_days": 63,
            "confidence": 0.80,
            "source_core": "baseline",
            "lower": 90.0,
            "upper": 110.0,
            "point": 100.0,
        })
        # row default 不帶 internal_only → payload 帶 False
        sql, params = conn.cur.calls[0]
        assert "internal_only" in sql
        assert params["internal_only"] is False

    def test_passes_through_true(self):
        from forecast._db import upsert_forecast

        conn = _ConnMock()
        upsert_forecast(conn, {
            "stock_id": "2330",
            "forecast_date": date(2024, 6, 1),
            "horizon_days": 63,
            "confidence": 0.60,
            "source_core": "neely_fib",
            "internal_only": True,
            "lower": 90.0,
            "upper": 110.0,
            "point": 100.0,
        })
        sql, params = conn.cur.calls[0]
        assert params["internal_only"] is True


# ── fetch_resolved 預設過濾 internal_only ─────────────────────────────────────


class TestFetchResolvedFilter:
    def test_default_excludes_internal(self):
        from forecast._db import fetch_resolved

        conn = _ConnMock()
        fetch_resolved(conn)
        sql, _ = conn.cur.calls[0]
        assert "internal_only = FALSE" in sql

    def test_include_internal_skips_filter(self):
        from forecast._db import fetch_resolved

        conn = _ConnMock()
        fetch_resolved(conn, include_internal=True)
        sql, _ = conn.cur.calls[0]
        # SELECT 仍含 internal_only 欄(回傳值);WHERE 不應有 internal_only = FALSE
        assert "internal_only = FALSE" not in sql

    def test_combines_with_other_filters(self):
        from forecast._db import fetch_resolved

        conn = _ConnMock()
        fetch_resolved(conn, source_core="baseline", horizon_days=63,
                       stock_id="2330", since=date(2024, 1, 1))
        sql, _ = conn.cur.calls[0]
        assert "internal_only = FALSE" in sql
        assert "source_core = %s" in sql
        assert "horizon_days = %s" in sql


# ── fetch_unresolved 預設 include_internal=True(settlement 看全部)─────────


class TestFetchUnresolvedFilter:
    def test_default_includes_internal(self):
        """settlement 必須 resolve 所有 row(否則 internal 對齊影子永遠堆積)。"""
        from forecast._db import fetch_unresolved

        conn = _ConnMock()
        fetch_unresolved(conn, asof=date(2024, 6, 1))
        sql, _ = conn.cur.calls[0]
        # SELECT 仍含 internal_only 欄(回傳值);WHERE 不應有 internal_only = FALSE
        assert "internal_only = FALSE" not in sql

    def test_explicit_exclude(self):
        from forecast._db import fetch_unresolved

        conn = _ConnMock()
        fetch_unresolved(conn, asof=date(2024, 6, 1), include_internal=False)
        sql, _ = conn.cur.calls[0]
        assert "internal_only = FALSE" in sql


# ── fusion.eligible_cores / _fetch_eligible_forecasts SQL 含過濾 ──────────────


class TestFusionFilter:
    def test_mean_pinball_sql_filters(self):
        """_mean_pinball 內 subquery 含 internal_only = FALSE。"""
        from forecast.fusion import _mean_pinball

        conn = _ConnMock()
        _mean_pinball(
            conn,
            source_core="baseline",
            horizon_days=63,
            confidence=0.80,
            asof=date(2024, 6, 1),
            window=100,
        )
        sql, _ = conn.cur.calls[0]
        assert "internal_only = FALSE" in sql

    def test_eligible_cores_excludes_internal(self):
        """eligible_cores 的 candidate discovery SQL 含 internal_only=FALSE。"""
        from forecast import fusion

        # 模擬 baseline mean_pinball 過門檻
        conn = _ConnMock()

        # 攔截 _mean_pinball 讓它回(baseline_pinball=10.0, n=100),不污染 SQL 捕捉
        from unittest.mock import patch
        with patch.object(fusion, "_mean_pinball", return_value=(10.0, 100)):
            fusion.eligible_cores(
                conn,
                asof=date(2024, 6, 1),
                horizon_days=63,
                confidence=0.80,
            )
        # 找到 candidate discovery 的 SQL — 應有 "DISTINCT source_core"
        sqls = [c[0] for c in conn.cur.calls]
        candidate_sqls = [s for s in sqls if "DISTINCT source_core" in s]
        assert candidate_sqls, "expected DISTINCT source_core SQL"
        assert "internal_only" in candidate_sqls[0]

    def test_fetch_eligible_forecasts_filters(self):
        from forecast.fusion import _fetch_eligible_forecasts

        conn = _ConnMock()
        _fetch_eligible_forecasts(
            conn,
            stock_id="2330",
            forecast_date=date(2024, 6, 1),
            horizon_days=63,
            confidence=0.80,
            cores=["kalman_cqr"],
        )
        sql, _ = conn.cur.calls[0]
        assert "internal_only = FALSE" in sql


# ── calibration helpers 含過濾 ────────────────────────────────────────────────


class TestCalibrationFilter:
    def test_raw_forecast_filters(self):
        from forecast.calibration import _fetch_raw_forecast

        conn = _ConnMock()
        _fetch_raw_forecast(
            conn,
            stock_id="2330",
            forecast_date=date(2024, 6, 1),
            horizon_days=63,
            confidence=0.80,
            source_core="kalman_forecast_core",
        )
        sql, _ = conn.cur.calls[0]
        assert "internal_only = FALSE" in sql

    def test_calibration_set_filters(self):
        from forecast.calibration import _fetch_calibration_set

        conn = _ConnMock()
        _fetch_calibration_set(
            conn,
            stock_id="2330",
            asof=date(2024, 6, 1),
            horizon_days=63,
            confidence=0.80,
            source_core="kalman_forecast_core",
            window=500,
        )
        sql, _ = conn.cur.calls[0]
        assert "internal_only = FALSE" in sql
