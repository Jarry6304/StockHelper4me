"""Facts 散點雲(對齊 user 「星雲圖 = 散點雲」需求)。

兩種使用情境:
- build_facts_scatter():獨立 figure,x=fact_date / y=source_core / color=kind
- add_facts_to_kline():把 facts 標記疊到 K-line 主圖各 indicator subplot
"""

from __future__ import annotations

from typing import Any

import plotly.graph_objects as go

from dashboards.charts._base import (
    PALETTE,
    coerce_date,
    fact_color_by_kind,
)


# ────────────────────────────────────────────────────────────
# K-line 各 row 對應 source_core(讓 facts marker 標到對的 subplot)
# ────────────────────────────────────────────────────────────

# row index 對齊 build_kline_figure(with_volume=True, n_indicator_subplots=6):
#   row 1: K-line(主圖)
#   row 2: Volume
#   row 3: MACD
#   row 4: RSI
#   row 5: KD
#   row 6: ADX
#   row 7: ATR
#   row 8: OBV
KLINE_FACTS_ROW_MAP = {
    "macd_core":           3,
    "rsi_core":            4,
    "kd_core":             5,
    "adx_core":            6,
    "atr_core":            7,
    "obv_core":            8,
    # 其他(neely / bollinger / ma / chip / fund / env)→ row 1 主 K-line
}


def add_facts_to_kline(
    fig: go.Figure,
    facts: list[dict[str, Any]],
    *,
    row_map: dict[str, int] | None = None,
    default_row: int = 1,
    main_panel_max_y: float | None = None,
) -> None:
    """把 facts 標記到 K-line figure 對應 subplot。

    各 source_core 的 facts 在自己的 subplot 上方標 marker(避免 overlap K-line price)。

    Args:
        fig: build_kline_figure 回傳的 figure
        facts: AsOfSnapshot.facts list(dict 形式;to_dict 後)
        row_map: source_core → row index;預設 KLINE_FACTS_ROW_MAP
        default_row: 不在 row_map 內的 core 標到哪一 row
        main_panel_max_y: 主圖 K-line price 上限(facts marker y 取此 + 邊距)
    """
    row_map = row_map if row_map is not None else KLINE_FACTS_ROW_MAP
    if not facts:
        return

    # group facts by row
    by_row: dict[int, list[dict[str, Any]]] = {}
    for f in facts:
        sc = f.get("source_core") or ""
        row = row_map.get(sc, default_row)
        by_row.setdefault(row, []).append(f)

    for row, row_facts in by_row.items():
        xs = []
        ys = []
        colors = []
        texts = []
        for f in row_facts:
            try:
                xs.append(coerce_date(f["fact_date"]))
            except Exception:
                continue
            md = f.get("metadata") or {}
            kind = md.get("kind")
            colors.append(fact_color_by_kind(kind))
            statement = f.get("statement") or ""
            sc = f.get("source_core") or ""
            texts.append(f"<b>{statement}</b><br>core: {sc}<br>kind: {kind or '-'}")

            # y 位置:row 1 主圖用 main_panel_max_y(若提供),否則用 fact 的 metadata.value
            # 其他 row 用 metadata.value(indicator subplot 量級)
            md_value = md.get("value")
            try:
                ys.append(float(md_value)) if md_value is not None else ys.append(None)
            except (TypeError, ValueError):
                ys.append(None)

        # 對 y=None 的 marker 用 row-default y 補位
        if row == default_row and main_panel_max_y is not None:
            ys = [y if y is not None else main_panel_max_y for y in ys]

        if not xs:
            continue

        fig.add_trace(
            go.Scatter(
                x=xs,
                y=ys,
                name=f"facts row{row}",
                mode="markers",
                marker=dict(
                    size=8,
                    color=colors,
                    line=dict(width=1, color="white"),
                    symbol="diamond",
                ),
                text=texts,
                hovertemplate="%{text}<extra></extra>",
                showlegend=False,
            ),
            row=row,
            col=1,
        )


# ────────────────────────────────────────────────────────────
# 獨立 facts 散點雲 tab
# ────────────────────────────────────────────────────────────

def build_facts_scatter(
    facts: list[dict[str, Any]],
    *,
    source_cores: list[str] | None = None,
    title: str = "Facts 散點雲",
) -> go.Figure:
    """獨立 figure:x=fact_date, y=source_core(類目), color=kind, size=importance。

    Args:
        facts: AsOfSnapshot.facts list(dict)
        source_cores: 限制 cores;None = 全部
        title: figure title

    Returns:
        plotly Figure scatter
    """
    if source_cores:
        facts = [f for f in facts if f.get("source_core") in set(source_cores)]

    if not facts:
        fig = go.Figure()
        fig.add_annotation(
            text="(無 facts 資料)",
            xref="paper", yref="paper",
            x=0.5, y=0.5,
            showarrow=False,
            font=dict(size=14, color="gray"),
        )
        fig.update_layout(title=title, height=300)
        return fig

    # group by (source_core, kind) for legend grouping
    fig = go.Figure()
    by_kind: dict[str, list[dict[str, Any]]] = {}
    for f in facts:
        md = f.get("metadata") or {}
        kind = md.get("kind") or "(unspecified)"
        by_kind.setdefault(kind, []).append(f)

    for kind, items in sorted(by_kind.items()):
        xs = []
        ys = []
        texts = []
        sizes = []
        for f in items:
            try:
                xs.append(coerce_date(f["fact_date"]))
            except Exception:
                continue
            ys.append(f.get("source_core") or "")
            statement = f.get("statement") or ""
            md = f.get("metadata") or {}
            value = md.get("value")
            value_str = f"<br>value: {value}" if value is not None else ""
            texts.append(
                f"<b>{statement}</b><br>"
                f"core: {f.get('source_core')}<br>"
                f"date: {f.get('fact_date')}<br>"
                f"kind: {kind}{value_str}"
            )
            # size by absolute value(防呆 default=8)
            try:
                v = abs(float(value)) if value is not None else 0
                sizes.append(min(20, max(8, 8 + v / 10)))
            except (TypeError, ValueError):
                sizes.append(8)

        color = fact_color_by_kind(None if kind == "(unspecified)" else kind)
        fig.add_trace(
            go.Scatter(
                x=xs,
                y=ys,
                name=kind,
                mode="markers",
                marker=dict(
                    size=sizes,
                    color=color,
                    line=dict(width=1, color="white"),
                    opacity=0.75,
                ),
                text=texts,
                hovertemplate="%{text}<extra></extra>",
            ),
        )

    fig.update_layout(
        title=title,
        xaxis_title="fact_date",
        yaxis_title="source_core",
        hovermode="closest",
        height=600,
        legend=dict(
            orientation="v",
            yanchor="top",
            y=1.0,
            xanchor="left",
            x=1.02,
        ),
        margin=dict(l=160, r=180, t=60, b=40),
        plot_bgcolor="rgba(250, 250, 252, 1)",
    )
    return fig
