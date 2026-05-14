"""Unit tests for mcp_server.tools.data。

Mock psycopg connection via monkeypatch — 不對真實 PG 跑。
"""

from __future__ import annotations

import sys
from datetime import date
from pathlib import Path
from unittest.mock import MagicMock

# Ensure sys.path 對齊 mcp_server/_conn.py 走 stdio 時的 path 配置
_REPO_ROOT = Path(__file__).resolve().parent.parent.parent
_SRC_ROOT = _REPO_ROOT / "src"
for p in (str(_SRC_ROOT), str(_REPO_ROOT)):
    if p not in sys.path:
        sys.path.insert(0, p)

import pytest

from mcp_server.tools import data as data_tools


class TestParseDate:
    def test_iso_string(self):
        assert data_tools._parse_date("2026-05-13") == date(2026, 5, 13)

    def test_date_passthrough(self):
        d = date(2026, 1, 1)
        assert data_tools._parse_date(d) == d

    def test_invalid_string(self):
        with pytest.raises(ValueError):
            data_tools._parse_date("nope")


class TestListCores:
    def test_returns_23_cores(self):
        out = data_tools.list_cores()
        assert out["total"] == 23
        assert len(out["cores"]) == 23

    def test_by_kind_breakdown(self):
        out = data_tools.list_cores()
        # 對齊 plan §Tool surface 註解 + cores_overview §8
        assert out["by_kind"] == {
            "Wave": 1,
            "Indicator": 8,
            "Chip": 5,
            "Fundamental": 3,
            "Environment": 6,
        }

    def test_neely_in_wave(self):
        out = data_tools.list_cores()
        names = {c["name"]: c["kind"] for c in out["cores"]}
        assert names["neely_core"] == "Wave"
        assert names["macd_core"] == "Indicator"
        assert names["institutional_core"] == "Chip"
        assert names["valuation_core"] == "Fundamental"
        assert names["taiex_core"] == "Environment"


class TestAsOfSnapshot:
    def test_wraps_agg_as_of(self, monkeypatch):
        """確認 tool 把 ISO 字串轉 date + 傳遞參數。"""
        from agg import _types

        captured: dict[str, object] = {}

        def fake_as_of(stock_id, as_of_date, **kwargs):
            captured["stock_id"] = stock_id
            captured["as_of"] = as_of_date
            captured.update(kwargs)
            return _types.AsOfSnapshot(
                stock_id=stock_id,
                as_of=as_of_date,
                metadata=_types.QueryMetadata(
                    stock_id=stock_id,
                    as_of=as_of_date,
                    lookback_days=kwargs.get("lookback_days", 0),
                    cores=kwargs.get("cores"),
                    include_market=kwargs.get("include_market", True),
                    timeframes=kwargs.get("timeframes"),
                ),
            )

        # data_tools.as_of_snapshot imports `from agg import as_of` locally,
        # so we monkey-patch the agg module's binding.
        import agg
        monkeypatch.setattr(agg, "as_of", fake_as_of)

        result = data_tools.as_of_snapshot(
            "2330",
            "2026-05-13",
            lookback_days=60,
            include_market=False,
            cores=["macd_core"],
        )

        assert captured["stock_id"] == "2330"
        assert captured["as_of"] == date(2026, 5, 13)
        assert captured["lookback_days"] == 60
        assert captured["include_market"] is False
        assert captured["cores"] == ["macd_core"]

        # 回 dict(JSON-serializable)
        assert isinstance(result, dict)
        assert result["stock_id"] == "2330"
        assert result["as_of"] == "2026-05-13"


class TestFindFacts:
    def test_wraps_find_facts_today(self, monkeypatch):
        from agg import _types

        captured: dict[str, object] = {}

        def fake_find(today, *, source_core=None, kind=None, **_):
            captured["today"] = today
            captured["source_core"] = source_core
            captured["kind"] = kind
            return [
                _types.FactRow(
                    stock_id="2330",
                    fact_date=today,
                    timeframe="daily",
                    source_core=source_core or "macd_core",
                    source_version="0.4.0",
                    statement="GoldenCross",
                    metadata={"kind": kind or "GoldenCross"},
                ),
            ]

        import agg
        monkeypatch.setattr(agg, "find_facts_today", fake_find)

        out = data_tools.find_facts("2026-05-13", source_core="macd_core", kind="GoldenCross")
        assert captured["today"] == date(2026, 5, 13)
        assert captured["source_core"] == "macd_core"
        assert captured["kind"] == "GoldenCross"
        assert len(out) == 1
        assert out[0]["stock_id"] == "2330"
        # date 已 ISO 字串
        assert out[0]["fact_date"] == "2026-05-13"


class TestFetchOhlc:
    def test_serializes_date_and_decimals(self, monkeypatch):
        """fetch_ohlc 內部 Decimal/date 應轉成 float/ISO 字串。"""
        from decimal import Decimal

        mock_conn = MagicMock()

        # Patch get_connection 直接回 mock conn
        from agg import _db
        monkeypatch.setattr(_db, "get_connection", lambda *a, **kw: mock_conn)

        # Patch fetch_ohlc 回固定 rows(模擬 PG response)
        def fake_fetch(_conn, *, stock_id, as_of, lookback_days):
            return [
                {
                    "date":   date(2026, 5, 12),
                    "open":   Decimal("100.5"),
                    "high":   Decimal("105.0"),
                    "low":    Decimal("99.0"),
                    "close":  Decimal("103.0"),
                    "volume": 12345678,
                },
                {
                    "date":   date(2026, 5, 13),
                    "open":   None,  # NULL value passthrough
                    "high":   None,
                    "low":    None,
                    "close":  None,
                    "volume": None,
                },
            ]

        monkeypatch.setattr(_db, "fetch_ohlc", fake_fetch)

        result = data_tools.fetch_ohlc("2330", "2026-05-13", lookback_days=60)
        assert len(result) == 2

        assert result[0]["date"] == "2026-05-12"
        assert isinstance(result[0]["open"], float)
        assert result[0]["open"] == 100.5
        assert result[0]["close"] == 103.0

        assert result[1]["date"] == "2026-05-13"
        assert result[1]["open"] is None
        assert result[1]["volume"] is None

        mock_conn.close.assert_called_once()
