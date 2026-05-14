"""3 個 fundamental core figure builders(各自獨立 figure,不對齊 daily)。

- revenue_core(月頻):revenue bar + YoY % line(twin axis)
- valuation_core(日頻):per/pbr/yield 三線 + percentile zone hrect
- financial_statement_core(季頻):key metrics line + EPS bar + 18 欄表
"""

from __future__ import annotations

from typing import Any

import plotly.graph_objects as go
from plotly.subplots import make_subplots

from dashboards.charts._base import (
    PALETTE,
    coerce_date,
    extract_series,
)


# ────────────────────────────────────────────────────────────
# revenue_core(月頻)
# ────────────────────────────────────────────────────────────

def build_revenue_chart(revenue_indicator: dict[str, Any] | None) -> go.Figure:
    """月頻 revenue bar + YoY % line(twin axis)+ MoM line。

    series point: revenue / yoy_pct / mom_pct / cumulative / cumulative_yoy_pct
    """
    fig = make_subplots(specs=[[{"secondary_y": True}]])
    series = extract_series(revenue_indicator)
    if not series:
        fig.add_annotation(
            text="(無 revenue 資料)", xref="paper", yref="paper",
            x=0.5, y=0.5, showarrow=False, font=dict(size=14, color="gray"),
        )
        fig.update_layout(height=400, title="月營收")
        return fig

    dates = [coerce_date(p["date"]) for p in series if "date" in p]
    revenue = [p.get("revenue") for p in series if "date" in p]
    yoy = [p.get("yoy_pct") for p in series if "date" in p]
    mom = [p.get("mom_pct") for p in series if "date" in p]

    fig.add_trace(
        go.Bar(x=dates, y=revenue, name="月營收", marker_color="#1976D2", opacity=0.7),
        secondary_y=False,
    )
    if any(v is not None for v in yoy):
        fig.add_trace(
            go.Scatter(x=dates, y=yoy, name="YoY %", mode="lines+markers",
                       line=dict(color="#E91E63", width=2)),
            secondary_y=True,
        )
    if any(v is not None for v in mom):
        fig.add_trace(
            go.Scatter(x=dates, y=mom, name="MoM %", mode="lines+markers",
                       line=dict(color="#FF9800", width=1.5, dash="dot")),
            secondary_y=True,
        )

    fig.update_layout(
        title="月營收 + YoY/MoM",
        height=400,
        hovermode="x unified",
        plot_bgcolor="rgba(250, 250, 252, 1)",
    )
    fig.update_yaxes(title_text="營收(元)", secondary_y=False)
    fig.update_yaxes(title_text="YoY/MoM %", secondary_y=True)
    return fig


# ────────────────────────────────────────────────────────────
# valuation_core(日頻)
# ────────────────────────────────────────────────────────────

def build_valuation_chart(valuation_indicator: dict[str, Any] | None) -> go.Figure:
    """per / pbr / dividend_yield 三線 + 5y percentile zone hrect。

    series point: per / pbr / dividend_yield + per_percentile_5y / pbr_percentile_5y / dividend_yield_percentile_5y
    """
    fig = make_subplots(
        rows=3, cols=1, shared_xaxes=True,
        vertical_spacing=0.06,
        subplot_titles=["PER", "PBR", "殖利率 %"],
        row_heights=[0.34, 0.33, 0.33],
    )
    series = extract_series(valuation_indicator)
    if not series:
        fig.add_annotation(
            text="(無 valuation 資料)", xref="paper", yref="paper",
            x=0.5, y=0.5, showarrow=False, font=dict(size=14, color="gray"),
        )
        fig.update_layout(height=600, title="估值 percentile")
        return fig

    dates = [coerce_date(p["date"]) for p in series if "date" in p]
    per = [p.get("per") for p in series if "date" in p]
    pbr = [p.get("pbr") for p in series if "date" in p]
    yield_pct = [p.get("dividend_yield") for p in series if "date" in p]

    fig.add_trace(
        go.Scatter(x=dates, y=per, name="PER", mode="lines",
                   line=dict(color="#1976D2", width=1.8)),
        row=1, col=1,
    )
    fig.add_trace(
        go.Scatter(x=dates, y=pbr, name="PBR", mode="lines",
                   line=dict(color="#43A047", width=1.8)),
        row=2, col=1,
    )
    fig.add_trace(
        go.Scatter(x=dates, y=yield_pct, name="殖利率 %", mode="lines",
                   line=dict(color="#E91E63", width=1.8)),
        row=3, col=1,
    )

    # 5y 全期間 min-max 帶(可視為「歷史區間」)
    for i, vals in enumerate([per, pbr, yield_pct], start=1):
        clean = [v for v in vals if v is not None]
        if clean:
            lo, hi = min(clean), max(clean)
            band = (hi - lo) * 0.2
            # 80 percentile zone(高位區)
            fig.add_hrect(y0=hi - band, y1=hi,
                          fillcolor="rgba(239, 83, 80, 0.08)", line_width=0,
                          row=i, col=1)
            # 20 percentile zone(低位區)
            fig.add_hrect(y0=lo, y1=lo + band,
                          fillcolor="rgba(38, 166, 154, 0.08)", line_width=0,
                          row=i, col=1)

    fig.update_layout(
        title="估值與歷史區間(高/低位 zone 標色)",
        height=600,
        hovermode="x unified",
        plot_bgcolor="rgba(250, 250, 252, 1)",
        showlegend=False,
    )
    return fig


# ────────────────────────────────────────────────────────────
# financial_statement_core(季頻)— EPS bar + key metrics line + 18 欄表
# ────────────────────────────────────────────────────────────

def build_financial_statement_view(fin_indicator: dict[str, Any] | None) -> tuple[go.Figure, list[dict[str, Any]]]:
    """回 (figure, table_rows):
    - figure:EPS bar + revenue / gross_profit / net_income trend
    - table_rows:季頻 18 欄 raw rows(streamlit st.dataframe 用)
    """
    fig = make_subplots(
        rows=2, cols=1, shared_xaxes=True,
        vertical_spacing=0.08,
        subplot_titles=["EPS", "收入 / 毛利 / 淨利"],
        row_heights=[0.40, 0.60],
    )
    series = extract_series(fin_indicator)
    if not series:
        fig.add_annotation(
            text="(無 financial_statement 資料)", xref="paper", yref="paper",
            x=0.5, y=0.5, showarrow=False, font=dict(size=14, color="gray"),
        )
        fig.update_layout(height=500, title="財報季頻")
        return fig, []

    dates = [coerce_date(p["date"]) for p in series if "date" in p]
    eps = [p.get("eps") for p in series if "date" in p]
    revenue = [p.get("revenue") for p in series if "date" in p]
    gross = [p.get("gross_profit") for p in series if "date" in p]
    net = [p.get("net_income") for p in series if "date" in p]

    # EPS bar
    eps_colors = [PALETTE["macd_hist_up"] if (e or 0) >= 0 else PALETTE["macd_hist_down"] for e in eps]
    fig.add_trace(
        go.Bar(x=dates, y=eps, name="EPS", marker_color=eps_colors),
        row=1, col=1,
    )

    fig.add_trace(
        go.Scatter(x=dates, y=revenue, name="營收", mode="lines+markers",
                   line=dict(color="#1976D2", width=2)),
        row=2, col=1,
    )
    fig.add_trace(
        go.Scatter(x=dates, y=gross, name="毛利", mode="lines+markers",
                   line=dict(color="#43A047", width=2)),
        row=2, col=1,
    )
    fig.add_trace(
        go.Scatter(x=dates, y=net, name="淨利", mode="lines+markers",
                   line=dict(color="#FB8C00", width=2)),
        row=2, col=1,
    )

    fig.update_layout(
        title="財報季頻指標",
        height=500,
        hovermode="x unified",
        plot_bgcolor="rgba(250, 250, 252, 1)",
    )

    # Table rows(展開所有 fields)
    table_rows = []
    for p in series:
        row = {"date": p.get("date")}
        for k, v in p.items():
            if k != "date":
                row[k] = v
        table_rows.append(row)
    return fig, table_rows
