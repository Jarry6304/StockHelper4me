"""Tool 1 內部演算法:`neely_forecast` — 4 個時間框架的 % + 價位區間。

對齊 plan §Tool 1:
1. 撈 Neely structural_snapshots(取 top 5 by `power_rating`)+ indicator_values latest
2. 4 個時間框架的價位區間 from `expected_fib_zones`(Fib ratio scaling)
3. prob_up 跨 cores 加權公式(plan §Tool 1 第 3 點)
4. invalidation_price 從 scenario invalidation_triggers 抽

呼叫端:`mcp_server.tools.data.neely_forecast()`。

設計:
- Forest 不選 primary(對齊 architecture §8.2),top 5 by `power_rating` 取首位作 primary_scenario
- 4 時間框架 fib ratio scaling 寫死(plan §Tool 1 第 2 點)
- prob_up 公式寫死 constants(對齊 NEELY constants 寫死慣例)
- 跨 cores 加權算機率 = Aggregation Layer 整合層責任(cores_overview §10.0)
"""

from __future__ import annotations

from datetime import date
from typing import Any

# ────────────────────────────────────────────────────────────
# 4 時間框架 Fibonacci ratio scaling(plan §Tool 1 第 2 點)
# ────────────────────────────────────────────────────────────

# 每個 timeframe 用 fib zones 的 ratio 範圍取 range_high / range_low
# ratio_lo = 預期下界 fib;ratio_hi = 預期上界 fib
_TIMEFRAME_FIB_RANGE: dict[str, tuple[float, float]] = {
    "1_month":   (0.382, 0.618),
    "1_quarter": (0.618, 1.000),
    "6_month":   (1.000, 1.382),
    "1_year":    (1.382, 1.618),
}

# Prob_up 時間衰減(plan §Tool 1 第 3 點)
_TIMEFRAME_DECAY: dict[str, float] = {
    "1_month":   1.00,
    "1_quarter": 0.85,
    "6_month":   0.70,
    "1_year":    0.55,
}

# Power Rating → base prob_up(plan §Tool 1 第 3 點)
_POWER_TO_PROB: dict[str, float] = {
    "StrongBullish":  0.70,
    "Bullish":        0.62,
    "SlightBullish":  0.56,
    "Neutral":        0.50,
    "SlightBearish":  0.44,
    "Bearish":        0.38,
    "StrongBearish":  0.30,
}


# ────────────────────────────────────────────────────────────
# Public API
# ────────────────────────────────────────────────────────────

def compute_neely_forecast(
    stock_id: str,
    as_of: date,
    *,
    database_url: str | None = None,
) -> dict[str, Any]:
    """主入口 — Neely 4 時間框架預測。

    Args:
        stock_id: 股票代號(例 "2330")
        as_of: 查詢日
        database_url: 可選 PG 連線字串

    Returns:
        dict 結構對齊 plan §Tool 1 Output(~2 KB / ~500 tokens)
    """
    from agg import as_of as agg_as_of

    # 撈 Neely structural + 跨 cores 動能 / 籌碼最新值
    relevant_cores = [
        "neely_core",
        "macd_core", "rsi_core", "adx_core",
        "institutional_core", "foreign_holding_core",
    ]
    snapshot = agg_as_of(
        stock_id,
        as_of,
        cores=relevant_cores,
        lookback_days=30,
        include_market=False,
        database_url=database_url,
    )

    # 1. Primary scenario 從 neely structural 抽
    primary, all_scenarios = _extract_primary_and_top_scenarios(snapshot, limit=5)

    # 2. Current price(從 ma_core series 最後一筆,或 valuation_core fallback)
    current_price = _extract_current_price(snapshot)

    # 3. 4 時間框架價位區間
    forecasts = _build_forecasts(primary, current_price, snapshot, as_of)

    # 4. Key levels:supports / resistances 從 fib zones 推
    key_levels = _extract_key_levels(primary, current_price)

    # 5. Invalidation price 從 primary 的 triggers 抽
    invalidation_price = _extract_invalidation_price(primary, current_price)

    return {
        "stock_id":           stock_id,
        "as_of":              as_of.isoformat(),
        "current_price":      current_price,
        "primary_scenario":   _format_primary_scenario(primary),
        "scenario_count":     len(all_scenarios),
        "forecasts":          forecasts,
        "key_levels":         key_levels,
        "invalidation_price": invalidation_price,
    }


# ────────────────────────────────────────────────────────────
# Internal helpers
# ────────────────────────────────────────────────────────────

def _extract_primary_and_top_scenarios(snapshot, *, limit: int = 5) -> tuple[dict | None, list[dict]]:
    """從 structural['neely_core@daily'] 取 scenario_forest,by power_rating 排序取 top-N。"""
    structural = snapshot.structural
    neely_row = None
    for key, row in structural.items():
        if key.startswith("neely_core"):
            neely_row = row
            break
    if neely_row is None:
        return None, []

    snap = neely_row.snapshot or {}
    scenarios = snap.get("scenario_forest") or snap.get("scenarios") or []
    if not isinstance(scenarios, list) or not scenarios:
        return None, []

    # 排序:power_rating 絕對值大 → 強訊號 / passed_rules_count 多 → 可信度高
    def _score(s: dict) -> tuple[int, int]:
        pr_strength = _power_rating_strength(s.get("power_rating"))
        rules_count = int(s.get("rules_passed_count") or 0)
        return (pr_strength, rules_count)

    sorted_scenarios = sorted(scenarios, key=_score, reverse=True)
    top = sorted_scenarios[:limit]
    primary = top[0] if top else None
    return primary, top


def _power_rating_strength(rating: Any) -> int:
    """PowerRating → 強度級別(0-3)。"""
    if not rating:
        return 0
    if isinstance(rating, dict):
        # serde tagged enum,取唯一 key
        rating = next(iter(rating.keys()), None)
    if not isinstance(rating, str):
        return 0
    mapping = {
        "StrongBullish": 3, "StrongBearish": 3,
        "Bullish":       2, "Bearish":       2,
        "SlightBullish": 1, "SlightBearish": 1,
        "Neutral":       0,
    }
    return mapping.get(rating, 0)


def _power_rating_sign(rating: Any) -> int:
    """+1 bull / -1 bear / 0 neutral。"""
    if isinstance(rating, dict):
        rating = next(iter(rating.keys()), None)
    if not isinstance(rating, str):
        return 0
    if rating.endswith("Bullish"):
        return +1
    if rating.endswith("Bearish"):
        return -1
    return 0


def _power_rating_label(rating: Any) -> str:
    """正規化 PowerRating 字串。"""
    if isinstance(rating, dict):
        return next(iter(rating.keys()), "Neutral")
    if isinstance(rating, str):
        return rating
    return "Neutral"


def _format_primary_scenario(scenario: dict | None) -> dict[str, Any]:
    """壓縮 primary scenario 成最小 LLM-friendly 摘要(避免回 raw scenario blob)。"""
    if scenario is None:
        return {
            "label":        None,
            "pattern_type": None,
            "power_rating": "Neutral",
            "wave_count":   0,
        }
    pattern_type = scenario.get("pattern_type")
    if isinstance(pattern_type, dict):
        pattern_label = next(iter(pattern_type.keys()), "Unknown")
    elif isinstance(pattern_type, str):
        pattern_label = pattern_type
    else:
        pattern_label = "Unknown"

    return {
        "label":        scenario.get("structure_label") or scenario.get("id"),
        "pattern_type": pattern_label,
        "power_rating": _power_rating_label(scenario.get("power_rating")),
        "wave_count":   int(scenario.get("rules_passed_count") or 0),
    }


def _extract_current_price(snapshot) -> float:
    """從 ma_core series 最後一筆 close 取;沒有 fallback 0.0。"""
    for key, row in snapshot.indicator_latest.items():
        if not key.startswith("ma_core"):
            continue
        value = row.value or {}
        series = value.get("series")
        if isinstance(series, list) and series:
            last = series[-1]
            if isinstance(last, dict) and "close" in last:
                try:
                    return float(last["close"])
                except (TypeError, ValueError):
                    pass
    return 0.0


def _build_forecasts(
    primary: dict | None,
    current_price: float,
    snapshot,
    as_of: date,
) -> dict[str, dict[str, Any]]:
    """4 個 timeframe 各算 prob_up + range_high + range_low。"""
    if primary is None or current_price <= 0:
        # 無 primary scenario / 無價格 → 全 neutral
        return {
            tf: {"prob_up": 0.50, "range_high": None, "range_low": None}
            for tf in _TIMEFRAME_FIB_RANGE
        }

    fib_zones = primary.get("expected_fib_zones") or []
    pr_label = _power_rating_label(primary.get("power_rating"))
    base_prob = _POWER_TO_PROB.get(pr_label, 0.50)
    pr_sign = _power_rating_sign(primary.get("power_rating"))

    # 跨 cores 動能 / 籌碼 adjustments(plan §Tool 1 第 3 點公式)
    momentum_adj = _compute_momentum_adj(snapshot)
    chip_adj     = _compute_chip_adj(snapshot)

    forecasts: dict[str, dict[str, Any]] = {}
    for tf, (ratio_lo, ratio_hi) in _TIMEFRAME_FIB_RANGE.items():
        decay = _TIMEFRAME_DECAY[tf]
        # prob_up = base + decay * (momentum + chip) ×(sign 對齊 bullish/bearish 方向)
        raw_prob = base_prob + decay * pr_sign * (momentum_adj + chip_adj)
        prob_up = max(0.10, min(0.90, raw_prob))

        # 價位區間從 fib zones 拉
        range_low, range_high = _project_range(fib_zones, ratio_lo, ratio_hi, current_price, pr_sign)

        forecasts[tf] = {
            "prob_up":    round(prob_up, 2),
            "range_high": _round_range(range_high),
            "range_low":  _round_range(range_low),
        }

    return forecasts


def _compute_momentum_adj(snapshot) -> float:
    """從 macd / rsi / adx 算 momentum adjustment(-0.13 ~ +0.13)。"""
    adj = 0.0
    indicator_latest = snapshot.indicator_latest

    # macd histogram sign(+0.05 / -0.05 / 0)
    for key, row in indicator_latest.items():
        if not key.startswith("macd_core"):
            continue
        value = row.value or {}
        last = _get_last_series_point(value)
        if last:
            histogram = last.get("histogram") or 0
            try:
                if float(histogram) > 0:
                    adj += 0.05
                elif float(histogram) < 0:
                    adj -= 0.05
            except (TypeError, ValueError):
                pass
        break

    # rsi: (rsi - 50) / 100 × 0.05
    for key, row in indicator_latest.items():
        if not key.startswith("rsi_core"):
            continue
        value = row.value or {}
        last = _get_last_series_point(value)
        if last:
            rsi = last.get("rsi") or last.get("value") or 50
            try:
                adj += (float(rsi) - 50.0) / 100.0 * 0.05
            except (TypeError, ValueError):
                pass
        break

    # adx +DI / -DI 比較(or adx 趨勢強度)
    for key, row in indicator_latest.items():
        if not key.startswith("adx_core"):
            continue
        value = row.value or {}
        last = _get_last_series_point(value)
        if last:
            try:
                plus_di = float(last.get("plus_di") or 0)
                minus_di = float(last.get("minus_di") or 0)
                if plus_di > minus_di:
                    adj += 0.03
                elif minus_di > plus_di:
                    adj -= 0.03
            except (TypeError, ValueError):
                pass
        break

    return 0.5 * adj


def _compute_chip_adj(snapshot) -> float:
    """從 institutional / foreign_holding 算 chip adjustment(-0.07 ~ +0.07)。"""
    adj = 0.0
    indicator_latest = snapshot.indicator_latest

    # institutional net_5d sign
    for key, row in indicator_latest.items():
        if not key.startswith("institutional_core"):
            continue
        value = row.value or {}
        last = _get_last_series_point(value)
        if last:
            try:
                inst_net = float(last.get("total_net") or last.get("net") or 0)
                if inst_net > 0:
                    adj += 0.04
                elif inst_net < 0:
                    adj -= 0.04
            except (TypeError, ValueError):
                pass
        break

    # foreign_holding change_pct
    for key, row in indicator_latest.items():
        if not key.startswith("foreign_holding_core"):
            continue
        value = row.value or {}
        last = _get_last_series_point(value)
        if last:
            try:
                change = float(last.get("change_pct") or last.get("holding_change") or 0)
                if change > 0:
                    adj += 0.03
                elif change < 0:
                    adj -= 0.03
            except (TypeError, ValueError):
                pass
        break

    return 0.5 * adj


def _get_last_series_point(value: dict) -> dict | None:
    """從 indicator value JSONB 拿最後一筆 series point。"""
    series = value.get("series")
    if isinstance(series, list) and series:
        last = series[-1]
        if isinstance(last, dict):
            return last
    return None


def _project_range(
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
    zone_lo = _find_closest_zone(fib_zones, ratio_lo)
    zone_hi = _find_closest_zone(fib_zones, ratio_hi)

    range_low = [zone_lo["low"], zone_lo["high"]] if zone_lo else None
    range_high = [zone_hi["low"], zone_hi["high"]] if zone_hi else None

    # 確保 range_high 真的 ≥ current,range_low 真的 ≤ current(bullish 場景)
    # bearish 場景 swap
    if sign < 0 and range_high and range_low:
        # bearish 反過來
        range_low, range_high = range_high, range_low

    return range_low, range_high


def _find_closest_zone(fib_zones: list[dict], target_ratio: float) -> dict | None:
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


def _round_range(rng: list[float] | None) -> list[float] | None:
    """範圍 round 到 2 位小數。"""
    if rng is None:
        return None
    return [round(float(v), 2) for v in rng]


def _extract_key_levels(primary: dict | None, current_price: float) -> dict[str, list[float]]:
    """從 primary scenario 的 fib_zones + invalidation triggers 抽 support / resistance。"""
    if primary is None or current_price <= 0:
        return {"support": [], "resistance": []}

    fib_zones = primary.get("expected_fib_zones") or []
    supports: list[float] = []
    resistances: list[float] = []

    for z in fib_zones:
        try:
            low = float(z["low"])
            high = float(z["high"])
            mid = (low + high) / 2
            if mid < current_price:
                supports.append(round(mid, 2))
            elif mid > current_price:
                resistances.append(round(mid, 2))
        except (KeyError, TypeError, ValueError):
            continue

    # 排序:supports 大 → 小(近 current),resistances 小 → 大
    supports.sort(reverse=True)
    resistances.sort()

    # 限制各 3 個避免噪音
    return {
        "support":    supports[:3],
        "resistance": resistances[:3],
    }


def _extract_invalidation_price(primary: dict | None, current_price: float) -> float | None:
    """從 primary scenario 的 invalidation_triggers 抽價格(PriceBreakBelow / PriceBreakAbove)。"""
    if primary is None:
        return None
    triggers = primary.get("invalidation_triggers") or []
    pr_sign = _power_rating_sign(primary.get("power_rating"))

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
