"""K-line candlestick + Volume builders。

對齊 plan §「charts/candlestick.py」。
"""

from __future__ import annotations

from typing import Any

import plotly.graph_objects as go

from dashboards.charts._base import (
    PALETTE,
    coerce_date,
    extract_series,
    make_kline_subplots,
)


def build_kline_figure(
    ohlc: list[dict[str, Any]],
    *,
    n_indicator_subplots: int = 0,
    indicator_titles: list[str] | None = None,
    with_volume: bool = True,
    with_day_trading_secondary: bool = False,
) -> go.Figure:
    """建立 K-line 主 figure(候 add_* 函式接著疊圖)。

    Layout:
        row 1 (40-60%):  K-line candlestick(主圖)
        row 2 (10%):     Volume(若 with_volume)— with_day_trading_secondary 時 secondary y
        row 3+ :         indicator subplots(留空 caller 決定)

    Args:
        ohlc: list of {date, open, high, low, close, volume} ASC
        n_indicator_subplots: 額外要預留的 indicator subplot 列數
        indicator_titles: 各 indicator subplot 標題
        with_volume: 是否有 Volume 列
        with_day_trading_secondary: Volume 列是否加 day_trading_ratio secondary y axis

    Returns:
        plotly Figure(只含 K-line + Volume,indicator subplot 為空殼待 caller 填)
    """
    n_rows = 1 + (1 if with_volume else 0) + n_indicator_subplots

    # row_heights:K-line 主圖 ~50%,Volume ~10%,indicator 各 ~10%
    if n_rows == 1:
        row_heights = [1.0]
    else:
        kline_h = 0.55 if n_rows >= 4 else 0.6
        remaining = 1.0 - kline_h
        per_row = remaining / (n_rows - 1)
        row_heights = [kline_h] + [per_row] * (n_rows - 1)

    titles = ["K-line"]
    if with_volume:
        titles.append("Volume")
    if indicator_titles:
        titles.extend(indicator_titles)

    specs: list[list[dict]] = [[{"secondary_y": False}]]
    if with_volume:
        specs.append([{"secondary_y": with_day_trading_secondary}])
    for _ in range(n_indicator_subplots):
        specs.append([{"secondary_y": False}])

    fig = make_kline_subplots(
        n_rows=n_rows,
        row_heights=row_heights,
        subplot_titles=titles,
        specs=specs,
    )

    # Add candlestick
    if ohlc:
        dates = [coerce_date(r["date"]) for r in ohlc]
        opens = [float(r["open"]) for r in ohlc]
        highs = [float(r["high"]) for r in ohlc]
        lows = [float(r["low"]) for r in ohlc]
        closes = [float(r["close"]) for r in ohlc]

        fig.add_trace(
            go.Candlestick(
                x=dates,
                open=opens,
                high=highs,
                low=lows,
                close=closes,
                name="K-line",
                increasing_line_color=PALETTE["candle_up"],
                decreasing_line_color=PALETTE["candle_down"],
                showlegend=False,
            ),
            row=1,
            col=1,
        )

        if with_volume:
            volumes = [float(r["volume"] or 0) for r in ohlc]
            colors = [
                PALETTE["volume_up"] if c >= o else PALETTE["volume_down"]
                for c, o in zip(closes, opens)
            ]
            fig.add_trace(
                go.Bar(
                    x=dates,
                    y=volumes,
                    name="Volume",
                    marker_color=colors,
                    showlegend=False,
                ),
                row=2,
                col=1,
            )

    fig.update_yaxes(title_text="Price", row=1, col=1)
    if with_volume:
        fig.update_yaxes(title_text="Volume", row=2, col=1)
    return fig


def add_day_trading_overlay(
    fig: go.Figure,
    day_trading_indicator: dict[str, Any] | None,
    *,
    row: int = 2,
) -> None:
    """day_trading_core series 疊到 Volume 列(secondary y)。

    series point 欄位:date / day_trade_ratio / momentum / day_trade_volume / total_volume
    """
    series = extract_series(day_trading_indicator)
    if not series:
        return
    dates = [coerce_date(p["date"]) for p in series if "date" in p]
    ratios = [p.get("day_trade_ratio") for p in series if "date" in p]
    fig.add_trace(
        go.Scatter(
            x=dates,
            y=ratios,
            name="day_trade_ratio %",
            mode="lines",
            line=dict(color=PALETTE["day_trading_ratio"], width=1.5),
            yaxis=f"y{row}2",  # secondary y
        ),
        row=row,
        col=1,
        secondary_y=True,
    )
    fig.update_yaxes(title_text="DT %", row=row, col=1, secondary_y=True)
