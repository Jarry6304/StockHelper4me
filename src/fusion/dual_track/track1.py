"""dual_track · 軌道一(結構)讀法。

對齊 m3Spec/dual_track_resonance.md §三 + §六:
- 一律讀 structural_snapshots(neely_core 完整 forest)
- 不靠 forecast_log 的 neely_fib 行(那是 internal_only 對齊影子)
- primary picker 對齊 v3.35 degree-aware 規則

輸出 Track1View 含:
- primary scenario 的離散 fib 線清單(從 expected_fib_zones,fallback flat_fib_zones)
- 失效價(invalidation_triggers 內 InvalidateScenario + PriceBreakBelow/Above)
- 方向(power_rating sign 推 bullish/bearish)
- A-3 失效閘門狀態(若給 current_price)
"""

from __future__ import annotations

import re
from datetime import date
from typing import Any

from fusion.raw._db import fetch_structural_latest

from fusion.dual_track._shared import (
    FIB_LINES_CLUSTER_PCT,
    FIB_LINES_MAX_COUNT,
    FibLine,
    Track1View,
)


__all__ = ["read_track1", "scenario_is_invalidated"]


# ─── Picker / direction / degree helpers(對齊 mcp_server/_forecast.py)──────


_DEGREE_RANK: dict[str, int] = {
    "Subminuette": 1, "SubMinuette": 1, "Minuette": 2,
    "Minute": 3,
    "Minor": 4, "Intermediate": 4,
    "Primary": 5, "Cycle": 6, "Supercycle": 7, "GrandSupercycle": 8,
}


def _coerce_date(s: Any) -> date | None:
    if isinstance(s, date):
        return s
    if isinstance(s, str):
        try:
            return date.fromisoformat(s[:10])
        except ValueError:
            return None
    return None


def _scenario_span_days(scenario: dict) -> int | None:
    wt = scenario.get("wave_tree") or {}
    start = _coerce_date(wt.get("start"))
    end = _coerce_date(wt.get("end"))
    if start is None or end is None:
        return None
    delta = (end - start).days
    return delta if delta > 0 else None


def _effective_degree(scenario: dict) -> str | None:
    """Stage 11 §13.3 Degree Ceiling 表(對齊 mcp_server picker)。"""
    span = _scenario_span_days(scenario)
    if span is None:
        return None
    years = span / 365.0
    if years < 0.3:
        return "Subminuette"
    if years < 1.0:
        return "Minuette"
    if years < 3.0:
        return "Minute"
    if years < 10.0:
        return "Minor"
    if years < 30.0:
        return "Primary"
    if years < 100.0:
        return "Cycle"
    return "Supercycle"


def _power_rating_label(rating: Any) -> str:
    if isinstance(rating, dict):
        return next(iter(rating.keys()), "Neutral")
    if isinstance(rating, str):
        return rating
    return "Neutral"


def _power_rating_strength(rating: Any) -> int:
    """0..3,對齊 mcp_server/_forecast.py:_power_rating_strength。"""
    if not rating:
        return 0
    if isinstance(rating, dict):
        rating = next(iter(rating.keys()), None)
    if not isinstance(rating, str):
        return 0
    return {
        "StrongBullish": 3, "StrongBearish": 3,
        "Bullish": 2, "Bearish": 2,
        "SlightBullish": 1, "SlightBearish": 1,
        "Neutral": 0,
    }.get(rating, 0)


def _direction_from_power(rating: Any) -> str:
    """+1 bull / -1 bear / 0 neutral → 'bullish' / 'bearish' / 'neutral'。"""
    label = _power_rating_label(rating)
    if label.endswith("Bullish"):
        return "bullish"
    if label.endswith("Bearish"):
        return "bearish"
    return "neutral"


def _pattern_type_label(pattern_type: Any) -> str | None:
    if isinstance(pattern_type, dict):
        return next(iter(pattern_type.keys()), None)
    if isinstance(pattern_type, str):
        return pattern_type
    return None


def _wave_count_from_label(label: str | None) -> int:
    if not label:
        return 0
    m = re.search(r"(\d+)-wave", label)
    return int(m.group(1)) if m else 0


def _pick_primary(forest: list[dict]) -> dict | None:
    """對齊 v3.35:(degree DESC, power DESC, rules DESC)。"""
    if not forest:
        return None
    scored = [
        (
            _DEGREE_RANK.get(_effective_degree(s) or "", 0),
            _power_rating_strength(s.get("power_rating")),
            int(s.get("rules_passed_count") or 0),
            s,
        )
        for s in forest
    ]
    scored.sort(key=lambda x: (x[0], x[1], x[2]), reverse=True)
    return scored[0][3]


# ─── Invalidation(A-3 閘門前置)──────────────────────────────────────────────


def _extract_invalidation_price(scenario: dict, direction: str) -> float | None:
    """從 scenario.invalidation_triggers 抽 InvalidateScenario + PriceBreakBelow/Above。

    對齊 mcp_server/_forecast.py:_scenario_is_invalidated 的解析:
    - bullish scenario → PriceBreakBelow(price);direction bearish → PriceBreakAbove
    - on_trigger 必 InvalidateScenario(WeakenScenario / PromoteAlternative 不算)

    本函式 returns 顯示用的 invalidation_price(對齊 LLM 看 UI),只挑「主方向」trigger。
    A-3 閘門實際判定走 `_extract_all_invalidation_thresholds` + 全 trigger 檢查
    (v4.25.x:對齊 user 拍版 neutral 也走 A-3,只要有 trigger 就判)。
    """
    triggers = scenario.get("invalidation_triggers") or []
    for t in triggers:
        action = t.get("on_trigger")
        if isinstance(action, dict):
            action = next(iter(action.keys()), None)
        if action != "InvalidateScenario":
            continue
        trigger_type = t.get("trigger_type")
        if not isinstance(trigger_type, dict):
            continue
        if direction == "bullish" and "PriceBreakBelow" in trigger_type:
            try:
                return float(trigger_type["PriceBreakBelow"])
            except (TypeError, ValueError):
                continue
        if direction == "bearish" and "PriceBreakAbove" in trigger_type:
            try:
                return float(trigger_type["PriceBreakAbove"])
            except (TypeError, ValueError):
                continue
        # direction == "neutral":兩種都收(取第一個 bullish-style)
        if direction == "neutral":
            if "PriceBreakBelow" in trigger_type:
                try:
                    return float(trigger_type["PriceBreakBelow"])
                except (TypeError, ValueError):
                    continue
            if "PriceBreakAbove" in trigger_type:
                try:
                    return float(trigger_type["PriceBreakAbove"])
                except (TypeError, ValueError):
                    continue
    return None


def _extract_all_invalidation_thresholds(scenario: dict) -> list[tuple[str, float]]:
    """抽所有 InvalidateScenario triggers,回 [(kind, threshold), ...]。

    kind:
        - "below":PriceBreakBelow,當 current < threshold → 觸發
        - "above":PriceBreakAbove,當 current > threshold → 觸發

    本函式只負責解析、不套 direction policy。read_track1 依 direction 決定要
    feed 哪些 kind 給 _check_any_threshold_breached(對齊 v4.25.x:neutral 走
    ALL kinds,bullish 只看 below,bearish 只看 above — 對齊 spec §四 字面 +
    保守不擴張)。
    """
    out: list[tuple[str, float]] = []
    for t in scenario.get("invalidation_triggers") or []:
        action = t.get("on_trigger")
        if isinstance(action, dict):
            action = next(iter(action.keys()), None)
        if action != "InvalidateScenario":
            continue
        trigger_type = t.get("trigger_type")
        if not isinstance(trigger_type, dict):
            continue
        if "PriceBreakBelow" in trigger_type:
            try:
                out.append(("below", float(trigger_type["PriceBreakBelow"])))
            except (TypeError, ValueError):
                pass
        if "PriceBreakAbove" in trigger_type:
            try:
                out.append(("above", float(trigger_type["PriceBreakAbove"])))
            except (TypeError, ValueError):
                pass
    return out


def _check_any_threshold_breached(
    thresholds: list[tuple[str, float]],
    current_price: float | None,
) -> tuple[bool, str | None, float | None]:
    """任一 trigger 觸發即回 True。

    Returns:
        (breached, fired_kind, fired_threshold)
    """
    if current_price is None or not thresholds:
        return False, None, None
    cp = float(current_price)
    for kind, threshold in thresholds:
        if kind == "below" and cp < threshold:
            return True, kind, threshold
        if kind == "above" and cp > threshold:
            return True, kind, threshold
    return False, None, None


def scenario_is_invalidated(
    *,
    direction: str,
    invalidation_price: float | None,
    current_price: float | None,
) -> bool:
    """A-3 失效閘門判定(backward-compat 簽章)。

    bullish + current < invalidation → True
    bearish + current > invalidation → True
    neutral / None / 缺資料 → False

    Note: v4.25.x 新加 `_check_any_threshold_breached` 才是 read_track1 實際走的
    路徑(對齊 neutral A-3 user 拍版)。本函式保留給既有 caller / 既有 tests
    backward compat。
    """
    if current_price is None or invalidation_price is None:
        return False
    if direction == "bullish":
        return float(current_price) < float(invalidation_price)
    if direction == "bearish":
        return float(current_price) > float(invalidation_price)
    return False


# ─── Fib line extraction ─────────────────────────────────────────────────────


def _zone_to_fib_line(zone: dict) -> FibLine | None:
    """expected_fib_zones / flat_fib_zones 元素 → FibLine。"""
    lo, hi = zone.get("low"), zone.get("high")
    if not isinstance(lo, (int, float)) or not isinstance(hi, (int, float)):
        return None
    if isinstance(lo, bool) or isinstance(hi, bool):
        return None
    price = (float(lo) + float(hi)) / 2.0
    return FibLine(
        price=price,
        low=float(lo),
        high=float(hi),
        label=str(zone["label"]) if zone.get("label") is not None else None,
        source_ratio=float(zone["source_ratio"]) if isinstance(
            zone.get("source_ratio"), (int, float)
        ) and not isinstance(zone.get("source_ratio"), bool) else None,
    )


def _cluster_and_cap_fib_lines(
    lines: list[FibLine],
    *,
    max_count: int = FIB_LINES_MAX_COUNT,
    cluster_pct: float = FIB_LINES_CLUSTER_PCT,
) -> tuple[list[FibLine], int, bool]:
    """1% bucket cluster + hard cap fib_lines。對齊 fusion._shared.cluster_price_levels。

    Production 案例:flat_fib_zones 可達 100+ 條(2330 fallback union 155 條),
    直暴露 MCP 會撐爆 context budget(70KB+)。本函式:
    1. 對 input lines 按 price 升序 greedy 收 1% bucket(同 bucket 取代表)
    2. cluster 後若仍 > max_count → 等距取樣 cap(保留價格覆蓋範圍)
    3. label 字串記錄合併狀態(`clustered(N): label_a, label_b ...`)

    Returns:
        (clustered_lines, n_input_raw, was_reduced)
    """
    if not lines:
        return [], 0, False
    n_input = len(lines)

    # Step 1:1% bucket cluster
    sorted_lines = sorted(lines, key=lambda f: f.price)
    clusters: list[list[FibLine]] = []
    current: list[FibLine] = [sorted_lines[0]]
    for f in sorted_lines[1:]:
        anchor = current[0].price
        if anchor > 0 and abs(f.price - anchor) / anchor < cluster_pct:
            current.append(f)
        else:
            clusters.append(current)
            current = [f]
    clusters.append(current)

    # Step 2:merge each cluster(中位點 + 合併 label)
    merged: list[FibLine] = []
    for c in clusters:
        if len(c) == 1:
            merged.append(c[0])
            continue
        prices = sorted(f.price for f in c)
        median = prices[len(prices) // 2]
        labels = sorted({f.label for f in c if f.label})
        label_str = f"clustered({len(c)})"
        if labels:
            preview = ", ".join(labels[:3])
            if len(labels) > 3:
                preview += f", +{len(labels) - 3} more"
            label_str = f"{label_str}: {preview}"
        # source_ratio:取首個非 None(0.382 / 0.618 / 1.0 等代表值)
        rep_ratio = next((f.source_ratio for f in c if f.source_ratio is not None), None)
        merged.append(FibLine(
            price=round(median, 4),
            low=round(min(f.low for f in c), 4),
            high=round(max(f.high for f in c), 4),
            label=label_str,
            source_ratio=rep_ratio,
        ))

    # Step 3:仍超 max_count → 等距取樣(保留價格分布)
    if len(merged) > max_count:
        step = len(merged) / max_count
        sampled: list[FibLine] = []
        i = 0.0
        while i < len(merged) and len(sampled) < max_count:
            sampled.append(merged[int(i)])
            i += step
        merged = sampled

    was_reduced = len(merged) < n_input
    return merged, n_input, was_reduced


# ─── Public API ──────────────────────────────────────────────────────────────


def read_track1(
    conn,
    *,
    stock_id: str,
    as_of: date,
    current_price: float | None = None,
    timeframe: str = "daily",
) -> Track1View:
    """讀 structural_snapshots → Track1View(neely primary scenario + fib lines)。

    Args:
        conn: PG conn(dict_row factory)
        stock_id: 股票代號
        as_of: 上界(包含)
        current_price: 用來判 A-3 失效閘門;None → invalidated 一律 False
        timeframe: structural_snapshots.timeframe(預設 daily)

    Returns:
        Track1View(has_snapshot=False / fib_lines=[] 表示軌道一不可用)
    """
    rows = fetch_structural_latest(
        conn, stock_id=stock_id, as_of=as_of, cores=["neely_core"]
    )
    # 走指定 timeframe 那筆(fetch_structural_latest 對每 (core, timeframe) 取最新)
    row = next(
        (r for r in rows if r.get("timeframe") == timeframe),
        None,
    )
    if row is None:
        return Track1View(
            stock_id=stock_id, as_of=as_of, snapshot_date=None,
            has_snapshot=False, pattern_type=None, power_rating=None,
            direction="neutral", effective_degree=None, wave_count=0,
            fib_lines=[], notes=[f"no neely_core structural_snapshot ≤ {as_of} (tf={timeframe})"],
        )

    snapshot = row.get("snapshot") or {}
    if isinstance(snapshot, str):
        import json
        try:
            snapshot = json.loads(snapshot)
        except Exception:
            snapshot = {}
    snapshot_date = row.get("snapshot_date")

    forest = snapshot.get("scenario_forest") or []
    primary = _pick_primary(forest)
    if primary is None:
        return Track1View(
            stock_id=stock_id, as_of=as_of, snapshot_date=snapshot_date,
            has_snapshot=True, pattern_type=None, power_rating=None,
            direction="neutral", effective_degree=None, wave_count=0,
            fib_lines=[], notes=["empty scenario_forest"],
        )

    pattern_label = _pattern_type_label(primary.get("pattern_type"))
    direction = _direction_from_power(primary.get("power_rating"))
    power_label = _power_rating_label(primary.get("power_rating"))
    degree = _effective_degree(primary)
    structure_label = primary.get("structure_label") or primary.get("id")
    wave_count = _wave_count_from_label(structure_label)

    # Fib zones — primary 優先,fallback flat_fib_zones
    zones = primary.get("expected_fib_zones") or []
    fallback_used = False
    if not zones:
        flat = snapshot.get("flat_fib_zones") or []
        if flat:
            zones = flat
            fallback_used = True

    raw_fib_lines = [fl for fl in (_zone_to_fib_line(z) for z in zones) if fl is not None]
    # cluster + cap(對齊 §六 失真處理:flat_union 可達 100+ 條 → MCP payload 爆炸)
    fib_lines, n_raw, was_reduced = _cluster_and_cap_fib_lines(raw_fib_lines)

    # invalidation_price 顯示用(對齊 LLM context):取主方向 trigger
    invalidation_price = _extract_invalidation_price(primary, direction)
    # A-3 閘門實際判定:依 direction filter thresholds(對齊 v4.25.x user 拍版
    # 「neutral 遇 trigger 就判」+ spec §四「現價跌破 invalidation 軌道一退場」
    # 字面;bullish/bearish 維持只看主方向 trigger,不擴成 wave 5 extended)。
    all_thresholds = _extract_all_invalidation_thresholds(primary)
    if direction == "bullish":
        # bullish 只看 PriceBreakBelow(current 跌破 → thesis 破)
        relevant_thresholds = [t for t in all_thresholds if t[0] == "below"]
    elif direction == "bearish":
        # bearish 只看 PriceBreakAbove(current 漲破 → thesis 破)
        relevant_thresholds = [t for t in all_thresholds if t[0] == "above"]
    else:
        # neutral 走 ALL(下/上 任一 trigger 中即失效;對齊 0050 等 ETF case)
        relevant_thresholds = all_thresholds
    invalidated, fired_kind, fired_threshold = _check_any_threshold_breached(
        relevant_thresholds, current_price
    )

    notes: list[str] = []
    if not fib_lines:
        notes.append("no fib zones (neither primary.expected_fib_zones nor flat_fib_zones populated)")
    if fallback_used:
        notes.append("fib_lines from flat_fib_zones fallback (primary.expected_fib_zones empty)")
    if was_reduced:
        notes.append(
            f"fib_lines reduced {n_raw} → {len(fib_lines)} "
            f"(1% bucket cluster + cap {FIB_LINES_MAX_COUNT};對齊 MCP context budget)"
        )
    if invalidated:
        op_word = "跌破" if fired_kind == "below" else "漲破"
        notes.append(
            f"A-3 invalidation gate triggered: {direction} scenario, "
            f"current={current_price} {op_word} threshold={fired_threshold} "
            f"(trigger_kind={fired_kind})"
        )

    return Track1View(
        stock_id=stock_id,
        as_of=as_of,
        snapshot_date=snapshot_date,
        has_snapshot=True,
        pattern_type=pattern_label,
        power_rating=power_label,
        direction=direction,
        effective_degree=degree,
        wave_count=wave_count,
        fib_lines=fib_lines,
        invalidation_price=invalidation_price,
        invalidated=invalidated,
        fallback_to_flat_union=fallback_used,
        notes=notes,
    )
