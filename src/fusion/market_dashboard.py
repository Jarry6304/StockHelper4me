"""Fusion Layer · Integration 端口 — market_dashboard(D 視角)。

對齊 m3Spec/fusion_layer.md §4.2 / §8 + api_roadmap_v1.md §6.4.1。

讀 7 個 environment cores 寫進 `indicator_values` 的最新一筆,抽出各核心 headline
metric + `percentile_252` + 短期變化,組成大盤環境快照。**純資料** — 不打主觀標籤
(對齊 api_roadmap §6.2 零主觀規則),由 caller / LLM 自行判讀。
"""

from __future__ import annotations

from datetime import date
from typing import Any

from fusion.raw._db import fetch_indicator_latest, get_connection

# core_name → 寫入的保留字 stock_id(對齊各 env core 的 RESERVED 常數)
_CORE_RESERVED: dict[str, str] = {
    "taiex_core": "_index_taiex_",
    "us_market_core": "_index_us_market_",
    "exchange_rate_core": "_global_",
    "fear_greed_core": "_global_",
    "commodity_macro_core": "_global_",
    "market_margin_core": "_market_",
    "business_indicator_core": "_index_business_",
}


def market_dashboard(
    as_of: date,
    *,
    database_url: str | None = None,
    conn: Any = None,
) -> dict[str, Any]:
    """大盤環境快照 — 7 個 environment cores 的 headline metric。

    Args:
        as_of: 查詢日(各 core 取 `value_date <= as_of` 最新一筆)。
        database_url / conn: 連線。

    Returns:
        {as_of, component_count, components, missing}
        每個 component:{latest_date, value, change_pct, percentile_252, state, ...}
        某 core 無資料 → 進 `missing` list,不影響其他(graceful degradation)。
    """
    own_conn = conn is None
    if own_conn:
        conn = get_connection(database_url)
    components: dict[str, Any] = {}
    missing: list[str] = []
    try:
        for core, reserved in _CORE_RESERVED.items():
            point = _latest_point(conn, core, reserved, as_of)
            if point is None:
                missing.append(core)
            else:
                components[core] = _component(core, point)
    finally:
        if own_conn:
            conn.close()

    return {
        "as_of": as_of.isoformat(),
        "component_count": len(components),
        "components": components,
        "missing": missing,
    }


def _latest_point(conn: Any, core: str, reserved: str, as_of: date) -> dict[str, Any] | None:
    """撈 core 在 indicator_values 的最新一筆,從 value JSONB 取最後一個 series 點。"""
    rows = fetch_indicator_latest(conn, stock_id=reserved, as_of=as_of, cores=[core])
    if not rows:
        return None
    value = rows[0].get("value") or {}
    if core == "taiex_core":
        # TaiexOutput.series_by_index = [{index_code, series}, ...];取 Taiex 那條
        for entry in value.get("series_by_index") or []:
            if str(entry.get("index_code")) == "Taiex":
                series = entry.get("series") or []
                return series[-1] if series else None
        return None
    series = value.get("series") or []
    return series[-1] if series else None


def _component(core: str, point: dict[str, Any]) -> dict[str, Any]:
    """core 的最新 series 點 → 標準化 component dict。"""
    comp: dict[str, Any] = {
        "latest_date": point.get("date") or point.get("fact_date"),
        "percentile_252": point.get("percentile_252"),
    }
    if core == "taiex_core":
        comp.update(value=point.get("close"), change_pct=point.get("change_pct"),
                    state=point.get("trend_state"), rsi=point.get("rsi"))
    elif core == "us_market_core":
        comp.update(value=point.get("spy_close"), change_pct=point.get("spy_change_pct"),
                    state=point.get("vix_zone"), vix_close=point.get("vix_close"))
    elif core == "exchange_rate_core":
        comp.update(value=point.get("rate"), change_pct=point.get("change_pct"),
                    state=point.get("trend_state"), currency_pair=point.get("currency_pair"))
    elif core == "fear_greed_core":
        comp.update(value=point.get("value"), change_pct=None, state=point.get("zone"))
    elif core == "market_margin_core":
        comp.update(value=point.get("maintenance_rate"), change_pct=point.get("change_pct"),
                    state=point.get("zone"), margin_balance=point.get("margin_balance"))
    elif core == "business_indicator_core":
        comp.update(value=point.get("monitoring"), change_pct=None,
                    state=point.get("monitoring_color"),
                    leading=point.get("leading_indicator"),
                    coincident=point.get("coincident_indicator"))
    elif core == "commodity_macro_core":
        comp.update(value=point.get("price"), change_pct=point.get("return_pct"),
                    state=point.get("momentum_state"), commodity=point.get("commodity"),
                    return_z=point.get("return_z_score"))
    return comp
