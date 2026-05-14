"""6 個 indicator core subplot helpers:macd / rsi / kd / adx / atr / obv。

每個 helper 接 (fig, indicator, row=N) 把該 indicator 的 series 加進指定 row。
對齊 plan §「charts/indicators.py」。
"""

from __future__ import annotations

from typing import Any

import plotly.graph_objects as go

from dashboards.charts._base import (
    PALETTE,
    add_horizontal_reference,
    coerce_date,
    extract_series,
)


# ────────────────────────────────────────────────────────────
# MACD — 3 trace:line / signal / histogram
# ────────────────────────────────────────────────────────────

def add_macd_subplot(
    fig: go.Figure,
    macd_indicator: dict[str, Any] | None,
    *,
    row: int,
) -> None:
    """series point: macd_line / signal_line / histogram"""
    series = extract_series(macd_indicator)
    if not series:
        return
    dates = [coerce_date(p["date"]) for p in series if "date" in p]
    macd_line = [p.get("macd_line") for p in series if "date" in p]
    signal = [p.get("signal_line") for p in series if "date" in p]
    histogram = [p.get("histogram") for p in series if "date" in p]

    # Histogram(bar with up/down color)
    hist_colors = [
        PALETTE["macd_hist_up"] if (h or 0) >= 0 else PALETTE["macd_hist_down"]
        for h in histogram
    ]
    fig.add_trace(
        go.Bar(
            x=dates,
            y=histogram,
            name="MACD hist",
            marker_color=hist_colors,
            opacity=0.6,
        ),
        row=row,
        col=1,
    )
    fig.add_trace(
        go.Scatter(
            x=dates,
            y=macd_line,
            name="MACD",
            mode="lines",
            line=dict(color=PALETTE["macd_line"], width=1.5),
        ),
        row=row,
        col=1,
    )
    fig.add_trace(
        go.Scatter(
            x=dates,
            y=signal,
            name="Signal",
            mode="lines",
            line=dict(color=PALETTE["macd_signal"], width=1.5, dash="dot"),
        ),
        row=row,
        col=1,
    )
    fig.update_yaxes(title_text="MACD", row=row, col=1)


# ────────────────────────────────────────────────────────────
# RSI — 1 line + 70/30 reference
# ────────────────────────────────────────────────────────────

def add_rsi_subplot(
    fig: go.Figure,
    rsi_indicator: dict[str, Any] | None,
    *,
    row: int,
) -> None:
    """series point: value (RSI value 0-100)"""
    series = extract_series(rsi_indicator)
    if not series:
        return
    dates = [coerce_date(p["date"]) for p in series if "date" in p]
    values = [p.get("value") for p in series if "date" in p]

    fig.add_trace(
        go.Scatter(
            x=dates,
            y=values,
            name="RSI",
            mode="lines",
            line=dict(color=PALETTE["rsi"], width=1.5),
        ),
        row=row,
        col=1,
    )
    # 70 / 30 reference
    add_horizontal_reference(fig, 70, row=row, color=PALETTE["rsi_ref"], annotation="70")
    add_horizontal_reference(fig, 30, row=row, color=PALETTE["rsi_ref"], annotation="30")
    fig.update_yaxes(title_text="RSI", row=row, col=1, range=[0, 100])


# ────────────────────────────────────────────────────────────
# KD — k + d 兩線 + 80/20 reference
# ────────────────────────────────────────────────────────────

def add_kd_subplot(
    fig: go.Figure,
    kd_indicator: dict[str, Any] | None,
    *,
    row: int,
) -> None:
    """series point: k / d"""
    series = extract_series(kd_indicator)
    if not series:
        return
    dates = [coerce_date(p["date"]) for p in series if "date" in p]
    k_vals = [p.get("k") for p in series if "date" in p]
    d_vals = [p.get("d") for p in series if "date" in p]

    fig.add_trace(
        go.Scatter(
            x=dates, y=k_vals, name="K", mode="lines",
            line=dict(color=PALETTE["kd_k"], width=1.5),
        ),
        row=row, col=1,
    )
    fig.add_trace(
        go.Scatter(
            x=dates, y=d_vals, name="D", mode="lines",
            line=dict(color=PALETTE["kd_d"], width=1.5, dash="dot"),
        ),
        row=row, col=1,
    )
    add_horizontal_reference(fig, 80, row=row, color=PALETTE["rsi_ref"], annotation="80")
    add_horizontal_reference(fig, 20, row=row, color=PALETTE["rsi_ref"], annotation="20")
    fig.update_yaxes(title_text="KD", row=row, col=1, range=[0, 100])


# ────────────────────────────────────────────────────────────
# ADX — adx / plus_di / minus_di 三線
# ────────────────────────────────────────────────────────────

def add_adx_subplot(
    fig: go.Figure,
    adx_indicator: dict[str, Any] | None,
    *,
    row: int,
) -> None:
    """series point: adx / plus_di / minus_di"""
    series = extract_series(adx_indicator)
    if not series:
        return
    dates = [coerce_date(p["date"]) for p in series if "date" in p]
    adx_vals = [p.get("adx") for p in series if "date" in p]
    plus_di = [p.get("plus_di") for p in series if "date" in p]
    minus_di = [p.get("minus_di") for p in series if "date" in p]

    fig.add_trace(
        go.Scatter(
            x=dates, y=adx_vals, name="ADX", mode="lines",
            line=dict(color=PALETTE["adx"], width=1.8),
        ),
        row=row, col=1,
    )
    fig.add_trace(
        go.Scatter(
            x=dates, y=plus_di, name="+DI", mode="lines",
            line=dict(color=PALETTE["plus_di"], width=1.2),
        ),
        row=row, col=1,
    )
    fig.add_trace(
        go.Scatter(
            x=dates, y=minus_di, name="-DI", mode="lines",
            line=dict(color=PALETTE["minus_di"], width=1.2),
        ),
        row=row, col=1,
    )
    add_horizontal_reference(fig, 25, row=row, color=PALETTE["rsi_ref"], annotation="25")
    fig.update_yaxes(title_text="ADX", row=row, col=1)


# ────────────────────────────────────────────────────────────
# ATR — atr + atr_pct(twin axis 簡化:都畫 primary,因 ATR 與 ATR% 量級 OK 共圖)
# ────────────────────────────────────────────────────────────

def add_atr_subplot(
    fig: go.Figure,
    atr_indicator: dict[str, Any] | None,
    *,
    row: int,
) -> None:
    """series point: atr / atr_pct"""
    series = extract_series(atr_indicator)
    if not series:
        return
    dates = [coerce_date(p["date"]) for p in series if "date" in p]
    atr_vals = [p.get("atr") for p in series if "date" in p]

    fig.add_trace(
        go.Scatter(
            x=dates, y=atr_vals, name="ATR", mode="lines",
            line=dict(color=PALETTE["atr"], width=1.5),
        ),
        row=row, col=1,
    )
    fig.update_yaxes(title_text="ATR", row=row, col=1)


# ────────────────────────────────────────────────────────────
# OBV — obv + obv_ma
# ────────────────────────────────────────────────────────────

def add_obv_subplot(
    fig: go.Figure,
    obv_indicator: dict[str, Any] | None,
    *,
    row: int,
) -> None:
    """series point: obv / obv_ma"""
    series = extract_series(obv_indicator)
    if not series:
        return
    dates = [coerce_date(p["date"]) for p in series if "date" in p]
    obv_vals = [p.get("obv") for p in series if "date" in p]
    obv_ma = [p.get("obv_ma") for p in series if "date" in p]

    fig.add_trace(
        go.Scatter(
            x=dates, y=obv_vals, name="OBV", mode="lines",
            line=dict(color=PALETTE["obv"], width=1.5),
        ),
        row=row, col=1,
    )
    if any(v is not None for v in obv_ma):
        fig.add_trace(
            go.Scatter(
                x=dates, y=obv_ma, name="OBV MA", mode="lines",
                line=dict(color=PALETTE["obv_ma"], width=1.2, dash="dot"),
            ),
            row=row, col=1,
        )
    fig.update_yaxes(title_text="OBV", row=row, col=1)
