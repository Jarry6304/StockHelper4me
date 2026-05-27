"""Fusion Layer · Integration 端口 — indicator_assembly(E 視角)。

對齊 m3Spec/fusion_layer.md §5 + api_roadmap_v1.md §七。

為一組 indicator cores 組裝 series(從 indicator_values 讀)+ events(從 facts
讀),對齊 cores §7.5 的 `{series, events}` 同構結構。E 視角 5 個 MCP 工具
(indicator_momentum / volatility / volume / pattern / stack)共用本模組。

只讀 pre-computed indicator_values + facts — **不重跑 core**,故無法套自訂
params(資料是 batch pipeline 以預設 params 算好的)。
"""

from __future__ import annotations

from datetime import date
from typing import Any

from fusion._shared import fact_to_event
from fusion.raw._db import fetch_facts, fetch_indicator_latest, get_connection

# E 視角子類 → indicator cores(對齊 cores_overview §8.2 四子類)
INDICATOR_CATEGORIES: dict[str, list[str]] = {
    "momentum": [
        "macd_core", "rsi_core", "kd_core", "adx_core", "ma_core",
        "ichimoku_core", "williams_r_core", "cci_core", "coppock_core",
    ],
    "volatility": ["bollinger_core", "keltner_core", "donchian_core", "atr_core"],
    "volume": ["obv_core", "vwap_core", "mfi_core"],
    "pattern": ["candlestick_pattern_core", "support_resistance_core", "trendline_core"],
}

# indicator_stack preset 組合(對齊 api_roadmap §7.5)
INDICATOR_STACK_PRESETS: dict[str, list[str]] = {
    "default": ["macd_core", "rsi_core", "kd_core", "bollinger_core", "ma_core"],
    "day_trade": ["kd_core", "rsi_core", "vwap_core", "bollinger_core"],
    "swing": ["macd_core", "ma_core", "adx_core", "atr_core"],
    "position": ["ma_core", "ichimoku_core", "obv_core", "support_resistance_core"],
}


def _normalize_core(name: str) -> str:
    """"macd" / "MACD" / "macd_core" → "macd_core"。"""
    n = str(name).strip().lower()
    return n if n.endswith("_core") else f"{n}_core"


def category_indicators(category: str, requested: list[str] | None = None) -> list[str]:
    """回某類別要組裝的 cores。requested 為 None → 全類別;否則取(正規化後)交集。"""
    allowed = INDICATOR_CATEGORIES.get(category, [])
    if not requested:
        return list(allowed)
    allow_set = set(allowed)
    return [c for c in (_normalize_core(r) for r in requested) if c in allow_set]


def assemble_indicators(
    stock_id: str,
    as_of: date,
    indicators: list[str],
    *,
    lookback_days: int = 60,
    database_url: str | None = None,
    conn: Any = None,
) -> dict[str, Any]:
    """為 `indicators` 內每個 core 組裝 series + events。

    Returns:
        {stock_id, as_of, indicator_count, indicators, missing}
        indicators[<core>] = {value_date, series, events};series 為該 core 的
        indicator_values JSONB,events 為該 core 近期 facts(統一 Event schema)。
        無 indicator_values 也無 facts 的 core → 進 missing。
    """
    own_conn = conn is None
    if own_conn:
        conn = get_connection(database_url)
    out: dict[str, Any] = {}
    missing: list[str] = []
    try:
        for core in indicators:
            iv = fetch_indicator_latest(
                conn, stock_id=stock_id, as_of=as_of, cores=[core]
            )
            facts = fetch_facts(
                conn, stock_ids=[stock_id], as_of=as_of,
                lookback_days=lookback_days, cores=[core],
            )
            if not iv and not facts:
                missing.append(core)
                continue
            row = iv[0] if iv else {}
            vdate = row.get("value_date")
            value_blob = row.get("value")
            if isinstance(value_blob, dict) and lookback_days > 0:
                series = value_blob.get("series")
                if isinstance(series, list) and len(series) > lookback_days:
                    value_blob = dict(value_blob)
                    value_blob["series"] = series[-lookback_days:]
            out[core] = {
                "value_date": vdate.isoformat() if hasattr(vdate, "isoformat") else vdate,
                "series": value_blob,
                "events": [fact_to_event(f) for f in facts],
            }
    finally:
        if own_conn:
            conn.close()

    return {
        "stock_id": stock_id,
        "as_of": as_of.isoformat(),
        "indicator_count": len(out),
        "indicators": out,
        "missing": missing,
    }
