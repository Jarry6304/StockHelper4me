"""5 個 chip core subplot helpers + Tab 2 build_chip_figure。

對齊 plan §「charts/chip.py」+ explore 結果 chip cores Output 結構:
- institutional_core:foreign_net / trust_net / dealer_net / total_net / cumulative
- margin_core:margin_balance / short_balance / short_to_margin_ratio / margin_change_pct
- foreign_holding_core:foreign_holding_pct / foreign_limit_pct / remaining_pct / change_pct
- day_trading_core:day_trade_ratio / momentum / day_trade_volume / total_volume
- shareholder_core:concentration_index + 4 級距 holders count/pct(週頻)
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


# ────────────────────────────────────────────────────────────
# 5 個獨立 helper(各取一 subplot row)
# ────────────────────────────────────────────────────────────

def add_institutional_bars(
    fig: go.Figure,
    inst_indicator: dict[str, Any] | None,
    *,
    row: int,
) -> None:
    """三大法人 net buy 三色 stacked bar。"""
    series = extract_series(inst_indicator)
    if not series:
        return
    dates = [coerce_date(p["date"]) for p in series if "date" in p]
    foreign = [p.get("foreign_net") for p in series if "date" in p]
    trust = [p.get("trust_net") for p in series if "date" in p]
    dealer = [p.get("dealer_net") for p in series if "date" in p]

    fig.add_trace(
        go.Bar(x=dates, y=foreign, name="外資 net",
               marker_color=PALETTE["foreign_net"], opacity=0.85),
        row=row, col=1,
    )
    fig.add_trace(
        go.Bar(x=dates, y=trust, name="投信 net",
               marker_color=PALETTE["trust_net"], opacity=0.85),
        row=row, col=1,
    )
    fig.add_trace(
        go.Bar(x=dates, y=dealer, name="自營 net",
               marker_color=PALETTE["dealer_net"], opacity=0.85),
        row=row, col=1,
    )
    fig.update_yaxes(title_text="法人 net(股)", row=row, col=1)


def add_margin_panel(
    fig: go.Figure,
    margin_indicator: dict[str, Any] | None,
    *,
    row: int,
) -> None:
    """融資餘額 line + 融券餘額 line(共圖)。"""
    series = extract_series(margin_indicator)
    if not series:
        return
    dates = [coerce_date(p["date"]) for p in series if "date" in p]
    margin_bal = [p.get("margin_balance") for p in series if "date" in p]
    short_bal = [p.get("short_balance") for p in series if "date" in p]

    fig.add_trace(
        go.Scatter(x=dates, y=margin_bal, name="融資餘額", mode="lines",
                   line=dict(color="#1976D2", width=1.5)),
        row=row, col=1,
    )
    fig.add_trace(
        go.Scatter(x=dates, y=short_bal, name="融券餘額", mode="lines",
                   line=dict(color="#EF5350", width=1.5, dash="dot")),
        row=row, col=1,
    )
    fig.update_yaxes(title_text="融資/融券", row=row, col=1)


def add_foreign_holding(
    fig: go.Figure,
    fh_indicator: dict[str, Any] | None,
    *,
    row: int,
) -> None:
    """外資持股 % line + 上限 reference。"""
    series = extract_series(fh_indicator)
    if not series:
        return
    dates = [coerce_date(p["date"]) for p in series if "date" in p]
    holding = [p.get("foreign_holding_pct") for p in series if "date" in p]
    limit = [p.get("foreign_limit_pct") for p in series if "date" in p]

    fig.add_trace(
        go.Scatter(x=dates, y=holding, name="外資持股 %", mode="lines",
                   line=dict(color="#1976D2", width=1.8)),
        row=row, col=1,
    )
    if any(v is not None for v in limit):
        fig.add_trace(
            go.Scatter(x=dates, y=limit, name="持股上限 %", mode="lines",
                       line=dict(color="rgba(120,120,120,0.5)", width=1, dash="dash")),
            row=row, col=1,
        )
    fig.update_yaxes(title_text="外資 %", row=row, col=1)


def add_day_trading(
    fig: go.Figure,
    dt_indicator: dict[str, Any] | None,
    *,
    row: int,
) -> None:
    """當沖比 % line。"""
    series = extract_series(dt_indicator)
    if not series:
        return
    dates = [coerce_date(p["date"]) for p in series if "date" in p]
    ratio = [p.get("day_trade_ratio") for p in series if "date" in p]

    fig.add_trace(
        go.Scatter(x=dates, y=ratio, name="當沖比 %", mode="lines",
                   line=dict(color=PALETTE["day_trading_ratio"], width=1.5),
                   fill="tozeroy", fillcolor="rgba(233, 30, 99, 0.1)"),
        row=row, col=1,
    )
    fig.update_yaxes(title_text="當沖 %", row=row, col=1)


def add_shareholder(
    fig: go.Figure,
    sh_indicator: dict[str, Any] | None,
    *,
    row: int,
) -> None:
    """大戶/中戶/散戶持股級距 area chart + concentration line(secondary y)。"""
    series = extract_series(sh_indicator)
    if not series:
        return
    dates = [coerce_date(p["date"]) for p in series if "date" in p]

    # 4 級距 pct(若 series 有此 keys)
    levels = [
        ("super_large_holders_pct", "超大戶 %", "#9C27B0"),
        ("large_holders_pct",       "大戶 %",   "#1976D2"),
        ("mid_holders_pct",         "中戶 %",   "#43A047"),
        ("small_holders_pct",       "散戶 %",   "#FB8C00"),
    ]
    for key, name, color in levels:
        values = [p.get(key) for p in series if "date" in p]
        if any(v is not None for v in values):
            fig.add_trace(
                go.Scatter(x=dates, y=values, name=name, mode="lines",
                           line=dict(color=color, width=1.2),
                           stackgroup="holders",
                           groupnorm="percent"),
                row=row, col=1,
            )
    fig.update_yaxes(title_text="持股分佈 %", row=row, col=1)


# ────────────────────────────────────────────────────────────
# Tab 2 整合 figure builder
# ────────────────────────────────────────────────────────────

def build_chip_figure(
    ohlc: list[dict[str, Any]] | None,
    *,
    institutional=None,
    margin=None,
    foreign_holding=None,
    day_trading=None,
    shareholder=None,
) -> go.Figure:
    """Tab 2 Chip 整合 figure:5 row subplots(daily 對齊)。

    Row 1: mini K-line(reference)
    Row 2: institutional 三色 bar
    Row 3: margin balance + short
    Row 4: foreign_holding %
    Row 5: day_trading 比 + shareholder concentration(若有)
    """
    titles = ["K-line", "三大法人 net", "融資/融券", "外資持股 %", "當沖 / 持股分佈"]
    fig = make_kline_subplots(
        n_rows=5,
        row_heights=[0.30, 0.18, 0.18, 0.17, 0.17],
        subplot_titles=titles,
    )

    # Row 1: candlestick
    if ohlc:
        dates = [coerce_date(r["date"]) for r in ohlc]
        fig.add_trace(
            go.Candlestick(
                x=dates,
                open=[float(r["open"]) for r in ohlc],
                high=[float(r["high"]) for r in ohlc],
                low=[float(r["low"]) for r in ohlc],
                close=[float(r["close"]) for r in ohlc],
                name="K-line",
                increasing_line_color=PALETTE["candle_up"],
                decreasing_line_color=PALETTE["candle_down"],
                showlegend=False,
            ),
            row=1, col=1,
        )

    add_institutional_bars(fig, institutional, row=2)
    add_margin_panel(fig, margin, row=3)
    add_foreign_holding(fig, foreign_holding, row=4)

    # Row 5: 當沖 + shareholder(共 row 不同 trace)
    add_day_trading(fig, day_trading, row=5)
    add_shareholder(fig, shareholder, row=5)

    fig.update_layout(barmode="relative")  # institutional bars not stacked, side-by-side
    return fig
