"""v4.34 regression:revenue / financial chart x 軸欄位對齊真實序列化 shape。

RevenuePoint / FinancialPoint Rust 序列化**無 `date` 欄**(只有 period / fact_date /
report_date)。舊 chart 讀 `p["date"]` → `if "date" in p` 永遠 False → 靜默空圖(不
crash,比 crash 更難察覺)。本 test 鎖定改讀 `fact_date` 後 x 軸有資料。

plotly 缺裝時整檔 skip(對齊 sandbox 無 plotly)。
"""

from __future__ import annotations

import pytest

pytest.importorskip("plotly")

from dashboards.charts.fundamental import (  # noqa: E402
    build_financial_statement_view,
    build_revenue_chart,
)
from dashboards.charts.environment import build_business_indicator_matrix  # noqa: E402


def _revenue_indicator() -> dict:
    """對齊 RevenuePoint 真實序列化:period / fact_date / report_date(無 date)。"""
    return {
        "value": {
            "series": [
                {"period": "2026-01", "fact_date": "2026-01-31", "report_date": "2026-02-10",
                 "revenue": 1000, "yoy_pct": 12.5, "mom_pct": 3.2},
                {"period": "2026-02", "fact_date": "2026-02-28", "report_date": "2026-03-10",
                 "revenue": 1100, "yoy_pct": 15.0, "mom_pct": 10.0},
            ]
        }
    }


def _financial_indicator() -> dict:
    """對齊 FinancialPoint 真實序列化:period / fact_date(無 date)。"""
    return {
        "value": {
            "series": [
                {"period": "2026Q1", "fact_date": "2026-03-31", "report_date": "2026-05-15",
                 "revenue": 5000, "gross_profit": 2000, "net_income": 800, "eps": 1.5},
                {"period": "2026Q2", "fact_date": "2026-06-30", "report_date": "2026-08-14",
                 "revenue": 5500, "gross_profit": 2300, "net_income": 950, "eps": 1.8},
            ]
        }
    }


def test_revenue_chart_x_axis_uses_fact_date():
    fig = build_revenue_chart(_revenue_indicator())
    # bar trace(月營收)x 軸應有 2 點(改讀 fact_date 後不再空)
    bar = next(t for t in fig.data if t.type == "bar")
    assert len(bar.x) == 2, "revenue bar x 軸空白 — fact_date 讀取失敗"
    assert list(bar.y) == [1000, 1100]


def test_financial_view_x_axis_uses_fact_date():
    fig, table_rows = build_financial_statement_view(_financial_indicator())
    bar = next(t for t in fig.data if t.type == "bar")  # EPS bar
    assert len(bar.x) == 2, "financial EPS bar x 軸空白 — fact_date 讀取失敗"
    assert list(bar.y) == [1.5, 1.8]
    # table rows 以 period / fact_date 領頭,且保留所有欄位
    assert table_rows[0]["period"] == "2026Q1"
    assert table_rows[0]["fact_date"] == "2026-03-31"
    assert table_rows[0]["eps"] == 1.5


def _business_indicator() -> dict:
    """對齊 BusinessIndicatorPoint 真實序列化:period / fact_date(無 date)。"""
    return {
        "value": {
            "series": [
                {"period": "2026-01", "fact_date": "2026-01-31", "report_date": "2026-02-27",
                 "leading_indicator": 99.5, "coincident_indicator": 101.2,
                 "lagging_indicator": 100.0, "monitoring": 28, "monitoring_color": "G"},
                {"period": "2026-02", "fact_date": "2026-02-28", "report_date": "2026-03-27",
                 "leading_indicator": 100.1, "coincident_indicator": 102.0,
                 "lagging_indicator": 100.5, "monitoring": 32, "monitoring_color": "YR"},
            ]
        }
    }


def test_business_indicator_matrix_x_axis_uses_fact_date():
    fig = build_business_indicator_matrix(_business_indicator())
    # monitoring bar(對策信號)x 軸應有 2 點(改讀 fact_date 後不再空)
    bar = next(t for t in fig.data if t.type == "bar")
    assert len(bar.x) == 2, "景氣指標 bar x 軸空白 — fact_date 讀取失敗"
    assert list(bar.y) == [28, 32]
