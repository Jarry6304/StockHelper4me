"""Neely Wave deep-dive — scenario forest 多 scenario picker + 完整 wave 結構視覺化。

對齊 plan §「charts/neely_wave.py」+ explore 結果(snapshot 結構):
{
  "scenario_forest": [
    {"id": "scenario_0", "monowave_labels": [...], "expected_fib_zones": [...], "power_rating": "..."},
    ...
  ],
  "monowave_series": [
    {"start_date": "...", "end_date": "...", "start_price": ..., "end_price": ...,
     "direction": "Up"|"Down"|"Neutral", "bar_indices": [i0, i1], "label": "..."}
  ]
}

v4.33:Rust Monowave 序列化為 snake_case start_date/end_date/start_price/end_price
(無 serde rename);舊 dashboard 讀 start/end/price_range 對 production 全回 None →
zigzag 永遠畫空。已修(見 _base.neely_monowave_points)+ 新增 build_neely_forest_cloud。
"""

from __future__ import annotations

from datetime import date, timedelta
from typing import Any

import plotly.graph_objects as go

from dashboards.charts._base import PALETTE, coerce_date, neely_monowave_points
from fusion._fib_projection import (
    TIMEFRAME_FIB_RANGE,
    extract_invalidation_price,
    project_range,
)
from fusion._picker import effective_degree, power_rating_sign, power_rating_strength
from forecast.neely_emitter import _DEFAULT_HORIZON, _DEGREE_TO_HORIZON


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

    # Zigzag(全部 monowaves;對齊 overlays.add_neely_zigzag 同款邏輯,v4.33 欄位修復)
    pts_x, pts_y = neely_monowave_points(monowaves)

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
            end = mw.get("end_date")
            end_price = mw.get("end_price")
            if label and end is not None and end_price is not None:
                fig.add_annotation(
                    x=coerce_date(end),
                    y=float(end_price),
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


# ────────────────────────────────────────────────────────────
# v4.33 NeelyWave 複合雲圖 — 過去折線 + 未來價位密度雲 + 合成波型 + 失效 gauge
#                            + 可選雙軌共振疊層
# ────────────────────────────────────────────────────────────

_CLOUD_N_BINS = 60
_TOP_K_SYNTH = 20
# 繪圖固定三 horizon(對齊 _fib_projection.TIMEFRAME_FIB_RANGE)
_DRAW_HORIZONS: tuple[tuple[str, int], ...] = (("1m", 21), ("3m", 63), ("6m", 126))
_RESONANCE_COLOR = {"strong": "#D32F2F", "basic": "#FB8C00", "divergence": "#9E9E9E"}


def _pick_primary(forest: list[dict[str, Any]]) -> dict[str, Any] | None:
    """forest 取 primary scenario:power 強度 → rules_passed_count 降序。"""
    if not forest:
        return None
    scored = [s for s in forest if isinstance(s, dict)]
    if not scored:
        return None
    return max(
        scored,
        key=lambda s: (
            power_rating_strength(s.get("power_rating")),
            s.get("rules_passed_count") or 0,
        ),
    )


def _trading_days_ahead(anchor: date, n: int) -> date:
    """n 個交易日 → 約略日曆日(× 7/5);僅作雲 / 波型 x 錨點,非精確交易日。"""
    return anchor + timedelta(days=round(n * 7 / 5))


def _price_axis_range(
    ohlc: list[dict[str, Any]] | None,
    forest: list[dict[str, Any]],
    extra_ys: list[float],
) -> tuple[float, float] | None:
    """涵蓋歷史 OHLC + 全 per-scenario fib zones + 既有點的價格軸範圍。"""
    vals: list[float] = list(extra_ys)
    for r in ohlc or []:
        try:
            vals.append(float(r["low"]))
            vals.append(float(r["high"]))
        except (KeyError, TypeError, ValueError):
            continue
    for s in forest:
        for z in s.get("expected_fib_zones") or []:
            try:
                vals.append(float(z["low"]))
                vals.append(float(z["high"]))
            except (KeyError, TypeError, ValueError):
                continue
    vals = [v for v in vals if v > 0]
    if not vals:
        return None
    lo, hi = min(vals), max(vals)
    if hi <= lo:
        hi = lo * 1.05 + 1.0
    pad = (hi - lo) * 0.05
    return lo - pad, hi + pad


def _build_density_heat(
    forest: list[dict[str, Any]],
    p_lo: float,
    p_hi: float,
) -> tuple[list[float], bool]:
    """逐 scenario × power 加權 → 價位密度(沿價格軸)。回 (heat, any_zone)。"""
    heat = [0.0] * _CLOUD_N_BINS
    any_zone = False
    span = p_hi - p_lo
    if span <= 0:
        return heat, any_zone

    def _bin(price: float) -> int:
        idx = int((price - p_lo) / span * _CLOUD_N_BINS)
        return max(0, min(_CLOUD_N_BINS - 1, idx))

    for s in forest:
        w = power_rating_strength(s.get("power_rating"))
        if w <= 0:
            continue
        for z in s.get("expected_fib_zones") or []:
            try:
                lo, hi = float(z["low"]), float(z["high"])
            except (KeyError, TypeError, ValueError):
                continue
            any_zone = True
            lo_i, hi_i = _bin(min(lo, hi)), _bin(max(lo, hi))
            for i in range(lo_i, hi_i + 1):
                heat[i] += w
    return heat, any_zone


def _synth_path(
    fib_zones: list[dict[str, Any]],
    current_price: float,
    sign: int,
    anchor: date,
) -> tuple[list[date], list[float], list[float], list[float]]:
    """合成投影:回 (xs, median_ys, lower_ys, upper_ys)。xs[0]=anchor / current。"""
    xs: list[date] = [anchor]
    med: list[float] = [current_price]
    lo: list[float] = [current_price]
    hi: list[float] = [current_price]
    for tf, n in _DRAW_HORIZONS:
        ratio_lo, ratio_hi = TIMEFRAME_FIB_RANGE[tf]
        rl, rh = project_range(fib_zones, ratio_lo, ratio_hi, current_price, sign)
        if rl is None or rh is None:
            continue
        span_vals = [float(v) for v in (*rl, *rh)]
        env_lo, env_hi = min(span_vals), max(span_vals)
        if sign > 0:
            target = sum(float(v) for v in rh) / 2.0
        elif sign < 0:
            target = sum(float(v) for v in rl) / 2.0
        else:
            target = (env_lo + env_hi) / 2.0
        xs.append(_trading_days_ahead(anchor, n))
        med.append(target)
        lo.append(env_lo)
        hi.append(env_hi)
    return xs, med, lo, hi


def build_neely_forest_cloud(
    ohlc: list[dict[str, Any]] | None,
    structural: dict[str, Any] | None,
    *,
    current_price: float | None = None,
    resonance_result: dict[str, Any] | None = None,
    show_resonance: bool = True,
) -> go.Figure:
    """NeelyWave forest 複合雲圖。

    Layer 1 過去折線(monowave end_price zigzag + 淡 close backdrop)
    Layer 2 未來價位密度雲(逐 scenario × power 加權,禁 flat_fib_zones)
    Layer 3 未來合成波型(current → 各 horizon fib 中點,虛線標 synthesized)
    + 失效線 + 距失效 gauge
    + 可選雙軌共振疊層(resonance_result.to_dict();single_track_mode → 只畫 Track2)
    """
    fig = go.Figure()
    snapshot = (structural or {}).get("snapshot") if isinstance(structural, dict) else None
    forest = (snapshot or {}).get("scenario_forest") or []
    monowaves = (snapshot or {}).get("monowave_series") or []
    notes: list[str] = []

    if not snapshot or not forest:
        fig.add_annotation(
            text="(無 neely structural snapshot / scenario_forest)",
            xref="paper", yref="paper", x=0.5, y=0.95, showarrow=False,
            font=dict(color="gray", size=14),
        )
        fig.update_layout(height=640, title="NeelyWave 複合雲圖",
                          xaxis_rangeslider_visible=False,
                          plot_bgcolor="rgba(250,250,252,1)")
        return fig

    # ── 時間 / 價格錨點 ──
    zigzag_x, zigzag_y = neely_monowave_points(monowaves)
    if current_price is None:
        if ohlc:
            try:
                current_price = float(ohlc[-1]["close"])
            except (KeyError, TypeError, ValueError):
                current_price = None
        if current_price is None and zigzag_y:
            current_price = zigzag_y[-1]

    as_of: date | None = None
    if ohlc:
        try:
            as_of = coerce_date(ohlc[-1]["date"])
        except (KeyError, TypeError, ValueError):
            as_of = None
    if as_of is None and zigzag_x:
        as_of = zigzag_x[-1]

    # ── Layer 1:過去折線 ──
    if ohlc:
        try:
            fig.add_trace(go.Scatter(
                x=[coerce_date(r["date"]) for r in ohlc],
                y=[float(r["close"]) for r in ohlc],
                name="close", mode="lines",
                line=dict(color="rgba(120,120,120,0.35)", width=1),
                showlegend=False, hoverinfo="skip",
            ))
        except (KeyError, TypeError, ValueError):
            pass
    if zigzag_x:
        fig.add_trace(go.Scatter(
            x=zigzag_x, y=zigzag_y, name="Monowave zigzag",
            mode="lines+markers",
            line=dict(color=PALETTE["neely_zigzag"], width=2.5),
            marker=dict(size=6, color=PALETTE["neely_zigzag"]),
            opacity=0.9,
        ))
    else:
        notes.append("monowave_series 空 — 過去折線跳過")

    primary = _pick_primary(forest)

    # ── Layer 2:未來價位密度雲 ──
    axis_range = _price_axis_range(ohlc, forest, zigzag_y + ([current_price] if current_price else []))
    anchor_dates = (
        [_trading_days_ahead(as_of, n) for _, n in _DRAW_HORIZONS]
        if as_of is not None else []
    )
    if axis_range and as_of is not None:
        p_lo, p_hi = axis_range
        heat, any_zone = _build_density_heat(forest, p_lo, p_hi)
        if not any_zone:
            notes.append("未來密度雲稀疏:所有 scenario expected_fib_zones 為空(不退 flat 假裝密集)")
        else:
            step = (p_hi - p_lo) / _CLOUD_N_BINS
            bin_centers = [p_lo + (i + 0.5) * step for i in range(_CLOUD_N_BINS)]
            # 各 horizon 錨點同密度(fib 無時間,不沿時間漸變)
            z_grid = [[(heat[i] if heat[i] > 0 else None) for _ in anchor_dates]
                      for i in range(_CLOUD_N_BINS)]
            fig.add_trace(go.Heatmap(
                x=anchor_dates, y=bin_centers, z=z_grid,
                colorscale="Blues", showscale=True, zmin=0,
                opacity=0.55, name="未來密度雲",
                colorbar=dict(title="density", len=0.4, y=0.8),
                hovertemplate="價位 %{y:.1f}<br>density %{z:.1f}<extra></extra>",
            ))

    # ── Layer 3:未來合成波型(誠實標 synthesized)──
    if primary is not None and current_price and current_price > 0 and as_of is not None:
        sign = power_rating_sign(primary.get("power_rating"))
        fib_zones = primary.get("expected_fib_zones") or []
        xs, med, lo, hi = _synth_path(fib_zones, current_price, sign, as_of)
        if len(xs) > 1:
            # envelope(lower 先,upper fill='tonexty')
            fig.add_trace(go.Scatter(
                x=xs, y=lo, mode="lines", name="proj lower",
                line=dict(width=0), showlegend=False, hoverinfo="skip",
            ))
            fig.add_trace(go.Scatter(
                x=xs, y=hi, mode="lines", name="proj envelope",
                line=dict(width=0), fill="tonexty",
                fillcolor="rgba(255,193,7,0.12)", showlegend=False, hoverinfo="skip",
            ))
            fig.add_trace(go.Scatter(
                x=xs, y=med, mode="lines+markers",
                name="synthesized projection",
                line=dict(color=PALETTE["neely_zigzag"], width=2, dash="dash"),
                marker=dict(size=7, symbol="diamond"),
            ))
            fig.add_annotation(
                x=xs[-1], y=med[-1], text="synthesized projection（非 Neely 算出）",
                showarrow=True, arrowhead=2, font=dict(size=10, color="#8D6E63"),
                bgcolor="rgba(255,248,225,0.85)", bordercolor=PALETTE["neely_zigzag"],
                borderwidth=1, ax=0, ay=-28,
            )

        # top-K scenario 細線
        ranked = sorted(
            (s for s in forest if isinstance(s, dict)),
            key=lambda s: power_rating_strength(s.get("power_rating")),
            reverse=True,
        )[:_TOP_K_SYNTH]
        for s in ranked:
            zs = s.get("expected_fib_zones") or []
            if not zs:
                continue
            sx, sm, _, _ = _synth_path(zs, current_price, power_rating_sign(s.get("power_rating")), as_of)
            if len(sx) > 1:
                fig.add_trace(go.Scatter(
                    x=sx, y=sm, mode="lines",
                    line=dict(color=PALETTE["neely_zigzag"], width=0.8),
                    opacity=0.15, showlegend=False, hoverinfo="skip",
                ))

    # ── 失效線 + 距失效 gauge ──
    if primary is not None and current_price and current_price > 0:
        inv = extract_invalidation_price(primary, current_price)
        if inv is not None:
            buffer_pct = (current_price - inv) / current_price * 100
            fig.add_hline(
                y=inv, line=dict(color="#C62828", dash="dot", width=1.5),
                annotation_text=f"失效 {inv:.1f}（距 {buffer_pct:+.1f}%）",
                annotation_position="right",
                annotation_font=dict(size=10, color="#C62828"),
            )

    # ── 雙軌共振疊層(可選)──
    if show_resonance:
        notes.extend(_overlay_resonance(fig, resonance_result, anchor_dates))

    if notes:
        fig.add_annotation(
            text="ℹ️ " + " ｜ ".join(notes),
            xref="paper", yref="paper", x=0.0, y=-0.12, showarrow=False,
            font=dict(size=9, color="gray"), xanchor="left",
        )

    pat = (primary or {}).get("pattern_type")
    deg = effective_degree(primary) if primary else None
    horizon = _DEGREE_TO_HORIZON.get(deg or "", _DEFAULT_HORIZON)
    fig.update_layout(
        height=720,
        title=f"NeelyWave 複合雲圖 — {len(forest)} scenarios｜primary degree={deg or '?'}（horizon {horizon}d）",
        xaxis_rangeslider_visible=False,
        hovermode="x unified",
        plot_bgcolor="rgba(250,250,252,1)",
        margin=dict(l=40, r=120, t=50, b=70),
    )
    return fig


def _overlay_resonance(
    fig: go.Figure,
    res: dict[str, Any] | None,
    anchor_dates: list[date],
) -> list[str]:
    """疊雙軌共振:Track2 多 horizon 帶 + fib 線 level 高亮。回 notes。"""
    if not res:
        return ["雙軌共振:無 forecast_log 資料 / 未啟用"]
    notes: list[str] = []
    single = bool(res.get("single_track_mode"))
    if single:
        notes.append("A-3 失效閘門:軌道一退場,僅顯示 Track2 統計帶")

    # horizon_days → anchor date(對齊 _DRAW_HORIZONS 21/63/126)
    hmap = {n: (anchor_dates[i] if i < len(anchor_dates) else None)
            for i, (_, n) in enumerate(_DRAW_HORIZONS)}
    track2 = res.get("track2") or {}
    horizons = track2.get("horizons") or {}
    for h_key, band in horizons.items():
        if not isinstance(band, dict):
            continue
        try:
            h = int(h_key)
        except (TypeError, ValueError):
            continue
        adate = hmap.get(h)
        if adate is None:
            continue
        lower, upper = band.get("lower"), band.get("upper")
        point = band.get("point")  # ⚠️ Track2Band 中位數欄叫 point,非 median
        if lower is None or upper is None:
            continue
        fig.add_trace(go.Scatter(
            x=[adate, adate], y=[float(lower), float(upper)],
            mode="lines", name=f"Track2 {h}d",
            line=dict(color="#5E35B1", width=6), opacity=0.45,
            hovertemplate=f"Track2 {h}d<br>[%{{y:.1f}}]<extra></extra>",
        ))
        if point is not None:
            fig.add_trace(go.Scatter(
                x=[adate], y=[float(point)], mode="markers",
                marker=dict(size=8, color="#5E35B1", symbol="line-ew-open"),
                showlegend=False, hoverinfo="skip",
            ))

    if not single:
        for f in res.get("findings") or []:
            fib_line = (f or {}).get("fib_line") or {}
            price = fib_line.get("price")
            level = (f or {}).get("level")
            if price is None:
                continue
            fig.add_hline(
                y=float(price),
                line=dict(color=_RESONANCE_COLOR.get(level, "#9E9E9E"), width=1, dash="dot"),
                opacity=0.6,
            )
    return notes


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
