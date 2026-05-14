"""Render tools — Plotly Figure → PNG image,包 dashboards/charts/ 6 個 tab。

對齊 plan Phase D §Tool surface(Render tools)。

每個 tool 回 list[Image, dict]:
- Image:PNG bytes(Desktop 直接 inline 顯示)
- dict:summary text(facts_count / 主要 indicator latest 值 等),純文字 fallback

Tools:
- render_kline:K-line + bollinger + MA + neely + 動態 indicator subplots + facts markers
- render_chip:institutional / margin / foreign_holding / day_trading / shareholder 5-row
- render_fundamental:revenue / valuation / financial sub-view
- render_environment:taiex / us_market / global 三 view
- render_neely:scenario forest deep-dive(scenario_idx 可選)
- render_facts_cloud:facts 散點圖(x=date, y=source_core, color=kind)
"""

from __future__ import annotations

from datetime import date as Date
from typing import Any

from fastmcp.utilities.types import Image

from mcp_server._image import figure_to_png


def _parse_date(value: str | Date) -> Date:
    if isinstance(value, Date):
        return value
    return Date.fromisoformat(value)


def _png_image(fig, *, width: int = 1280, height: int = 800) -> Image:
    """Plotly Figure → fastmcp Image(PNG)。"""
    data = figure_to_png(fig, width=width, height=height)
    return Image(data=data, format="png")


def _fetch_snapshot_and_ohlc(
    stock_id: str,
    as_of_date: Date,
    *,
    lookback_days: int,
    include_market: bool = True,
):
    """共用:agg.as_of_with_ohlc 一次撈完。回 (snapshot_dict, ohlc_rows)。"""
    from agg import as_of_with_ohlc

    snapshot, ohlc = as_of_with_ohlc(
        stock_id,
        as_of_date,
        lookback_days=lookback_days,
        include_market=include_market,
    )
    return snapshot.to_dict(), ohlc


# ────────────────────────────────────────────────────────────
# Render tools
# ────────────────────────────────────────────────────────────


def render_kline(
    stock_id: str,
    date: str,
    lookback_days: int = 90,
    indicators: list[str] | None = None,
    with_volume: bool = True,
    show_bollinger: bool = True,
    show_ma: bool = True,
    show_neely_zigzag: bool = True,
    show_facts_markers: bool = True,
) -> list[Any]:
    """K-line PNG — candlestick + bollinger + MA + neely zigzag + facts markers。

    Args:
        stock_id: 股票代號(例 "2330")
        date: as_of 查詢日 ISO 字串(例 "2026-05-13")
        lookback_days: 期間天數。預設 90
        indicators: indicator subplots,擇 subset of
            ["macd", "rsi", "kd", "adx", "atr", "obv"]。
            預設 ["macd", "rsi", "kd"](對齊 Streamlit dashboard 預設)
        with_volume: 是否含 Volume 列。預設 True
        show_bollinger / show_ma / show_neely_zigzag / show_facts_markers: layer toggles

    Returns:
        [Image(PNG), summary_dict] —
          summary 含 stock_id / as_of / ohlc_days / facts_count / 主要 indicator latest
    """
    from dashboards.charts import (
        candlestick,
        facts_cloud,
        indicators as ind_module,
        overlays,
    )

    as_of_date = _parse_date(date)
    snapshot, ohlc = _fetch_snapshot_and_ohlc(
        stock_id, as_of_date, lookback_days=lookback_days,
    )
    indicators_dict = snapshot["indicator_latest"]
    structural_dict = snapshot["structural"]
    facts_list = snapshot["facts"]

    if not ohlc:
        # 空 figure + warning summary
        import plotly.graph_objects as go
        fig = go.Figure()
        fig.add_annotation(
            text=f"(無 price_daily_fwd 資料 for {stock_id} @ {as_of_date} -{lookback_days}d)",
            xref="paper", yref="paper",
            x=0.5, y=0.5, showarrow=False,
        )
        fig.update_layout(height=400, width=1280)
        return [
            _png_image(fig),
            {"stock_id": stock_id, "as_of": str(as_of_date), "warning": "no OHLC"},
        ]

    requested = indicators if indicators is not None else ["macd", "rsi", "kd"]
    # 對齊 dashboards/aggregation.py active_indicators (label, core_key) pattern
    _CORE_KEY = {
        "macd": "macd_core", "rsi": "rsi_core", "kd": "kd_core",
        "adx": "adx_core", "atr": "atr_core", "obv": "obv_core",
    }
    active = [(name.upper(), _CORE_KEY[name]) for name in requested if name in _CORE_KEY]
    n_ind = len(active)

    fig = candlestick.build_kline_figure(
        ohlc,
        n_indicator_subplots=n_ind,
        indicator_titles=[label for label, _ in active],
        with_volume=with_volume,
    )

    # Row 1 overlays
    if show_ma:
        overlays.add_ma_lines(fig, indicators_dict.get("ma_core@daily"))
    if show_bollinger:
        overlays.add_bollinger_band(fig, indicators_dict.get("bollinger_core@daily"))
    if show_neely_zigzag:
        overlays.add_neely_zigzag(
            fig, structural_dict.get("neely_core@daily"), show_fib_zones=False,
        )

    # Indicator subplots
    row_offset = 2 + (1 if with_volume else 0)
    actual_row = row_offset
    row_for_core: dict[str, int] = {}
    for label, core_key in active:
        ind = indicators_dict.get(f"{core_key}@daily")
        if   label == "MACD": ind_module.add_macd_subplot(fig, ind, row=actual_row)
        elif label == "RSI":  ind_module.add_rsi_subplot(fig, ind, row=actual_row)
        elif label == "KD":   ind_module.add_kd_subplot(fig, ind, row=actual_row)
        elif label == "ADX":  ind_module.add_adx_subplot(fig, ind, row=actual_row)
        elif label == "ATR":  ind_module.add_atr_subplot(fig, ind, row=actual_row)
        elif label == "OBV":  ind_module.add_obv_subplot(fig, ind, row=actual_row)
        row_for_core[core_key] = actual_row
        actual_row += 1

    # Facts markers
    if show_facts_markers and facts_list:
        facts_cloud.add_facts_to_kline(
            fig, facts_list, row_map=row_for_core, default_row=1,
        )

    # Height 隨 row 數動態調
    total_rows = 1 + (1 if with_volume else 0) + n_ind
    fig_height = 400 + total_rows * 110
    fig.update_layout(height=fig_height, width=1280)

    summary = {
        "stock_id": stock_id,
        "as_of": str(as_of_date),
        "lookback_days": lookback_days,
        "ohlc_days": len(ohlc),
        "facts_count": len(facts_list),
        "indicators_rendered": [label for label, _ in active],
        "latest_close": float(ohlc[-1]["close"]) if ohlc[-1].get("close") is not None else None,
    }
    return [_png_image(fig, height=fig_height), summary]


def render_chip(
    stock_id: str,
    date: str,
    lookback_days: int = 90,
) -> list[Any]:
    """籌碼 5-row PNG — institutional / margin / foreign_holding / day_trading / shareholder。

    facts markers 對應 5 cores → 5 rows。

    Args:
        stock_id: 股票代號
        date: as_of ISO 字串
        lookback_days: 期間天數。預設 90

    Returns:
        [Image, summary_dict]
    """
    from dashboards.charts import chip, facts_cloud

    as_of_date = _parse_date(date)
    snapshot, ohlc = _fetch_snapshot_and_ohlc(
        stock_id, as_of_date, lookback_days=lookback_days, include_market=False,
    )
    indicators_dict = snapshot["indicator_latest"]
    facts_list = snapshot["facts"]

    if not ohlc:
        import plotly.graph_objects as go
        fig = go.Figure()
        fig.add_annotation(text=f"(無 OHLC for {stock_id})", x=0.5, y=0.5,
                           xref="paper", yref="paper", showarrow=False)
        fig.update_layout(height=400, width=1280)
        return [_png_image(fig),
                {"stock_id": stock_id, "as_of": str(as_of_date), "warning": "no OHLC"}]

    fig = chip.build_chip_figure(
        ohlc,
        institutional=indicators_dict.get("institutional_core@daily"),
        margin=indicators_dict.get("margin_core@daily"),
        foreign_holding=indicators_dict.get("foreign_holding_core@daily"),
        day_trading=indicators_dict.get("day_trading_core@daily"),
        shareholder=indicators_dict.get("shareholder_core@weekly"),
    )

    chip_cores = {
        "institutional_core", "margin_core",
        "foreign_holding_core", "day_trading_core", "shareholder_core",
    }
    chip_facts = [f for f in facts_list if f.get("source_core") in chip_cores]
    if chip_facts:
        chip_row_map = {
            "institutional_core":   2,
            "margin_core":          3,
            "foreign_holding_core": 4,
            "day_trading_core":     5,
            "shareholder_core":     5,
        }
        facts_cloud.add_facts_to_kline(
            fig, chip_facts, row_map=chip_row_map, default_row=1,
        )
    fig.update_layout(height=900, width=1280)

    summary = {
        "stock_id":     stock_id,
        "as_of":        str(as_of_date),
        "ohlc_days":    len(ohlc),
        "chip_facts":   len(chip_facts),
        "cores_loaded": [c for c in chip_cores
                         if any(k.startswith(c) for k in indicators_dict.keys())],
    }
    return [_png_image(fig, height=900), summary]


def render_fundamental(
    stock_id: str,
    date: str,
    view: str = "revenue",
    lookback_days: int = 365,
) -> list[Any]:
    """基本面 PNG。

    Args:
        stock_id: 股票代號
        date: as_of ISO 字串
        view: 子 view,擇一 {"revenue", "valuation", "financial"}
        lookback_days: 預設 365(基本面變動較慢,給較長範圍)

    Returns:
        [Image, summary_dict]
    """
    from dashboards.charts import fundamental

    as_of_date = _parse_date(date)
    snapshot, _ohlc = _fetch_snapshot_and_ohlc(
        stock_id, as_of_date, lookback_days=lookback_days, include_market=False,
    )
    indicators_dict = snapshot["indicator_latest"]

    if view == "revenue":
        ind = (indicators_dict.get("revenue_core@monthly")
               or indicators_dict.get("revenue_core@daily"))
        fig = fundamental.build_revenue_chart(ind)
        title = "月營收 yoy / mom"
    elif view == "valuation":
        ind = indicators_dict.get("valuation_core@daily")
        fig = fundamental.build_valuation_chart(ind)
        title = "估值 percentile"
    elif view == "financial":
        ind = (indicators_dict.get("financial_statement_core@quarterly")
               or indicators_dict.get("financial_statement_core@daily"))
        fig, _rows = fundamental.build_financial_statement_view(ind)
        title = "財報季頻"
    else:
        raise ValueError(
            f"unknown view={view!r}; pick one of revenue / valuation / financial"
        )

    fig.update_layout(height=600, width=1280)
    summary = {
        "stock_id": stock_id,
        "as_of":    str(as_of_date),
        "view":     view,
        "title":    title,
        "core_has_data": ind is not None,
    }
    return [_png_image(fig, height=600), summary]


def render_environment(
    date: str,
    view: str = "taiex",
    lookback_days: int = 90,
) -> list[Any]:
    """環境面 PNG — TAIEX / US market / global view。

    走 market-level 保留字 stock_id(_index_taiex_ / _us_spy_ etc),透過 agg
    include_market=True 撈 5 個保留字 facts;indicator 直接從 indicator_latest 取。

    Args:
        date: as_of ISO 字串
        view: {"taiex", "us_market", "global"}
        lookback_days: 預設 90

    Returns:
        [Image, summary_dict]
    """
    from dashboards.charts import environment

    as_of_date = _parse_date(date)
    # market-level indicator 走任一個保留字 stock_id 撈就行,選 _index_taiex_
    snapshot, _ohlc = _fetch_snapshot_and_ohlc(
        "_index_taiex_", as_of_date,
        lookback_days=lookback_days, include_market=False,
    )
    indicators_dict = snapshot["indicator_latest"]

    if view == "taiex":
        fig = environment.build_taiex_chart(indicators_dict.get("taiex_core@daily"))
    elif view == "us_market":
        # us_market 走 _us_spy_ + _us_vix_ 保留字。建議 view 切到對應 stock_id
        snapshot_us, _ = _fetch_snapshot_and_ohlc(
            "_us_spy_", as_of_date,
            lookback_days=lookback_days, include_market=False,
        )
        fig = environment.build_us_market_chart(
            snapshot_us["indicator_latest"].get("us_market_core@daily")
        )
    elif view == "global":
        # 三合一:fear_greed + exchange_rate + market_margin + business_indicator
        import plotly.graph_objects as go
        from plotly.subplots import make_subplots

        fg_snap, _ = _fetch_snapshot_and_ohlc(
            "_market_fear_greed_", as_of_date,
            lookback_days=lookback_days, include_market=False,
        )
        mm_snap, _ = _fetch_snapshot_and_ohlc(
            "_market_margin_", as_of_date,
            lookback_days=lookback_days, include_market=False,
        )
        # 簡單拼接:Fear-Greed gauge 主圖,others 留 caller 想看的 view
        fig = environment.build_fear_greed_gauge(
            fg_snap["indicator_latest"].get("fear_greed_core@daily")
        )
    else:
        raise ValueError(
            f"unknown view={view!r}; pick one of taiex / us_market / global"
        )

    fig.update_layout(height=600, width=1280)
    summary = {
        "as_of":          str(as_of_date),
        "view":           view,
        "lookback_days":  lookback_days,
    }
    return [_png_image(fig, height=600), summary]


def render_neely(
    stock_id: str,
    date: str,
    scenario_idx: int = 0,
    lookback_days: int = 180,
    show_fib_zones: bool = True,
) -> list[Any]:
    """Neely Wave deep-dive PNG — K-line + 選定 scenario zigzag + Fib zones。

    Args:
        stock_id: 股票代號
        date: as_of ISO 字串
        scenario_idx: scenario_forest 內第幾個 scenario(從 0 開始)
        lookback_days: 預設 180(波浪結構要看較長範圍)
        show_fib_zones: 是否顯示 Fibonacci 反彈 / 推浪 zone(add_hrect)

    Returns:
        [Image, summary_dict]
    """
    from dashboards.charts import neely_wave

    as_of_date = _parse_date(date)
    snapshot, ohlc = _fetch_snapshot_and_ohlc(
        stock_id, as_of_date, lookback_days=lookback_days, include_market=False,
    )
    structural = snapshot["structural"].get("neely_core@daily")
    scenarios = neely_wave.list_scenarios(structural)

    fig = neely_wave.build_neely_deep_dive(
        ohlc, structural,
        scenario_idx=scenario_idx,
        show_fib_zones=show_fib_zones,
    )
    fig.update_layout(height=700, width=1280)

    summary = {
        "stock_id":       stock_id,
        "as_of":          str(as_of_date),
        "ohlc_days":      len(ohlc),
        "scenario_count": len(scenarios),
        "scenario_idx":   scenario_idx,
        "selected_scenario": scenarios[scenario_idx] if scenarios and 0 <= scenario_idx < len(scenarios) else None,
        "diagnostics":    neely_wave.render_diagnostics(structural),
    }
    return [_png_image(fig, height=700), summary]


def render_facts_cloud(
    stock_id: str,
    date: str,
    lookback_days: int = 90,
    source_cores: list[str] | None = None,
) -> list[Any]:
    """Facts 散點圖 PNG — x=fact_date, y=source_core(類目), color=kind。

    Args:
        stock_id: 股票代號
        date: as_of ISO 字串
        lookback_days: 預設 90
        source_cores: 限制 source_core 過濾;None=全部

    Returns:
        [Image, summary_dict] —
          summary 含 facts_count 及 facts 明細(若 ≤ 20 筆)
    """
    from dashboards.charts import facts_cloud as fc_module

    as_of_date = _parse_date(date)
    snapshot, _ohlc = _fetch_snapshot_and_ohlc(
        stock_id, as_of_date, lookback_days=lookback_days, include_market=False,
    )
    facts = snapshot["facts"]

    fig = fc_module.build_facts_scatter(
        facts,
        source_cores=source_cores,
        title=f"{stock_id} Facts 散點雲(as_of {as_of_date}, lookback {lookback_days}d)",
    )
    fig.update_layout(height=600, width=1280)

    # filter 後的列表
    if source_cores:
        filtered = [f for f in facts if f.get("source_core") in set(source_cores)]
    else:
        filtered = facts

    summary: dict[str, Any] = {
        "stock_id":       stock_id,
        "as_of":          str(as_of_date),
        "facts_count":    len(filtered),
        "filtered_cores": source_cores,
    }
    # 若 ≤ 20 筆,把明細塞進 summary(Claude 直接讀)
    if len(filtered) <= 20:
        summary["facts"] = [
            {
                "fact_date":   f.get("fact_date"),
                "source_core": f.get("source_core"),
                "statement":   f.get("statement"),
                "kind":        (f.get("metadata") or {}).get("kind"),
            }
            for f in filtered
        ]
    return [_png_image(fig, height=600), summary]
