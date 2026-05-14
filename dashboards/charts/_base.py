"""Plotly figure builders 共用元件 — palette / layout / 工具。

對齊 plan /root/.claude/plans/squishy-foraging-stroustrup.md。
"""

from __future__ import annotations

from datetime import date
from typing import Any

import plotly.graph_objects as go
from plotly.subplots import make_subplots


# ────────────────────────────────────────────────────────────
# Color palette(對齊 TradingView 慣例 — 紅綠 candle / 藍橘 ma)
# ────────────────────────────────────────────────────────────

PALETTE = {
    # K-line(台股慣例:紅漲綠跌,但 international 慣例綠漲紅跌 — 採後者對齊 plotly default)
    "candle_up":      "#26A69A",
    "candle_down":    "#EF5350",
    # Volume bar(對齊 candle 顏色)
    "volume_up":      "rgba(38, 166, 154, 0.5)",
    "volume_down":    "rgba(239, 83, 80, 0.5)",
    # MA 線(漸進冷暖)
    "ma20":           "#1976D2",
    "ma60":           "#FF9800",
    "ma200":          "#9C27B0",
    "ma_default":     "#607D8B",
    # Bollinger
    "bollinger_mid":   "#90A4AE",
    "bollinger_band":  "#90A4AE",
    "bollinger_fill":  "rgba(96, 125, 139, 0.10)",
    # MACD
    "macd_line":      "#2196F3",
    "macd_signal":    "#FF9800",
    "macd_hist_up":   "#26A69A",
    "macd_hist_down": "#EF5350",
    # RSI
    "rsi":            "#9C27B0",
    "rsi_ref":        "rgba(120, 120, 120, 0.5)",
    # KD
    "kd_k":           "#1976D2",
    "kd_d":           "#FF5722",
    # ADX
    "adx":            "#212121",
    "plus_di":        "#26A69A",
    "minus_di":       "#EF5350",
    # ATR / OBV
    "atr":            "#795548",
    "atr_pct":        "#FFC107",
    "obv":            "#3F51B5",
    "obv_ma":         "#FF9800",
    # Neely zigzag
    "neely_zigzag":   "#FFC107",
    "neely_label":    "#212121",
    "neely_fib_zone": "rgba(255, 193, 7, 0.08)",
    # Day trading
    "day_trading_ratio": "#E91E63",
    # Chip — institutional
    "foreign_net":    "#1976D2",
    "trust_net":      "#43A047",
    "dealer_net":     "#FB8C00",
    # Facts default(實際 color by kind hash)
    "fact_default":   "#607D8B",
}


# ────────────────────────────────────────────────────────────
# Subplot layout helpers
# ────────────────────────────────────────────────────────────

def make_kline_subplots(
    n_rows: int,
    row_heights: list[float],
    *,
    vertical_spacing: float = 0.02,
    subplot_titles: list[str] | None = None,
    specs: list[list[dict]] | None = None,
) -> go.Figure:
    """共用 multi-row subplot factory(K-line + indicator subplots)。

    Args:
        n_rows: 總列數
        row_heights: 每列高度比例(總和應 = 1)
        vertical_spacing: 列間距
        subplot_titles: 各列標題(optional)
        specs: 各列 specs(twin axis 用 secondary_y=True)

    Returns:
        plotly Figure with shared x-axis
    """
    if specs is None:
        specs = [[{"secondary_y": False}] for _ in range(n_rows)]
    fig = make_subplots(
        rows=n_rows,
        cols=1,
        shared_xaxes=True,
        vertical_spacing=vertical_spacing,
        row_heights=row_heights,
        subplot_titles=subplot_titles,
        specs=specs,
    )
    fig.update_layout(
        showlegend=True,
        legend=dict(
            orientation="v",
            yanchor="top",
            y=1.0,
            xanchor="left",
            x=1.02,
            bgcolor="rgba(255, 255, 255, 0.8)",
            bordercolor="rgba(0, 0, 0, 0.1)",
            borderwidth=1,
        ),
        margin=dict(l=40, r=120, t=40, b=40),
        hovermode="x unified",
        xaxis_rangeslider_visible=False,
        plot_bgcolor="rgba(250, 250, 252, 1)",
        paper_bgcolor="white",
        font=dict(family="-apple-system, system-ui, sans-serif", size=11),
    )
    # 每個 subplot 的 x-axis 隱藏 rangeslider(K-line default 會出)
    for i in range(1, n_rows + 1):
        fig.update_xaxes(rangeslider=dict(visible=False), row=i, col=1)
    return fig


def add_horizontal_reference(
    fig: go.Figure,
    y: float,
    *,
    row: int,
    col: int = 1,
    color: str = "rgba(120, 120, 120, 0.5)",
    dash: str = "dash",
    width: int = 1,
    annotation: str | None = None,
) -> None:
    """加水平 reference line(rsi 70/30 / kd 80/20 等)。

    用 Scatter 而非 add_hline 因為 add_hline 不支援 row/col 在某些 plotly 版本。
    """
    fig.add_shape(
        type="line",
        xref=f"x{row if row > 1 else ''}",
        yref=f"y{row if row > 1 else ''}",
        x0=0, x1=1,
        y0=y, y1=y,
        line=dict(color=color, dash=dash, width=width),
        row=row,
        col=col,
    )
    if annotation:
        fig.add_annotation(
            xref="paper",
            yref=f"y{row if row > 1 else ''}",
            x=1.0,
            y=y,
            text=annotation,
            showarrow=False,
            font=dict(size=9, color=color),
            xanchor="left",
            yanchor="middle",
        )


def coerce_date(d: Any) -> date:
    """支援 date / datetime / ISO string 三種輸入(對齊 _lookahead.coerce_date)。"""
    if isinstance(d, date):
        return d
    if hasattr(d, "date"):
        return d.date()
    if isinstance(d, str):
        return date.fromisoformat(d[:10])
    raise TypeError(f"無法 coerce 成 date: {type(d).__name__}={d!r}")


def extract_series(indicator: dict[str, Any] | None) -> list[dict[str, Any]]:
    """從 indicator_latest dict 抽 series array。

    indicator value JSONB 形如 {series: [{date, ...}], ...}。
    若 indicator 為 None 或無 series,回 []。
    """
    if not indicator:
        return []
    value = indicator.get("value")
    if not isinstance(value, dict):
        return []
    series = value.get("series")
    return list(series) if isinstance(series, list) else []


def fact_color_by_kind(kind: str | None) -> str:
    """facts 散點雲:metadata.kind 雜湊到固定 color。

    對齊 plotly qualitative palette,確保同 kind 永遠同色。
    """
    if not kind:
        return PALETTE["fact_default"]
    palette = [
        "#1976D2", "#43A047", "#FB8C00", "#E91E63", "#9C27B0",
        "#00BCD4", "#FF5722", "#795548", "#3F51B5", "#FFC107",
        "#009688", "#673AB7", "#FF9800", "#4CAF50", "#F44336",
    ]
    return palette[hash(kind) % len(palette)]
