"""Neely Wave deep-dive — scenario forest 多 scenario picker + 完整 wave 結構視覺化。

對齊 plan §「charts/neely_wave.py」+ explore 結果(snapshot 結構):
{
  "scenario_forest": [
    {"id": "scenario_0", "monowave_labels": [...], "expected_fib_zones": [...], "power_rating": "..."},
    ...
  ],
  "monowave_series": [
    {"idx": 0, "start": "...", "end": "...", "label": "...", "price_range": {"low": ..., "high": ...}}
  ]
}
"""

from __future__ import annotations

from typing import Any

import plotly.graph_objects as go

from dashboards.charts._base import PALETTE, coerce_date


def list_scenarios(structural: dict[str, Any] | None) -> list[dict[str, Any]]:
    """從 structural snapshot 取出所有 scenarios meta(供 selectbox 用)。

    Returns:
        list of {idx, id, label_preview, monowave_count, power_rating}
    """
    if not structural:
        return []
    snapshot = structural.get("snapshot")
    if not isinstance(snapshot, dict):
        return []
    forest = snapshot.get("scenario_forest") or []
    out: list[dict[str, Any]] = []
    for i, sc in enumerate(forest):
        if not isinstance(sc, dict):
            continue
        labels = sc.get("monowave_labels") or []
        out.append({
            "idx": i,
            "id": sc.get("id", f"scenario_{i}"),
            "label_preview": " · ".join(str(label) for label in labels[:5])
                             + ("…" if len(labels) > 5 else ""),
            "monowave_count": len(labels),
            "power_rating": sc.get("power_rating") or sc.get("rating") or "-",
        })
    return out


def build_neely_deep_dive(
    ohlc: list[dict[str, Any]] | None,
    structural: dict[str, Any] | None,
    *,
    scenario_idx: int = 0,
    show_fib_zones: bool = True,
) -> go.Figure:
    """K-line + 選定 scenario 的:
       - monowave_series 連線(zigzag)
       - wave label annotation
       - expected_fib_zones add_hrect
       - power_rating 右上角 metric
    """
    fig = go.Figure()

    # K-line base
    if ohlc:
        dates = [coerce_date(r["date"]) for r in ohlc]
        fig.add_trace(go.Candlestick(
            x=dates,
            open=[float(r["open"]) for r in ohlc],
            high=[float(r["high"]) for r in ohlc],
            low=[float(r["low"]) for r in ohlc],
            close=[float(r["close"]) for r in ohlc],
            name="K-line",
            increasing_line_color=PALETTE["candle_up"],
            decreasing_line_color=PALETTE["candle_down"],
            showlegend=False,
        ))

    if not structural:
        fig.add_annotation(
            text="(無 neely structural snapshot)",
            xref="paper", yref="paper",
            x=0.5, y=0.95, showarrow=False, font=dict(color="gray", size=14),
        )
        fig.update_layout(height=600, title="Neely Wave deep dive",
                          xaxis_rangeslider_visible=False,
                          plot_bgcolor="rgba(250,250,252,1)")
        return fig

    snapshot = structural.get("snapshot") or {}
    forest = snapshot.get("scenario_forest") or []
    monowaves = snapshot.get("monowave_series") or []

    # 選定 scenario(若 idx 越界 fallback to 0)
    scenario = None
    if forest and 0 <= scenario_idx < len(forest):
        scenario = forest[scenario_idx]

    # Zigzag(全部 monowaves;對齊 overlays.add_neely_zigzag 同款邏輯)
    pts_x: list[Any] = []
    pts_y: list[float] = []
    for mw in monowaves:
        start = mw.get("start")
        end = mw.get("end")
        price_range = mw.get("price_range") or {}
        low = price_range.get("low")
        high = price_range.get("high")
        if start and low is not None and high is not None:
            if not pts_x:
                pts_x.append(coerce_date(start))
                pts_y.append((float(low) + float(high)) / 2)
        if end and low is not None and high is not None:
            pts_x.append(coerce_date(end))
            pts_y.append((float(low) + float(high)) / 2)

    if pts_x:
        fig.add_trace(go.Scatter(
            x=pts_x, y=pts_y, name="Monowaves",
            mode="lines+markers",
            line=dict(color=PALETTE["neely_zigzag"], width=2.5),
            marker=dict(size=8, color=PALETTE["neely_zigzag"]),
            opacity=0.9,
        ))

    # Wave labels(僅選定 scenario 的)
    if scenario:
        scenario_labels = scenario.get("monowave_labels") or []
        # 對齊 monowave_series 與 scenario_labels(理想是 1:1)
        for i, mw in enumerate(monowaves):
            if i >= len(scenario_labels):
                break
            label = scenario_labels[i]
            end = mw.get("end")
            price_range = mw.get("price_range") or {}
            high = price_range.get("high")
            if label and end and high is not None:
                fig.add_annotation(
                    x=coerce_date(end),
                    y=float(high),
                    text=str(label),
                    showarrow=True,
                    arrowhead=2,
                    arrowsize=1,
                    arrowwidth=1,
                    arrowcolor=PALETTE["neely_zigzag"],
                    font=dict(size=11, color=PALETTE["neely_label"]),
                    ax=0, ay=-30,
                    bgcolor="rgba(255, 248, 225, 0.85)",
                    bordercolor=PALETTE["neely_zigzag"],
                    borderwidth=1,
                )

        # Fib zones
        if show_fib_zones:
            zones = scenario.get("expected_fib_zones") or []
            for z in zones:
                low = z.get("low") or z.get("price_low")
                high = z.get("high") or z.get("price_high")
                if low is not None and high is not None:
                    fig.add_hrect(
                        y0=float(low), y1=float(high),
                        fillcolor=PALETTE["neely_fib_zone"],
                        line_width=0,
                        annotation_text=z.get("label", "Fib"),
                        annotation_position="left",
                    )

    # Power rating annotation
    if scenario:
        power = scenario.get("power_rating") or scenario.get("rating") or "-"
        sid = scenario.get("id", f"scenario_{scenario_idx}")
        fig.add_annotation(
            xref="paper", yref="paper",
            x=0.99, y=0.98,
            text=f"<b>{sid}</b><br>Power: {power}",
            showarrow=False,
            font=dict(size=12),
            bgcolor="rgba(255, 255, 255, 0.85)",
            bordercolor=PALETTE["neely_zigzag"],
            borderwidth=1,
            xanchor="right", yanchor="top",
        )

    fig.update_layout(
        height=700,
        title=f"Neely Wave deep dive — scenario {scenario_idx} of {len(forest)}",
        xaxis_rangeslider_visible=False,
        hovermode="x unified",
        plot_bgcolor="rgba(250,250,252,1)",
    )
    return fig


def render_diagnostics(structural: dict[str, Any] | None) -> dict[str, Any]:
    """從 structural snapshot 抽 diagnostics info(forest_size / elapsed_ms / rejections 等)。

    Returns:
        dict 形式給 streamlit st.json / st.metric 用
    """
    if not structural:
        return {}
    snapshot = structural.get("snapshot") or {}
    diag = snapshot.get("diagnostics") or {}
    forest = snapshot.get("scenario_forest") or []
    monowaves = snapshot.get("monowave_series") or []
    return {
        "forest_size": len(forest),
        "monowave_count": len(monowaves),
        **{k: v for k, v in diag.items() if not isinstance(v, (list, dict))},
        "rejections": diag.get("rejections", []),
        "stage_elapsed_ms": diag.get("stage_elapsed_ms", {}),
    }
