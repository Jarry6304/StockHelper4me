"""Fusion Layer · Integration 端口 — stop_loss(B 視角)。

對齊 m3Spec/fusion_layer.md §5 + api_roadmap_v1.md §4.5。

純計算 wrapper — 給定進場價,整合 ATR(atr_core)+ key_levels 兩種既有來源算出
止損 / 止盈候選。**不引入新規則**(fusion_layer §9 #1):只呈現 ATR-based 與
level-based 候選 + 風險報酬,由 caller / LLM 自行抉擇。
"""

from __future__ import annotations

from datetime import date
from typing import Any

from fusion.key_levels import key_levels
from fusion.raw._db import fetch_indicator_latest, get_connection


def stop_loss(
    stock_id: str,
    entry_price: float,
    as_of: date,
    *,
    direction: str = "long",
    atr_mult: float = 2.0,
    reward_risk_ratio: float = 2.0,
    lookback_days: int = 120,
    database_url: str | None = None,
    conn: Any = None,
) -> dict[str, Any]:
    """止損 / 止盈計算。

    Args:
        stock_id: 股票代號。
        entry_price: 進場價。
        as_of: 查詢日。
        direction: "long"(預設)或 "short"。
        atr_mult: ATR 止損倍數(預設 2.0)。
        reward_risk_ratio: ATR 止盈相對止損的報酬風險比(預設 2.0)。
        lookback_days: key_levels SR facts 回看天數。

    Returns:
        {stock_id, as_of, direction, entry_price, atr, stops, targets}
        stops/targets 各含 atr_based + level_based(nearest support/resistance);
        每筆:{price, distance, distance_pct}。資料缺則該候選為 None。
    """
    is_long = str(direction).strip().lower() != "short"
    own_conn = conn is None
    if own_conn:
        conn = get_connection(database_url)
    try:
        atr = _latest_atr(conn, stock_id, as_of)
        levels = key_levels(
            stock_id, as_of, lookback_days=lookback_days, conn=conn
        )["levels"]
    finally:
        if own_conn:
            conn.close()

    prices = [lv["price"] for lv in levels]
    below = max((p for p in prices if p < entry_price), default=None)
    above = min((p for p in prices if p > entry_price), default=None)

    if is_long:
        atr_stop = entry_price - atr_mult * atr if atr is not None else None
        atr_target = entry_price + atr_mult * reward_risk_ratio * atr if atr is not None else None
        level_stop, level_target = below, above
    else:
        atr_stop = entry_price + atr_mult * atr if atr is not None else None
        atr_target = entry_price - atr_mult * reward_risk_ratio * atr if atr is not None else None
        level_stop, level_target = above, below

    return {
        "stock_id": stock_id,
        "as_of": as_of.isoformat(),
        "direction": "long" if is_long else "short",
        "entry_price": entry_price,
        "atr": atr,
        "stops": {
            "atr_based": _leg(entry_price, atr_stop),
            "nearest_level": _leg(entry_price, level_stop),
        },
        "targets": {
            "atr_based": _leg(entry_price, atr_target),
            "nearest_level": _leg(entry_price, level_target),
        },
    }


def _latest_atr(conn: Any, stock_id: str, as_of: date) -> float | None:
    """從 atr_core indicator_values 取最新一筆 ATR 值。"""
    rows = fetch_indicator_latest(conn, stock_id=stock_id, as_of=as_of, cores=["atr_core"])
    if not rows:
        return None
    series = (rows[0].get("value") or {}).get("series") or []
    if not series:
        return None
    atr = series[-1].get("atr")
    return float(atr) if isinstance(atr, (int, float)) and not isinstance(atr, bool) else None


def _leg(entry_price: float, price: float | None) -> dict[str, Any] | None:
    """組一個止損 / 止盈腳位 dict;price 為 None 回 None。"""
    if price is None:
        return None
    distance = abs(price - entry_price)
    return {
        "price": round(price, 4),
        "distance": round(distance, 4),
        "distance_pct": round(100.0 * distance / entry_price, 3) if entry_price else None,
    }
