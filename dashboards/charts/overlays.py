"""主圖 K-line 疊圖:bollinger band fill / MA 多線 / neely zigzag。

對齊 plan §「charts/overlays.py」。
"""

from __future__ import annotations

from typing import Any

import plotly.graph_objects as go

from dashboards.charts._base import (
    PALETTE,
    coerce_date,
    extract_series,
)


# ────────────────────────────────────────────────────────────
# MA — 多 spec line(SMA / EMA / WMA / DEMA / TEMA / HMA)
# ────────────────────────────────────────────────────────────

# ma_core spec → suggested color(剩餘走 ma_default)
_MA_COLOR_MAP = {
    "SMA20": PALETTE["ma20"],
    "SMA60": PALETTE["ma60"],
    "SMA200": PALETTE["ma200"],
    "EMA20": "#0288D1",
    "EMA60": "#F57C00",
    "EMA200": "#7B1FA2",
}


def add_ma_lines(
    fig: go.Figure,
    ma_indicator: dict[str, Any] | None,
    *,
    row: int = 1,
) -> None:
    """ma_core.value.series_by_spec → 多條 line 疊在 K-line 主圖。

    series_by_spec 結構:
      [
        {"spec": "SMA20", "series": [{date, value}, ...]},
        {"spec": "SMA60", "series": [...]},
        ...
      ]
    """
    if not ma_indicator:
        return
    value = ma_indicator.get("value")
    if not isinstance(value, dict):
        return

    series_by_spec = value.get("series_by_spec")
    if not isinstance(series_by_spec, list) or not series_by_spec:
        # Fallback:單一 series 結構(若 ma_core Output shape 不同)
        series = extract_series(ma_indicator)
        if series:
            _add_ma_single(fig, series, "MA", PALETTE["ma_default"], row=row)
        return

    for entry in series_by_spec:
        spec = entry.get("spec") or entry.get("name") or "MA"
        series = entry.get("series") or []
        if not series:
            continue
        color = _MA_COLOR_MAP.get(spec, PALETTE["ma_default"])
        _add_ma_single(fig, series, spec, color, row=row)


def _add_ma_single(
    fig: go.Figure,
    series: list[dict[str, Any]],
    name: str,
    color: str,
    *,
    row: int,
) -> None:
    dates = [coerce_date(p["date"]) for p in series if "date" in p]
    values = [p.get("value") for p in series if "date" in p]
    fig.add_trace(
        go.Scatter(
            x=dates,
            y=values,
            name=name,
            mode="lines",
            line=dict(color=color, width=1.5),
            opacity=0.85,
        ),
        row=row,
        col=1,
    )


# ────────────────────────────────────────────────────────────
# Bollinger band fill(upper / middle / lower 三線 + tonexty 填色)
# ────────────────────────────────────────────────────────────

def add_bollinger_band(
    fig: go.Figure,
    boll_indicator: dict[str, Any] | None,
    *,
    row: int = 1,
) -> None:
    """bollinger_core series → upper/middle/lower 三線 + 上下軌間填色。

    series point 欄位:date / upper_band / middle_band / lower_band / bandwidth / percent_b
    """
    series = extract_series(boll_indicator)
    if not series:
        return
    dates = [coerce_date(p["date"]) for p in series if "date" in p]
    uppers = [p.get("upper_band") for p in series if "date" in p]
    middles = [p.get("middle_band") for p in series if "date" in p]
    lowers = [p.get("lower_band") for p in series if "date" in p]

    # Upper(no fill,is the upper edge)
    fig.add_trace(
        go.Scatter(
            x=dates,
            y=uppers,
            name="Boll Upper",
            mode="lines",
            line=dict(color=PALETTE["bollinger_band"], width=1, dash="dot"),
            opacity=0.7,
        ),
        row=row,
        col=1,
    )
    # Lower(fill='tonexty' → 填到 upper 為止)
    fig.add_trace(
        go.Scatter(
            x=dates,
            y=lowers,
            name="Boll Lower",
            mode="lines",
            line=dict(color=PALETTE["bollinger_band"], width=1, dash="dot"),
            fill="tonexty",
            fillcolor=PALETTE["bollinger_fill"],
            opacity=0.7,
        ),
        row=row,
        col=1,
    )
    # Middle(SMA20 typical)
    fig.add_trace(
        go.Scatter(
            x=dates,
            y=middles,
            name="Boll Mid",
            mode="lines",
            line=dict(color=PALETTE["bollinger_mid"], width=1),
            opacity=0.6,
        ),
        row=row,
        col=1,
    )


# ────────────────────────────────────────────────────────────
# Neely zigzag — monowave_series 連線 + wave label annotation + Fib zone hrect
# ────────────────────────────────────────────────────────────

def add_neely_zigzag(
    fig: go.Figure,
    structural: dict[str, Any] | None,
    *,
    row: int = 1,
    show_labels: bool = True,
    show_fib_zones: bool = False,
) -> None:
    """neely structural snapshot:
       - monowave_series → zigzag 連線
       - wave label → annotation
       - expected_fib_zones → 淡色背景 hrect(scenario forest 第 1 個 scenario)
    """
    if not structural:
        return
    snapshot = structural.get("snapshot")
    if not isinstance(snapshot, dict):
        return

    monowaves = snapshot.get("monowave_series") or []
    if not monowaves:
        return

    # Zigzag:每個 monowave 用 (start, end) 兩個 endpoint 連線
    # Endpoint price 取 (low) for downward, (high) for upward;若無 direction 信息,
    # 用中位數 (high + low) / 2 fallback
    pts_x: list[Any] = []
    pts_y: list[float] = []

    for mw in monowaves:
        start = mw.get("start")
        end = mw.get("end")
        price_range = mw.get("price_range") or {}
        low = price_range.get("low")
        high = price_range.get("high")
        if start and low is not None and high is not None:
            # 第 1 個點:取 mid (兩端都有時)
            if not pts_x:
                pts_x.append(coerce_date(start))
                pts_y.append((float(low) + float(high)) / 2)
        if end and low is not None and high is not None:
            pts_x.append(coerce_date(end))
            pts_y.append((float(low) + float(high)) / 2)

    if pts_x:
        fig.add_trace(
            go.Scatter(
                x=pts_x,
                y=pts_y,
                name="Neely zigzag",
                mode="lines+markers",
                line=dict(color=PALETTE["neely_zigzag"], width=2),
                marker=dict(size=6, color=PALETTE["neely_zigzag"]),
                opacity=0.85,
            ),
            row=row,
            col=1,
        )

    if show_labels:
        for mw in monowaves:
            label = mw.get("label")
            end = mw.get("end")
            price_range = mw.get("price_range") or {}
            high = price_range.get("high")
            if label and end and high is not None:
                fig.add_annotation(
                    x=coerce_date(end),
                    y=float(high),
                    text=label,
                    showarrow=False,
                    font=dict(size=10, color=PALETTE["neely_label"]),
                    yshift=12,
                    bgcolor="rgba(255, 248, 225, 0.7)",
                    bordercolor=PALETTE["neely_zigzag"],
                    borderwidth=1,
                    row=row,
                    col=1,
                )

    if show_fib_zones:
        forest = snapshot.get("scenario_forest") or []
        if forest:
            top = forest[0] if isinstance(forest[0], dict) else {}
            zones = top.get("expected_fib_zones") or []
            for zone in zones:
                low = zone.get("low") or zone.get("price_low")
                high = zone.get("high") or zone.get("price_high")
                if low is not None and high is not None:
                    fig.add_hrect(
                        y0=float(low),
                        y1=float(high),
                        fillcolor=PALETTE["neely_fib_zone"],
                        line_width=0,
                        row=row,
                        col=1,
                    )
