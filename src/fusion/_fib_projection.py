"""Fibonacci 投影 + 失效價抽取共用 helper(single source of truth)。

v4.33 從 `mcp_server/_forecast.py` 抽出 — 原為 module-private(`_project_range` /
`_find_closest_zone` / `_extract_invalidation_price` / `_TIMEFRAME_FIB_RANGE`)。
neelywave 複合雲圖(dashboards/charts/neely_wave.py)與 forecast tool 兩處共用,
避免跨 mcp_server → dashboards 邊界 import private + 邏輯 drift。

行為與原 `_forecast.py` 版本完全等價(0 behavior change);`_forecast.py` re-import
並以舊私名 alias 保留。
"""

from __future__ import annotations

from fusion._picker import power_rating_sign

# 每個 horizon 用 fib zones 的 ratio 範圍取 range_high / range_low
# ratio_lo = 預期下界 fib;ratio_hi = 預期上界 fib(v3.38 對齊 3 horizon)
TIMEFRAME_FIB_RANGE: dict[str, tuple[float, float]] = {
    "1m": (0.382, 0.618),
    "3m": (0.618, 1.000),
    "6m": (1.000, 1.382),
}


def find_closest_zone(fib_zones: list[dict], target_ratio: float) -> dict | None:
    """找 source_ratio 最接近 target 的 fib zone。"""
    best = None
    best_diff = float("inf")
    for z in fib_zones:
        try:
            r = float(z.get("source_ratio") or 0)
            diff = abs(r - target_ratio)
            if diff < best_diff:
                best_diff = diff
                best = z
        except (TypeError, ValueError):
            continue
    return best


def project_range(
    fib_zones: list[dict],
    ratio_lo: float,
    ratio_hi: float,
    current_price: float,
    sign: int,
) -> tuple[list[float] | None, list[float] | None]:
    """從 fib_zones 中找最接近 ratio_lo / ratio_hi 的 zone,回 (range_low, range_high)。

    每個 range 用 [low, high] list 表達(對應 FibZone.low / FibZone.high)。
    若 fib_zones 為空 → fallback 用 current_price × ratio 估算。
    """
    if not fib_zones:
        # Fallback:用 current_price × ratio scaling
        if sign >= 0:
            # bullish:預期上漲,target 高於 current
            return (
                [current_price * (1 - ratio_lo / 10), current_price * (1 - ratio_lo / 20)],
                [current_price * (1 + ratio_lo / 10), current_price * (1 + ratio_hi / 10)],
            )
        else:
            # bearish:預期下跌,target 低於 current
            return (
                [current_price * (1 - ratio_hi / 10), current_price * (1 - ratio_lo / 10)],
                [current_price * (1 + ratio_lo / 20), current_price * (1 + ratio_lo / 10)],
            )

    # 從 fib_zones 找 ratio_lo / ratio_hi 對應的 zone
    zone_lo = find_closest_zone(fib_zones, ratio_lo)
    zone_hi = find_closest_zone(fib_zones, ratio_hi)

    range_low = [zone_lo["low"], zone_lo["high"]] if zone_lo else None
    range_high = [zone_hi["low"], zone_hi["high"]] if zone_hi else None

    # 確保 range_high 真的 ≥ current,range_low 真的 ≤ current(bullish 場景)
    # bearish 場景 swap
    if sign < 0 and range_high and range_low:
        # bearish 反過來
        range_low, range_high = range_high, range_low

    return range_low, range_high


def extract_invalidation_price(primary: dict | None, current_price: float) -> float | None:
    """從 primary scenario 的 invalidation_triggers 抽價格(PriceBreakBelow / PriceBreakAbove)。"""
    if primary is None:
        return None
    triggers = primary.get("invalidation_triggers") or []
    pr_sign = power_rating_sign(primary.get("power_rating"))

    # bullish 看 break_below 失效;bearish 看 break_above 失效
    target_key = "PriceBreakBelow" if pr_sign >= 0 else "PriceBreakAbove"

    for t in triggers:
        trigger_type = t.get("trigger_type")
        if isinstance(trigger_type, dict) and target_key in trigger_type:
            try:
                return round(float(trigger_type[target_key]), 2)
            except (TypeError, ValueError):
                continue
    return None
