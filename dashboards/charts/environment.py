"""6 個 environment core figure builders。

對齊 plan §「charts/environment.py」+ explore 結果:
- taiex_core(雙序列 series_by_index TAIEX/TPEx)
- us_market_core(SPY + VIX zone)
- exchange_rate_core(rate + trend_state)
- fear_greed_core(zone gauge)
- market_margin_core(maintenance dial + zone)
- business_indicator_core(月頻 monitoring color matrix)
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
# taiex_core(雙序列 nested)
# ────────────────────────────────────────────────────────────

def build_taiex_chart(taiex_indicator: dict[str, Any] | None) -> go.Figure:
    """series_by_index dispatch:TAIEX / TPEx 各畫一條 close line + RSI subplot。"""
    fig = make_subplots(
        rows=2, cols=1, shared_xaxes=True,
        vertical_spacing=0.06,
        subplot_titles=["TAIEX / TPEx close", "RSI"],
        row_heights=[0.6, 0.4],
    )
    if not taiex_indicator:
        fig.add_annotation(text="(無 TAIEX 資料)", xref="paper", yref="paper",
                           x=0.5, y=0.5, showarrow=False, font=dict(color="gray"))
        fig.update_layout(height=500, title="台股大盤")
        return fig

    value = taiex_indicator.get("value", {})
    series_by_index = value.get("series_by_index") or []
    color_map = {"TAIEX": "#E91E63", "TPEx": "#1976D2"}

    for entry in series_by_index:
        idx_code = entry.get("index_code") or entry.get("code") or "TAIEX"
        series = entry.get("series") or []
        if not series:
            continue
        dates = [coerce_date(p["date"]) for p in series if "date" in p]
        closes = [p.get("close") for p in series if "date" in p]
        rsi = [p.get("rsi") for p in series if "date" in p]
        color = color_map.get(idx_code, "#9E9E9E")
        fig.add_trace(
            go.Scatter(x=dates, y=closes, name=f"{idx_code} close", mode="lines",
                       line=dict(color=color, width=1.8)),
            row=1, col=1,
        )
        if any(v is not None for v in rsi):
            fig.add_trace(
                go.Scatter(x=dates, y=rsi, name=f"{idx_code} RSI", mode="lines",
                           line=dict(color=color, width=1, dash="dot")),
                row=2, col=1,
            )

    fig.update_yaxes(title_text="點數", row=1, col=1)
    fig.update_yaxes(title_text="RSI", row=2, col=1, range=[0, 100])
    fig.update_layout(height=500, title="台股大盤(TAIEX / TPEx)",
                      hovermode="x unified", plot_bgcolor="rgba(250,250,252,1)")
    return fig


# ────────────────────────────────────────────────────────────
# us_market_core(SPY + VIX zone)
# ────────────────────────────────────────────────────────────

VIX_ZONE_COLORS = {
    "Low":         "rgba(38, 166, 154, 0.10)",
    "Normal":      "rgba(255, 255, 255, 0.0)",
    "High":        "rgba(255, 152, 0, 0.10)",
    "ExtremeHigh": "rgba(239, 83, 80, 0.15)",
}


def build_us_market_chart(us_indicator: dict[str, Any] | None) -> go.Figure:
    """SPY close + VIX line(twin)+ VIX zone background hrect。"""
    fig = make_subplots(specs=[[{"secondary_y": True}]])
    series = extract_series(us_indicator)
    if not series:
        fig.add_annotation(text="(無 US market 資料)", xref="paper", yref="paper",
                           x=0.5, y=0.5, showarrow=False, font=dict(color="gray"))
        fig.update_layout(height=400, title="美股 SPY / VIX")
        return fig

    dates = [coerce_date(p["date"]) for p in series if "date" in p]
    spy = [p.get("spy_close") for p in series if "date" in p]
    vix = [p.get("vix_close") for p in series if "date" in p]
    zones = [p.get("vix_zone") for p in series if "date" in p]

    fig.add_trace(
        go.Scatter(x=dates, y=spy, name="SPY", mode="lines",
                   line=dict(color="#1976D2", width=1.8)),
        secondary_y=False,
    )
    fig.add_trace(
        go.Scatter(x=dates, y=vix, name="VIX", mode="lines",
                   line=dict(color="#E91E63", width=1.5, dash="dot")),
        secondary_y=True,
    )

    # VIX zone band(連續同 zone 合併成一段 hrect)
    if zones and dates:
        cur_zone = zones[0]
        start = dates[0]
        for i in range(1, len(zones)):
            if zones[i] != cur_zone:
                color = VIX_ZONE_COLORS.get(cur_zone)
                if color and color != "rgba(255, 255, 255, 0.0)":
                    fig.add_vrect(x0=start, x1=dates[i],
                                  fillcolor=color, line_width=0,
                                  annotation_text=cur_zone, annotation_position="top left")
                cur_zone = zones[i]
                start = dates[i]
        # last
        color = VIX_ZONE_COLORS.get(cur_zone)
        if color and color != "rgba(255, 255, 255, 0.0)":
            fig.add_vrect(x0=start, x1=dates[-1],
                          fillcolor=color, line_width=0,
                          annotation_text=cur_zone, annotation_position="top left")

    fig.update_yaxes(title_text="SPY", secondary_y=False)
    fig.update_yaxes(title_text="VIX", secondary_y=True)
    fig.update_layout(height=400, title="美股 SPY / VIX(VIX zone 背景)",
                      hovermode="x unified", plot_bgcolor="rgba(250,250,252,1)")
    return fig


# ────────────────────────────────────────────────────────────
# exchange_rate_core
# ────────────────────────────────────────────────────────────

def build_exchange_rate_chart(er_indicator: dict[str, Any] | None) -> go.Figure:
    """匯率 line + MA + trend_state annotation。"""
    fig = go.Figure()
    series = extract_series(er_indicator)
    if not series:
        fig.add_annotation(text="(無匯率資料)", xref="paper", yref="paper",
                           x=0.5, y=0.5, showarrow=False, font=dict(color="gray"))
        fig.update_layout(height=300, title="匯率")
        return fig

    dates = [coerce_date(p["date"]) for p in series if "date" in p]
    rate = [p.get("rate") for p in series if "date" in p]
    ma = [p.get("ma_value") for p in series if "date" in p]

    fig.add_trace(go.Scatter(x=dates, y=rate, name="USD/TWD", mode="lines",
                             line=dict(color="#1976D2", width=1.8)))
    if any(v is not None for v in ma):
        fig.add_trace(go.Scatter(x=dates, y=ma, name="MA", mode="lines",
                                 line=dict(color="#FF9800", width=1, dash="dot")))

    fig.update_layout(height=300, title="USD/TWD 匯率",
                      hovermode="x unified", plot_bgcolor="rgba(250,250,252,1)")
    return fig


# ────────────────────────────────────────────────────────────
# fear_greed_core
# ────────────────────────────────────────────────────────────

def build_fear_greed_gauge(fg_indicator: dict[str, Any] | None) -> go.Figure:
    """plotly Indicator gauge(現值)+ 30 天時序 line。"""
    series = extract_series(fg_indicator)
    if not series:
        fig = go.Figure()
        fig.add_annotation(text="(無 fear-greed 資料)", xref="paper", yref="paper",
                           x=0.5, y=0.5, showarrow=False, font=dict(color="gray"))
        fig.update_layout(height=300, title="Fear & Greed")
        return fig

    latest = series[-1]
    current = latest.get("value")
    zone = latest.get("zone") or "Neutral"

    fig = make_subplots(
        rows=1, cols=2,
        column_widths=[0.4, 0.6],
        specs=[[{"type": "indicator"}, {"type": "xy"}]],
        subplot_titles=[f"當前: {zone}", "歷史時序"],
    )

    fig.add_trace(
        go.Indicator(
            mode="gauge+number",
            value=current or 0,
            number={"suffix": ""},
            gauge={
                "axis": {"range": [0, 100]},
                "bar": {"color": _zone_color(zone)},
                "steps": [
                    {"range": [0, 25],   "color": "rgba(239, 83, 80, 0.3)"},   # ExtremeFear
                    {"range": [25, 45],  "color": "rgba(255, 152, 0, 0.2)"},   # Fear
                    {"range": [45, 55],  "color": "rgba(158, 158, 158, 0.2)"}, # Neutral
                    {"range": [55, 75],  "color": "rgba(76, 175, 80, 0.2)"},   # Greed
                    {"range": [75, 100], "color": "rgba(38, 166, 154, 0.3)"},  # ExtremeGreed
                ],
                "threshold": {"line": {"color": "black", "width": 2}, "thickness": 0.8, "value": current or 0},
            },
        ),
        row=1, col=1,
    )

    dates = [coerce_date(p["date"]) for p in series if "date" in p]
    values = [p.get("value") for p in series if "date" in p]
    fig.add_trace(
        go.Scatter(x=dates, y=values, name="Fear-Greed", mode="lines",
                   line=dict(color="#9C27B0", width=1.8)),
        row=1, col=2,
    )
    fig.update_yaxes(range=[0, 100], row=1, col=2)
    fig.update_layout(height=350, title="Fear & Greed Index",
                      showlegend=False, plot_bgcolor="rgba(250,250,252,1)")
    return fig


def _zone_color(zone: str) -> str:
    return {
        "ExtremeFear":  "#EF5350",
        "Fear":         "#FF9800",
        "Neutral":      "#9E9E9E",
        "Greed":        "#4CAF50",
        "ExtremeGreed": "#26A69A",
    }.get(zone, "#9E9E9E")


# ────────────────────────────────────────────────────────────
# market_margin_core(融資維持率 dial)
# ────────────────────────────────────────────────────────────

def build_market_margin_dial(mm_indicator: dict[str, Any] | None) -> go.Figure:
    """融資維持率 gauge + 時序 line。"""
    series = extract_series(mm_indicator)
    if not series:
        fig = go.Figure()
        fig.add_annotation(text="(無融資維持率資料)", xref="paper", yref="paper",
                           x=0.5, y=0.5, showarrow=False, font=dict(color="gray"))
        fig.update_layout(height=300, title="融資維持率")
        return fig

    latest = series[-1]
    current = latest.get("maintenance_rate")
    zone = latest.get("zone") or "Safe"

    fig = make_subplots(
        rows=1, cols=2,
        column_widths=[0.35, 0.65],
        specs=[[{"type": "indicator"}, {"type": "xy"}]],
        subplot_titles=[f"當前: {zone} ({current})", "歷史時序"],
    )
    fig.add_trace(
        go.Indicator(
            mode="gauge+number",
            value=current or 0,
            number={"suffix": "%"},
            gauge={
                "axis": {"range": [100, 200]},
                "bar": {"color": {"Safe": "#26A69A", "Warning": "#FF9800", "Danger": "#EF5350"}.get(zone, "#9E9E9E")},
                "steps": [
                    {"range": [100, 130], "color": "rgba(239, 83, 80, 0.3)"},
                    {"range": [130, 160], "color": "rgba(255, 152, 0, 0.2)"},
                    {"range": [160, 200], "color": "rgba(38, 166, 154, 0.2)"},
                ],
                "threshold": {"line": {"color": "black", "width": 2}, "thickness": 0.8, "value": current or 0},
            },
        ),
        row=1, col=1,
    )
    dates = [coerce_date(p["date"]) for p in series if "date" in p]
    values = [p.get("maintenance_rate") for p in series if "date" in p]
    fig.add_trace(
        go.Scatter(x=dates, y=values, name="維持率", mode="lines",
                   line=dict(color="#1976D2", width=1.8)),
        row=1, col=2,
    )
    fig.update_layout(height=350, title="融資維持率",
                      showlegend=False, plot_bgcolor="rgba(250,250,252,1)")
    return fig


# ────────────────────────────────────────────────────────────
# business_indicator_core(月頻 5-color matrix)
# ────────────────────────────────────────────────────────────

BIZ_COLOR_MAP = {
    "Blue":       "#1976D2",
    "YellowBlue": "#03A9F4",
    "Green":      "#4CAF50",
    "YellowRed":  "#FF9800",
    "Red":        "#EF5350",
}


def build_business_indicator_matrix(bi_indicator: dict[str, Any] | None) -> go.Figure:
    """leading / coincident / lagging / monitoring 四列 line + 月份顏色點(monitoring_color)。"""
    fig = make_subplots(
        rows=2, cols=1, shared_xaxes=True,
        vertical_spacing=0.08,
        subplot_titles=["景氣指標(領先 / 同時 / 落後)", "景氣對策信號(monitoring 9-45)"],
        row_heights=[0.55, 0.45],
    )
    series = extract_series(bi_indicator)
    if not series:
        fig.add_annotation(text="(無景氣指標)", xref="paper", yref="paper",
                           x=0.5, y=0.5, showarrow=False, font=dict(color="gray"))
        fig.update_layout(height=400, title="景氣指標")
        return fig

    dates = [coerce_date(p["date"]) for p in series if "date" in p]
    leading = [p.get("leading_indicator") for p in series if "date" in p]
    coincident = [p.get("coincident_indicator") for p in series if "date" in p]
    lagging = [p.get("lagging_indicator") for p in series if "date" in p]
    monitoring = [p.get("monitoring") for p in series if "date" in p]
    colors = [p.get("monitoring_color") for p in series if "date" in p]

    fig.add_trace(go.Scatter(x=dates, y=leading, name="領先", mode="lines+markers",
                             line=dict(color="#1976D2", width=1.5)),
                  row=1, col=1)
    fig.add_trace(go.Scatter(x=dates, y=coincident, name="同時", mode="lines+markers",
                             line=dict(color="#43A047", width=1.5)),
                  row=1, col=1)
    fig.add_trace(go.Scatter(x=dates, y=lagging, name="落後", mode="lines+markers",
                             line=dict(color="#FB8C00", width=1.5)),
                  row=1, col=1)

    # 對策信號:bar with color from monitoring_color
    bar_colors = [BIZ_COLOR_MAP.get(c, "#9E9E9E") for c in colors]
    fig.add_trace(
        go.Bar(x=dates, y=monitoring, name="景氣對策信號", marker_color=bar_colors),
        row=2, col=1,
    )
    fig.update_layout(height=500, title="景氣指標(月頻)",
                      hovermode="x unified", plot_bgcolor="rgba(250,250,252,1)")
    return fig
