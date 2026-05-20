"""Fusion Layer · Integration 端口 — pattern_scan(B 視角)。

對齊 m3Spec/fusion_layer.md §8.1 + api_roadmap_v1.md §4.3。

撈 candlestick_pattern_core 近期型態,為每個型態補上 **key_levels context**
(型態發生價是否貼近支撐 / 壓力 — 同型態在支撐附近 vs 中段意義不同)。
不引入新規則:型態由 core 偵測,本層只整合 key_levels(fusion_layer §9 #1)。
"""

from __future__ import annotations

from datetime import date
from typing import Any

from fusion.key_levels import key_levels
from fusion.raw._db import fetch_facts, fetch_ohlc, get_connection


def pattern_scan(
    stock_id: str,
    as_of: date,
    *,
    lookback_days: int = 60,
    near_level_pct: float = 0.02,
    database_url: str | None = None,
    conn: Any = None,
) -> dict[str, Any]:
    """近期 K 線型態 + 支撐 / 壓力 context。

    Args:
        stock_id: 股票代號。
        as_of: 查詢日(look-ahead 上界)。
        lookback_days: 型態回看天數(預設 60)。
        near_level_pct: 「貼近某價位」的相對門檻(預設 2%)。
        database_url / conn: 連線。

    Returns:
        {stock_id, as_of, pattern_count, patterns}
        每個 pattern:{date, pattern, trend_context, strength, price, level_context}
        依 date 降序。
    """
    own_conn = conn is None
    if own_conn:
        conn = get_connection(database_url)
    try:
        facts = fetch_facts(
            conn, stock_ids=[stock_id], as_of=as_of,
            lookback_days=lookback_days, cores=["candlestick_pattern_core"],
        )
        ohlc = fetch_ohlc(
            conn, stock_id=stock_id, as_of=as_of, lookback_days=lookback_days + 10
        )
        levels = key_levels(stock_id, as_of, conn=conn)["levels"]
    finally:
        if own_conn:
            conn.close()

    close_by_date = {
        r["date"]: float(r["close"])
        for r in ohlc
        if r.get("date") is not None and r.get("close") is not None
    }

    patterns: list[dict[str, Any]] = []
    for f in facts:
        md = f.get("metadata") or {}
        fdate = f.get("fact_date")
        price = close_by_date.get(fdate)
        patterns.append({
            "date": fdate.isoformat() if fdate else None,
            "pattern": md.get("pattern") or md.get("event_kind"),
            "trend_context": md.get("trend_context"),
            "strength": md.get("strength_metric"),
            "price": price,
            "level_context": _level_context(price, levels, near_level_pct),
        })
    patterns.sort(key=lambda p: p["date"] or "", reverse=True)

    return {
        "stock_id": stock_id,
        "as_of": as_of.isoformat(),
        "pattern_count": len(patterns),
        "patterns": patterns,
    }


def _level_context(
    price: float | None, levels: list[dict[str, Any]], near_pct: float
) -> dict[str, Any]:
    """型態發生價 vs 最近 key_level。"""
    if price is None or not price or not levels:
        return {"near_level": False, "nearest_level_price": None}
    nearest = min(levels, key=lambda lv: abs(lv["price"] - price))
    dist_pct = abs(nearest["price"] - price) / price * 100.0
    return {
        "near_level": dist_pct <= near_pct * 100.0,
        "nearest_level_price": nearest["price"],
        "distance_pct": round(dist_pct, 3),
        "level_strength": nearest.get("strength"),
        "level_sources": nearest.get("sources"),
    }
