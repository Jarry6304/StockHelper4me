"""dual_track · 軌道一(結構)讀法。

對齊 m3Spec/dual_track_resonance.md §三 + §六 + m3Spec/wave_judgment_loop.md §8:
- 一律讀 structural_snapshots(neely_core 完整 forest)
- 不靠 forecast_log 的行(judgment forward 列是 internal_only 對齊影子)
- **v4.39 起無 picker**:有 active judgment → 用 accepted[preferred] 候選
  (pattern、fib zones、失效價);無 → 計數無關聚合特徵(up_share /
  invalidation_band / ambiguity_count),`up_share ∉ [0.4, 0.6]` 才給方向,
  否則 `direction="undecided"`

輸出 Track1View 含:
- judgment 路徑:preferred 候選的離散 fib 線(expected_fib_zones,fallback
  flat_fib_zones)、失效價、方向(power_rating sign)、A-3 失效閘門狀態
- aggregate 路徑:flat_fib_zones 聯集線(引擎聚合,無選取)+ 聚合特徵;
  無單一 thesis → invalidated 恆 False
"""

from __future__ import annotations

from datetime import date
from typing import Any

from fusion.judgment import fetch_active_judgment, scenario_anchor_key
from fusion.raw._db import fetch_structural_latest

from fusion.dual_track._shared import (
    FIB_LINES_CLUSTER_PCT,
    FIB_LINES_MAX_COUNT,
    FibLine,
    Track1View,
)
# v4.26 follow-up + B1 consolidation:picker / degree helpers 都從 src/fusion/_picker.py
# 取(single source of truth,對齊 Rust output.rs::Degree + degree/mod.rs::classify_degree)。
from fusion._picker import (
    DEGREE_RANK,
    coerce_date as _coerce_date,
    degree_rank as _degree_rank,
    direction_from_power as _direction_from_power,
    effective_degree as _effective_degree,
    pattern_type_label as _pattern_type_label,
    power_rating_label as _power_rating_label,
    power_rating_strength as _power_rating_strength,
)

# B1:track1 既有 _DEGREE_RANK 名稱保留為 alias(wave_impulse_screen 等 caller
# 仍 import _DEGREE_RANK from track1,並等同取 canonical;對齊 wave_impulse_screen
# 的 import 已同 PR 重新指向 _picker,但保留 alias 不破其他 unknown caller)。
_DEGREE_RANK = DEGREE_RANK


# ─── Aggregate 特徵(無 judgment 路徑;wave_judgment_loop §8)────────────────

# live-edge 判定與 dossier 同源(end_bar ≥ last_bar − LIVE_EDGE_BARS)
_LIVE_EDGE_BARS = 3

# post_pattern_behavior → 是否表達方向性前瞻。方向本身取 power_rating sign
# (engine 的 post_behavior 即由 (pattern_type, power_rating, ctx) 查表產生,
# 方向資訊在 power_rating;behavior 決定「有無約束」):
#   FullRetracementRequired / MinRetracement / ReachesWaveZone /
#   NextImpulseExceeds / NotFullyRetracedUnless / Composite → 有方向性(計數)
#   Unconstrained / HintsAtPattern / 缺欄 → 無方向性(不入 up_share 分母)
#   power_rating = Neutral → 同樣不入分母(sign = 0)
_NON_DIRECTIONAL_BEHAVIORS = {"Unconstrained", "HintsAtPattern"}


def _behavior_is_directional(behavior: Any) -> bool:
    if behavior is None:
        return False
    if isinstance(behavior, str):
        return behavior not in _NON_DIRECTIONAL_BEHAVIORS
    if isinstance(behavior, dict) and behavior:
        kind = next(iter(behavior.keys()))
        if kind == "Composite":
            subs = (behavior.get("Composite") or {}).get("behaviors") or []
            return any(_behavior_is_directional(b) for b in subs)
        return kind not in _NON_DIRECTIONAL_BEHAVIORS
    return False


def _live_scenarios(snapshot: dict, forest: list[dict]) -> tuple[list[dict], bool]:
    """live-edge 候選(dossier 同款 bar 對映)。

    無 monowave bar 對映(舊 snapshot / 精簡 fixture)→ 全 forest 視為 live
    (寬鬆側 fallback;聚合特徵仍 count-independent)。回傳 (live, had_bar_map)。
    """
    bar_of: dict[str, int] = {}
    last_bar_index = 0
    for m in snapshot.get("monowave_series") or []:
        if not isinstance(m, dict):
            continue
        idx = m.get("bar_indices") or [0, 0]
        try:
            s_idx, e_idx = int(idx[0]), int(idx[1])
        except (TypeError, ValueError, IndexError):
            continue
        bar_of[str(m.get("start_date"))] = s_idx
        bar_of[str(m.get("end_date"))] = e_idx
        last_bar_index = max(last_bar_index, e_idx)
    if not bar_of:
        return list(forest), False
    live = []
    for s in forest:
        end_bar = bar_of.get(str((s.get("wave_tree") or {}).get("end")))
        if end_bar is not None and end_bar >= last_bar_index - _LIVE_EDGE_BARS:
            live.append(s)
    return live, True


def _aggregate_features(
    snapshot: dict, live: list[dict],
) -> tuple[float | None, dict[str, float] | None, int | None]:
    """計數無關聚合:(up_share, invalidation_band, ambiguity_count)。

    up_share = 有方向性前瞻(見 _behavior_is_directional)且 power sign ≠ 0
    的 live 候選中,sign > 0 的比例(等權);分母 0 → None。
    invalidation_band = live 候選全部 InvalidateScenario 價位的 {min, max}。
    ambiguity_count = 引擎 E4 live_edge_ambiguity.count(1.3.0 起;缺 → None)。
    """
    ups = downs = 0
    thresholds: list[float] = []
    for s in live:
        if _behavior_is_directional(s.get("post_pattern_behavior")):
            d = _direction_from_power(s.get("power_rating"))
            if d == "bullish":
                ups += 1
            elif d == "bearish":
                downs += 1
        thresholds.extend(v for _, v in _extract_all_invalidation_thresholds(s))
    up_share = ups / (ups + downs) if (ups + downs) else None
    band = {"min": min(thresholds), "max": max(thresholds)} if thresholds else None
    ambiguity = snapshot.get("live_edge_ambiguity") or {}
    count = ambiguity.get("count") if isinstance(ambiguity, dict) else None
    return up_share, band, count


# ─── Invalidation(A-3 閘門前置)──────────────────────────────────────────────


def _extract_invalidation_price(scenario: dict, direction: str) -> float | None:
    """從 scenario.invalidation_triggers 抽 InvalidateScenario + PriceBreakBelow/Above。

    對齊 fusion._picker.canonical_is_invalidated 的解析(b1 canonical):
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
    """讀 structural_snapshots(+ wave_judgments)→ Track1View。

    wave_judgment_loop §8:
    - 有 active judgment 且 accepted[preferred] 的 anchor_key 對得回最新
      forest → **judgment 路徑**(該候選的 pattern / fib zones / 失效價 /
      A-3 閘門;source="judgment")
    - 無 judgment、no_fit 判讀、或 anchor 對不回(J2 diff 責任區)→
      **aggregate 路徑**:計數無關特徵(up_share / invalidation_band /
      ambiguity_count),`up_share > 0.6` → bullish、`< 0.4` → bearish、
      其餘(含分母 0)→ "undecided";fib_lines = flat_fib_zones 聯集
      (引擎聚合,無選取);無單一 thesis → invalidated 恆 False

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
            direction="undecided", effective_degree=None, wave_count=0,
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

    forest = [s for s in (snapshot.get("scenario_forest") or []) if isinstance(s, dict)]
    if not forest:
        return Track1View(
            stock_id=stock_id, as_of=as_of, snapshot_date=snapshot_date,
            has_snapshot=True, pattern_type=None, power_rating=None,
            direction="undecided", effective_degree=None, wave_count=0,
            fib_lines=[], notes=["empty scenario_forest"],
        )

    notes: list[str] = []

    # ── active judgment 查找(§8 消費優先序:human 先、其次最新)──────────
    judgment: dict | None = None
    try:
        judgment = fetch_active_judgment(conn, stock_id=stock_id, timeframe=timeframe)
    except Exception as e:
        notes.append(f"judgment lookup failed({type(e).__name__})→ aggregate 路徑")

    primary: dict | None = None
    judgment_id: int | None = None
    if judgment is not None:
        preferred_key = next(
            (a.get("anchor_key") for a in judgment.get("accepted") or []
             if isinstance(a, dict) and a.get("role") == "preferred"),
            None,
        )
        if preferred_key is None:
            notes.append(
                f"active judgment #{judgment.get('id')} 為 no_fit(無 preferred)→ aggregate 路徑"
            )
        else:
            primary = next(
                (s for s in forest if scenario_anchor_key(s) == preferred_key), None
            )
            if primary is None:
                notes.append(
                    f"active judgment #{judgment.get('id')} preferred anchor 不在最新 "
                    f"forest → 降級 aggregate(J2 diff 責任區,emitter/track1 不代判)"
                )
            else:
                judgment_id = judgment.get("id")

    # ── 聚合特徵(兩路徑都計;judgment 路徑作附帶脈絡)─────────────────────
    live, had_bar_map = _live_scenarios(snapshot, forest)
    up_share, invalidation_band, ambiguity_count = _aggregate_features(snapshot, live)
    if not had_bar_map:
        notes.append("無 monowave bar 對映(舊 snapshot)→ 全 forest 視為 live 聚合")

    source = "judgment" if primary is not None else "aggregate"

    if primary is None:
        # ── aggregate 路徑:無選取,無單一 thesis ──────────────────────────
        if up_share is None:
            direction = "undecided"
        elif up_share > 0.6:
            direction = "bullish"
        elif up_share < 0.4:
            direction = "bearish"
        else:
            direction = "undecided"

        flat = snapshot.get("flat_fib_zones") or []
        raw_fib_lines = [fl for fl in (_zone_to_fib_line(z) for z in flat) if fl is not None]
        fib_lines, n_raw, was_reduced = _cluster_and_cap_fib_lines(raw_fib_lines)
        if not fib_lines:
            notes.append("no fib zones (flat_fib_zones empty;aggregate 路徑無候選選取)")
        if was_reduced:
            notes.append(
                f"fib_lines reduced {n_raw} → {len(fib_lines)} "
                f"(1% bucket cluster + cap {FIB_LINES_MAX_COUNT};對齊 MCP context budget)"
            )
        notes.append(
            f"aggregate 路徑(無 active judgment):up_share={up_share} "
            f"→ direction={direction};判讀請走 dossier + judgment submit"
        )
        return Track1View(
            stock_id=stock_id, as_of=as_of, snapshot_date=snapshot_date,
            has_snapshot=True, pattern_type=None, power_rating=None,
            direction=direction, effective_degree=None, wave_count=0,
            fib_lines=fib_lines,
            invalidation_price=None,
            invalidated=False,
            fallback_to_flat_union=bool(fib_lines),
            notes=notes,
            source=source, judgment_id=None, up_share=up_share,
            invalidation_band=invalidation_band, ambiguity_count=ambiguity_count,
        )

    # ── judgment 路徑:accepted[preferred] 候選 ────────────────────────────
    pattern_label = _pattern_type_label(primary.get("pattern_type"))
    direction = _direction_from_power(primary.get("power_rating"))
    power_label = _power_rating_label(primary.get("power_rating"))
    degree = _effective_degree(primary)
    # compaction v2 §7.4 / Q6:wave_count 只讀結構化欄(字串 parse 已移除;
    # structure_label 新格式 `{Pattern} L{degree} [...]` 僅供顯示)
    wave_count = int(primary.get("wave_count") or 0)

    # Fib zones — preferred 候選優先,fallback flat_fib_zones
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
    # A-3 閘門實際判定:依 direction filter thresholds(B3:對齊 b1 canonical 統一
    # 讀寫面 — bullish 只看 PriceBreakBelow / bearish 只看 PriceBreakAbove /
    # neutral 不濾)。v4.25.x 「neutral 走 ALL kinds」自此退役 — 對齊 b1 skill
    # 「禁 direction-blind fallback」原則 + final output reliability(LLM 看到
    # neutral mode 時不會被 direction-blind 誤判 invalidated)。
    all_thresholds = _extract_all_invalidation_thresholds(primary)
    if direction == "bullish":
        # bullish 只看 PriceBreakBelow(current 跌破 → thesis 破)
        relevant_thresholds = [t for t in all_thresholds if t[0] == "below"]
    elif direction == "bearish":
        # bearish 只看 PriceBreakAbove(current 漲破 → thesis 破)
        relevant_thresholds = [t for t in all_thresholds if t[0] == "above"]
    else:
        # B3:neutral 不濾(canonical b1 spec)— neutral 無方向性 thesis,
        # 不可能被 invalidation trigger 「破」。MCP / LLM 應顯示 mode: neutral
        # (no directional thesis)而非錯誤的 invalidation 警示。
        relevant_thresholds = []
    invalidated, fired_kind, fired_threshold = _check_any_threshold_breached(
        relevant_thresholds, current_price
    )

    notes.append(f"judgment 路徑:active judgment #{judgment_id} accepted[preferred]")
    if not fib_lines:
        notes.append("no fib zones (neither preferred.expected_fib_zones nor flat_fib_zones populated)")
    if fallback_used:
        notes.append("fib_lines from flat_fib_zones fallback (preferred.expected_fib_zones empty)")
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
        source=source, judgment_id=judgment_id, up_share=up_share,
        invalidation_band=invalidation_band, ambiguity_count=ambiguity_count,
    )
