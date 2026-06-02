"""Traditional Core(Frost & Prechter EWP)波浪 deep-dive — 獨立 vertical,與 Neely 並排。

讀 `traditional_snapshots.forest`(整個 TraditionalCoreOutput);picker 依 `preference_score`
(**forest 不選 primary**)。渲染複製 neely_wave 風格(zigzag + fib zones),不抽共用(對齊
SPEC「容許重複,不抽象」)。

forest_doc JSON 結構(自有,異於 neely):
{
  "pivot_series": [{bar_index, date, price, kind}],
  "scenario_forest": [
    {"id", "structure_label", "pattern_type", "direction", "degree", "preference_score",
     "wave_tree": {"label","start","end","start_price","end_price","children":[{label,start,end,...}]},
     "expected_fib_zones": [{label, low, high, source_ratio}], "invalidation_triggers": [...]}
  ],
  "diagnostics": {...}
}
"""

from __future__ import annotations

from typing import Any

import plotly.graph_objects as go

from dashboards.charts._base import PALETTE, coerce_date


def fetch_traditional_forest(
    stock_id: str,
    timeframe: str = "daily",
    *,
    database_url: str | None = None,
) -> dict[str, Any] | None:
    """讀 traditional_snapshots 最新 forest dict(latest per (stock, timeframe))。None = 無 row。"""
    from fusion.raw._db import get_connection

    conn = get_connection(database_url)
    try:
        with conn.cursor() as cur:
            cur.execute(
                "SELECT forest FROM traditional_snapshots "
                "WHERE stock_id = %s AND timeframe = %s "
                "ORDER BY computed_at DESC LIMIT 1",
                [stock_id, timeframe],
            )
            row = cur.fetchone()
    finally:
        conn.close()
    return row["forest"] if row else None


def list_scenarios(forest_doc: dict[str, Any] | None) -> list[dict[str, Any]]:
    """forest_doc → scenarios meta(供 selectbox;依 forest 順序 = preference_score 降序)。"""
    if not isinstance(forest_doc, dict):
        return []
    forest = forest_doc.get("scenario_forest") or []
    out: list[dict[str, Any]] = []
    for i, sc in enumerate(forest):
        if not isinstance(sc, dict):
            continue
        out.append(
            {
                "idx": i,
                "id": sc.get("id", f"sc_{i}"),
                "structure_label": sc.get("structure_label", "-"),
                "preference_score": sc.get("preference_score", 0),
                "degree": sc.get("degree", "-"),
            }
        )
    return out


def _wave_tree_points(wave_tree: dict[str, Any] | None) -> tuple[list[Any], list[float]]:
    """wave_tree.children → zigzag 折線 (xs, ys)。N children → N+1 點。"""
    children = (wave_tree or {}).get("children") or []
    xs: list[Any] = []
    ys: list[float] = []
    for ch in children:
        s_d, s_p = ch.get("start"), ch.get("start_price")
        e_d, e_p = ch.get("end"), ch.get("end_price")
        if not xs and s_d is not None and s_p is not None:
            xs.append(coerce_date(s_d))
            ys.append(float(s_p))
        if e_d is not None and e_p is not None:
            xs.append(coerce_date(e_d))
            ys.append(float(e_p))
    return xs, ys


def build_traditional_deep_dive(
    ohlc: list[dict[str, Any]] | None,
    forest_doc: dict[str, Any] | None,
    *,
    scenario_idx: int = 0,
    show_fib_zones: bool = True,
) -> go.Figure:
    """K-line + pivot skeleton + 選定 scenario 的 wave_tree zigzag + labels + fib zones。"""
    fig = go.Figure()

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
            )
        )

    if not isinstance(forest_doc, dict):
        fig.add_annotation(
            text="(無 traditional snapshot)",
            xref="paper",
            yref="paper",
            x=0.5,
            y=0.95,
            showarrow=False,
            font=dict(color="gray", size=14),
        )
        fig.update_layout(
            height=600,
            title="Traditional Wave deep dive",
            xaxis_rangeslider_visible=False,
            plot_bgcolor="rgba(250,250,252,1)",
        )
        return fig

    # pivot skeleton(全 pivot_series — 灰虛線)
    pivots = forest_doc.get("pivot_series") or []
    px = [coerce_date(p["date"]) for p in pivots if p.get("date") is not None]
    py = [float(p["price"]) for p in pivots if p.get("price") is not None]
    if px:
        fig.add_trace(
            go.Scatter(
                x=px,
                y=py,
                name="Pivots",
                mode="lines+markers",
                line=dict(color="rgba(120,120,120,0.5)", width=1, dash="dot"),
                marker=dict(size=5, color="rgba(120,120,120,0.6)"),
            )
        )

    forest = forest_doc.get("scenario_forest") or []
    scenario = forest[scenario_idx] if forest and 0 <= scenario_idx < len(forest) else None

    if scenario:
        wt = scenario.get("wave_tree") or {}
        xs, ys = _wave_tree_points(wt)
        if xs:
            fig.add_trace(
                go.Scatter(
                    x=xs,
                    y=ys,
                    name=scenario.get("structure_label", "scenario"),
                    mode="lines+markers",
                    line=dict(color=PALETTE["neely_zigzag"], width=2.5),
                    marker=dict(size=8, color=PALETTE["neely_zigzag"]),
                )
            )
            for ch in wt.get("children") or []:
                lab, e_d, e_p = ch.get("label"), ch.get("end"), ch.get("end_price")
                if lab and e_d is not None and e_p is not None:
                    fig.add_annotation(
                        x=coerce_date(e_d),
                        y=float(e_p),
                        text=str(lab),
                        showarrow=True,
                        arrowhead=2,
                        arrowsize=1,
                        arrowwidth=1,
                        arrowcolor=PALETTE["neely_zigzag"],
                        font=dict(size=11, color=PALETTE["neely_label"]),
                        ax=0,
                        ay=-30,
                        bgcolor="rgba(255,248,225,0.85)",
                        bordercolor=PALETTE["neely_zigzag"],
                        borderwidth=1,
                    )
        if show_fib_zones:
            for z in scenario.get("expected_fib_zones") or []:
                low, high = z.get("low"), z.get("high")
                if low is not None and high is not None:
                    fig.add_hrect(
                        y0=float(low),
                        y1=float(high),
                        fillcolor=PALETTE["neely_fib_zone"],
                        line_width=0,
                        annotation_text=z.get("label", "Fib"),
                        annotation_position="left",
                    )
        fig.add_annotation(
            xref="paper",
            yref="paper",
            x=0.99,
            y=0.98,
            text=(
                f"{scenario.get('structure_label', '-')}<br>"
                f"degree={scenario.get('degree', '-')} · "
                f"pref={scenario.get('preference_score', 0)}"
            ),
            showarrow=False,
            align="right",
            font=dict(size=12, color=PALETTE["neely_label"]),
            bgcolor="rgba(255,255,255,0.85)",
            bordercolor=PALETTE["neely_zigzag"],
            borderwidth=1,
        )

    fig.update_layout(
        height=640,
        title="Traditional Wave deep dive(Frost & Prechter EWP · 並排不整合)",
        xaxis_rangeslider_visible=False,
        plot_bgcolor="rgba(250,250,252,1)",
        hovermode="x unified",
    )
    return fig
