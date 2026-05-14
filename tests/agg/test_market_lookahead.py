"""_market.fetch_market_facts 內建 look-ahead filter 測試。

對齊 m3Spec/aggregation_layer.md §六 + §七。
"""

from __future__ import annotations

from datetime import date
from typing import Any
from unittest.mock import patch

from agg import _market


class _StubCursor:
    def __init__(self, rows):
        self._rows = rows
        self._executed = False

    def __enter__(self):
        return self

    def __exit__(self, *exc):
        return False

    def execute(self, sql, params=None):
        self._executed = True

    def fetchall(self):
        return self._rows


class _StubConn:
    def __init__(self, rows):
        self._rows = rows

    def cursor(self):
        return _StubCursor(self._rows)


def test_apply_lookahead_filter_true_drops_future_revenue():
    """revenue_core 的 fact_date <= as_of 但 report_date > as_of → 應被內建 filter 砍掉。"""
    rows = [
        # 可見:taiex 日 fact,fact_date 過去
        {
            "stock_id": "_index_taiex_",
            "fact_date": date(2026, 4, 1),
            "timeframe": "daily",
            "source_core": "taiex_core",
            "source_version": "0.1.0",
            "statement": "TaiexHigh",
            "metadata": {},
            "params_hash": "h1",
        },
        # 不可見:business_indicator_core 4 月 fact 但 report_date = 5/27 > as_of(5/10)
        {
            "stock_id": "_index_business_",
            "fact_date": date(2026, 4, 30),
            "timeframe": "monthly",
            "source_core": "business_indicator_core",
            "source_version": "0.1.0",
            "statement": "BusinessIndicatorRise",
            "metadata": {"report_date": "2026-05-27"},
            "params_hash": "h2",
        },
    ]
    conn = _StubConn(rows)
    out = _market.fetch_market_facts(
        conn,
        as_of=date(2026, 5, 10),
        lookback_days=60,
    )
    # taiex 可見
    assert len(out["_index_taiex_"]) == 1
    # business_indicator 因 report_date 未到被砍
    assert len(out["_index_business_"]) == 0


def test_apply_lookahead_filter_false_returns_raw():
    """關閉內建 filter → 同樣 row 都會回來(debug 用)。"""
    rows = [
        {
            "stock_id": "_index_business_",
            "fact_date": date(2026, 4, 30),
            "timeframe": "monthly",
            "source_core": "business_indicator_core",
            "source_version": "0.1.0",
            "statement": "BusinessIndicatorRise",
            "metadata": {"report_date": "2026-05-27"},
            "params_hash": "h2",
        },
    ]
    conn = _StubConn(rows)
    out = _market.fetch_market_facts(
        conn,
        as_of=date(2026, 5, 10),
        lookback_days=60,
        apply_lookahead_filter=False,
    )
    assert len(out["_index_business_"]) == 1


def test_grouped_keys_always_present():
    """5 個保留字 stock_id 永遠在 dict 中,即使無 row。"""
    conn = _StubConn(rows=[])
    out = _market.fetch_market_facts(conn, as_of=date(2026, 5, 10), lookback_days=30)
    expected = {
        "_index_taiex_", "_index_us_market_", "_index_business_",
        "_market_", "_global_",
    }
    assert set(out.keys()) == expected
    for v in out.values():
        assert v == []
