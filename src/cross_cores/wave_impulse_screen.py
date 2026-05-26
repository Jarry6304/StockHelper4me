"""
cross_cores/wave_impulse_screen.py
==================================
Wave Impulse Cross-Stock Screen — r3 post-correction entry pivot。

## r3 pivot rationale

r1/r2 設計「掃 incomplete Impulse 找 W3 主升段早段」在 production verify
(2026-05-27 全市場 1152 stocks)揭露 **neely_core forest 對 Impulse/Diagonal
emit wave_count=5 100%** — 完全不留半完成假設。結果 candidates 永遠 = 0。

r3 改為對齊 NEoWave 真正可掃選的訊號:**3-wave Zigzag/Flat 修正剛完成 + 方向
DOWN** → 預期新 impulse 啟動(對齊 NEoWave「A-B-C 結束後啟動新 impulse」)。

## 處理流程

- 讀 structural_snapshots(neely_core)→ `_pick_recent_correction` 找最近
  RECENT_DAYS=14 內完成的 Zigzag/Flat scenario(fallback Impulse / canonical)
- `current_wave_position`:
  - Zigzag/Flat + rightmost end 在 RECENT_DAYS 內 + direction=down →
    `CORRECTION_DONE_DOWN` candidate
  - Zigzag/Flat + direction=up → `CORRECTION_DONE_UP` observe(空頭 setup)
  - Zigzag/Flat + 過 RECENT_DAYS → `CORRECTION_ONGOING` observe
  - Impulse/Diagonal → `IMPULSE_COMPLETE` observe(反轉警示)
  - 其他 → `OTHER`
- R/R 計算(only candidate):target = expected_fib_zones [1.382, 2.618] zone
  midpoint;invalidation = `below` kind 最高 threshold
- per-tf 獨立 row(PK 含 timeframe);第二輪 pass 算 cross_tf_aligned 軟對齊

## 設計約束(對齊 cores_overview §四 + §十四)

- 零耦合:只讀 structural_snapshots JSONB + price_daily_fwd,不 reach into Rust
- 不抽象:per-stock 邏輯 inline 寫,picker 函式從 fusion/dual_track/track1.py
  inline import(underscore-private 同 repo 內 OK,picker 抽 _picker.py 留下個 PR)
- best-guess thresholds(RECENT_DAYS=14 / RR_MIN=1.5)走 module 常數,
  production verify 後 calibrate(對齊 v3.32 F-Score 7→6 hotfix pattern)

## Refs

- NEoWave Ch6/7:corrective pattern 收尾後啟動新 impulse(Glenn Neely 1990
  "Mastering Elliott Wave")
- Prechter & Frost (1978) "Elliott Wave Principle" Ch.2 主升段定義
- r3 pivot 完整討論 + production data 見 commit message + plan
  /root/.claude/plans/wave-impulse-cross-stock-virtual-papert.md
"""

from __future__ import annotations

import logging
import time
from datetime import date
from itertools import groupby
from typing import Any

from silver._common import upsert_silver

from cross_cores._shared import empty_row, fetch_universe_filter

# Inline import — picker 對齊 v4.25.x canonical(track1.py)
# Out of scope:抽 src/fusion/_picker.py 共用(本 PR follow-up issue)
from fusion.dual_track.track1 import (  # noqa: F401
    _DEGREE_RANK,
    _direction_from_power,
    _effective_degree,
    _extract_all_invalidation_thresholds,
    _pattern_type_label,
    _pick_primary,
    _power_rating_label,
    _power_rating_strength,
    _wave_count_from_label,
)

logger = logging.getLogger("collector.cross_cores.wave_impulse_screen")


# ────────────────────────────────────────────────────────────
# Builder 契約
# ────────────────────────────────────────────────────────────


NAME            = "wave_impulse_screen"
OUTPUT_TABLE    = "wave_impulse_screen_derived"
UPSTREAM_TABLES = ["structural_snapshots", "price_daily_fwd", "stock_info_ref"]

# r3 pivot(production verify 揭露 neely_core forest 不 emit incomplete Impulse,
# wave_count=5 100%)— 改抓「3-wave Zigzag/Flat 修正剛完成」訊號:
#   - DOWN 修正剛完成 → 反彈/新 impulse 啟動 → 多方 candidate
#   - UP 修正剛完成 → 已漲完 ABC,可能轉跌 → observe
#   - Impulse complete → observe(對齊 W5 mature 反轉警示)
#
# 對齊 NEoWave Ch6/7 corrective pattern terminate → 新 impulse 啟動的判讀。

# Best-guess thresholds(production verify 後 calibrate)
RECENT_DAYS          = 14    # rightmost 在過去 N 天內視為「剛完成」
RR_MIN               = 1.5   # R/R 最小門檻
TOP_N                = 30
MAX_UPSIDE_MULTIPLE  = 2.0   # r6:target / current 上限(過濾異常 fib 投影,e.g. IPO 高點)
TIMEFRAMES           = ("daily", "weekly", "monthly")

# Axis-B label sets(對齊 output.rs:1310-1346 StructureLabel enum)
_LABELS_FIVE_MATURE = {"L5", "S5", "SL5"}    # Last impulse 訊號(C-wave 收尾)

# Phase enum(r3,字串常數)
PHASE_CORRECTION_DONE_DOWN = "CORRECTION_DONE_DOWN"  # 向下 ABC 剛完成 → 多頭反轉 candidate
PHASE_CORRECTION_DONE_UP   = "CORRECTION_DONE_UP"    # 向上 ABC 剛完成 → 空頭反轉 observe
PHASE_CORRECTION_ONGOING   = "CORRECTION_ONGOING"    # 修正中(rightmost 未到 RECENT_DAYS)
PHASE_IMPULSE_COMPLETE     = "IMPULSE_COMPLETE"      # 完整 5 波 Impulse,observe(對齊 W5 reverse)
PHASE_OTHER                = "OTHER"

# is_candidate=True 的 phase 集合(r3 預設多頭單向)
_CANDIDATE_PHASES = {PHASE_CORRECTION_DONE_DOWN}
# emit row 但 is_candidate=False 的 phase 集合
_OBSERVE_PHASES   = {
    PHASE_CORRECTION_DONE_UP,
    PHASE_CORRECTION_ONGOING,
    PHASE_IMPULSE_COMPLETE,
}


# ────────────────────────────────────────────────────────────
# Axis-B label parsing(StructureLabel enum 序列化)
# ────────────────────────────────────────────────────────────


def _label_string(label_field: Any) -> str | None:
    """StructureLabel 在 JSON 序列化成 enum variant string(對齊 output.rs:1310-1346)。

    serde 對 unit-variant enum 序列化成裸字串("L5"),但有些變體可能 dict-wrapped。
    """
    if isinstance(label_field, str):
        return label_field
    if isinstance(label_field, dict):
        return next(iter(label_field.keys()), None)
    return None


def _axis_b_label(scenario: dict, last_n: int) -> str | None:
    """從 scenario.monowave_structure_labels 找對應 monowave 的 Pass-2 label。

    對齊 plan §3 Step 2 + output.rs:457-481(JSON key 是 `labels`,不是
    `pass2_labels`;v4.10+ refilled 後是 Pass 2 result;v4.10 前老 snapshot
    `labels` 可能空 list)。

    Returns:
        Pass-2 label 字串(e.g. "L5" / "F3"),空時回 None。
    """
    msl = scenario.get("monowave_structure_labels") or []
    target_idx = last_n - 1   # wave_tree W{n} → monowave_index zero-indexed
    if target_idx < 0:
        return None
    for entry in msl:
        if not isinstance(entry, dict):
            continue
        if entry.get("monowave_index") != target_idx:
            continue
        cands = entry.get("labels") or []
        if not cands:
            return None
        # 取首個 candidate 的 label
        first = cands[0]
        if not isinstance(first, dict):
            continue
        return _label_string(first.get("label"))
    return None


# ────────────────────────────────────────────────────────────
# 浪位判定:current_wave_position
# ────────────────────────────────────────────────────────────


def current_wave_position(scenario: dict, snapshot_date: date) -> dict[str, Any]:
    """r3 浪位判定:把 Zigzag/Flat 修正完成度 + 方向當主訊號。

    對齊 r3 pivot — neely_core 不 emit incomplete Impulse,實際 actionable 訊號
    是「3-wave Zigzag/Flat 修正剛完成」→ 反轉 / 新 impulse 啟動。

    Returns:
        {
            "phase":            PHASE_* 常數
            "direction":        "up" / "down" / None(從 rightmost label 解析)
            "rightmost_end":    date | None
            "days_since":       int | None(snapshot - rightmost_end)
            "axis_b_label":     str | None
            "confidence_level": "strict" / "loose"
            "is_candidate":     bool
            "excluded_reason":  str | None
        }
    """
    base: dict[str, Any] = {
        "phase": PHASE_OTHER, "direction": None,
        "rightmost_end": None, "days_since": None,
        "axis_b_label": None, "confidence_level": "loose",
        "is_candidate": False, "excluded_reason": None,
    }

    is_correction, pat_label = _pattern_kind_ok(scenario)

    # ─ Branch A:correction 系(Zigzag / Flat)— primary candidate path
    if is_correction:
        end_date = _rightmost_end_date(scenario)
        base["rightmost_end"] = end_date
        if end_date is not None:
            base["days_since"] = max(0, (snapshot_date - end_date).days)

        direction = _correction_direction(scenario)
        base["direction"] = direction

        # Axis-B label(rightmost C-wave 的 Pass-2 label)— 純 informative
        # corrective rightmost = C-wave,index = len(children) - 1
        wt = scenario.get("wave_tree") or {}
        children = wt.get("children") or []
        if children:
            axis_b = _axis_b_label(scenario, len(children))
            base["axis_b_label"] = axis_b
            base["confidence_level"] = "strict" if axis_b else "loose"

        # 收斂狀態
        if base["days_since"] is None:
            base["phase"] = PHASE_OTHER
            base["excluded_reason"] = "no_end_date"
            return base

        if base["days_since"] > RECENT_DAYS:
            base["phase"] = PHASE_CORRECTION_ONGOING
            base["excluded_reason"] = "correction_stale"
            return base

        # 在 RECENT_DAYS 內完成 — 看方向
        if direction == "down":
            base["phase"] = PHASE_CORRECTION_DONE_DOWN
            base["is_candidate"] = True
        elif direction == "up":
            base["phase"] = PHASE_CORRECTION_DONE_UP
            base["excluded_reason"] = "bearish_setup_observe_only"
        else:
            base["phase"] = PHASE_OTHER
            base["excluded_reason"] = "no_direction"
        return base

    # ─ Branch B:impulse 系(Impulse / Diagonal)— observe path(完整 5 波)
    if _pattern_is_impulse(scenario):
        end_date = _rightmost_end_date(scenario)
        base["rightmost_end"] = end_date
        if end_date is not None:
            base["days_since"] = max(0, (snapshot_date - end_date).days)
        base["direction"] = _correction_direction(scenario)  # 同 parser
        # Axis-B rightmost label(C-wave / W5)
        wt = scenario.get("wave_tree") or {}
        children = wt.get("children") or []
        if children:
            axis_b = _axis_b_label(scenario, len(children))
            base["axis_b_label"] = axis_b
            base["confidence_level"] = "strict" if axis_b else "loose"
        base["phase"] = PHASE_IMPULSE_COMPLETE
        base["excluded_reason"] = "impulse_complete_observe"
        return base

    # ─ Branch C:Triangle / Combination / RunningCorrection / 未知 — OTHER
    base["phase"] = PHASE_OTHER
    base["excluded_reason"] = f"non_corrective_pattern_{pat_label or 'unknown'}"
    return base


def _pick_recent_correction(
    forest: list[dict], snapshot_date: date,
) -> dict | None:
    """r3 wave_screen 專屬 picker — 找最近完成的 3-wave Zigzag/Flat correction。

    r2 production verify(2026-05-27,全市場 1152 stocks)揭露 neely_core forest
    **完全不 emit incomplete Impulse**(wave_count=5 100%)。要找 actionable
    訊號必須改抓 corrective pattern 收尾 → 反轉/新 impulse 啟動。

    r3 picker 策略:
    1. Filter Zigzag/Flat scenarios(correction 系)
    2. Filter rightmost end ∈ [snapshot - RECENT_DAYS, snapshot](剛完成)
    3. 排序 (end_date DESC, degree↓, power↓, rules↓);取最近最強
    4. 若無 recent correction → fallback 最近的 Impulse complete scenario
       (emit IMPULSE_COMPLETE observe row)
    5. 若連 Impulse 都沒 → fallback canonical _pick_primary

    這對齊 NEoWave「A-B-C 結束後啟動新 impulse」設計精神。
    """
    if not forest:
        return None

    # Step 1-3:recent corrections
    recent_corrections: list[tuple[date, dict]] = []
    for s in forest:
        is_corr, _ = _pattern_kind_ok(s)
        if not is_corr:
            continue
        end = _rightmost_end_date(s)
        if end is None:
            continue
        days_since = (snapshot_date - end).days
        if 0 <= days_since <= RECENT_DAYS:
            recent_corrections.append((end, s))

    if recent_corrections:
        recent_corrections.sort(key=lambda t: (
            t[0],   # end_date DESC(取最近完成)
            _DEGREE_RANK.get(_effective_degree(t[1]) or "", 0),
            _power_rating_strength(t[1].get("power_rating")),
            int(t[1].get("rules_passed_count") or 0),
        ), reverse=True)
        return recent_corrections[0][1]

    # Step 4:fallback 找最近完成的 Impulse(emit observe IMPULSE_COMPLETE)
    impulses: list[tuple[date, dict]] = []
    for s in forest:
        if not _pattern_is_impulse(s):
            continue
        end = _rightmost_end_date(s)
        if end is None:
            continue
        impulses.append((end, s))
    if impulses:
        impulses.sort(key=lambda t: (
            t[0],
            _DEGREE_RANK.get(_effective_degree(t[1]) or "", 0),
            _power_rating_strength(t[1].get("power_rating")),
        ), reverse=True)
        return impulses[0][1]

    # Step 5:final fallback — canonical picker
    return _pick_primary(forest)


def _pattern_kind_ok(scenario: dict) -> tuple[bool, str | None]:
    """r3 pattern_type gate:Zigzag / Flat 視為 correction 系(primary candidate);
    Impulse / Diagonal 視為 impulse 完成系(observe);其他歸 OTHER。

    回 (is_correction, pattern_label)。
    - (True, "Zigzag"/"Flat"):corrective scenario,主要判斷對象
    - (False, "Impulse"/"Diagonal"):完整 5 波,emit observe row
    - (False, 其他):OTHER
    """
    label = _pattern_type_label(scenario.get("pattern_type"))
    if label in ("Zigzag", "Flat"):
        return True, label
    return False, label


def _pattern_is_impulse(scenario: dict) -> bool:
    """檢查是否為完整 Impulse / Diagonal(供 IMPULSE_COMPLETE fallback)。"""
    label = _pattern_type_label(scenario.get("pattern_type"))
    return label in ("Impulse", "Diagonal")


def _rightmost_end_date(scenario: dict) -> date | None:
    """從 wave_tree.children rightmost 抽 end date(NaiveDate ISO 字串)。"""
    wt = scenario.get("wave_tree") or {}
    children = wt.get("children") or []
    if not children:
        end = wt.get("end")
    else:
        last = children[-1] if isinstance(children[-1], dict) else None
        if last is None:
            return None
        end = last.get("end") or wt.get("end")
    if isinstance(end, date):
        return end
    if isinstance(end, str):
        try:
            return date.fromisoformat(end[:10])
        except ValueError:
            return None
    return None


def _correction_direction(scenario: dict) -> str | None:
    """從 rightmost child label 解析 correction 方向(對齊 wave_tree.children label
    格式:`W{n}:{Hint}{Dir}` 例 `W3:L5↓` / `W3:Five↑`,或 `3-wave Up/Down`)。

    Returns: "up" / "down" / None
    """
    wt = scenario.get("wave_tree") or {}
    children = wt.get("children") or []
    if not children:
        wt_label = wt.get("label") or ""
        return _parse_direction(wt_label)
    last = children[-1] if isinstance(children[-1], dict) else None
    if last is None:
        return None
    return _parse_direction(last.get("label") or "")


def _parse_direction(label: str) -> str | None:
    """Parse direction arrow / 'Up'/'Down' from label string。"""
    if not label:
        return None
    if "↑" in label or "Up" in label:
        return "up"
    if "↓" in label or "Down" in label:
        return "down"
    return None


def _extract_target_price(scenario: dict) -> float | None:
    """r1/r2 W3 entry path:從 expected_fib_zones 抽 [1.382, 2.618] expansion midpoint。

    r3 corrective entry 不該用此函式 — 走 _extract_reversal_target_upside
    (對齊 NEoWave「修正完成 → 反彈到 fib zone」)。
    """
    zones = scenario.get("expected_fib_zones") or []
    candidates: list[float] = []
    for z in zones:
        if not isinstance(z, dict):
            continue
        sr = z.get("source_ratio")
        if not isinstance(sr, (int, float)) or isinstance(sr, bool):
            continue
        if not (1.382 <= float(sr) <= 2.618):
            continue
        lo, hi = z.get("low"), z.get("high")
        if not isinstance(lo, (int, float)) or not isinstance(hi, (int, float)):
            continue
        if isinstance(lo, bool) or isinstance(hi, bool):
            continue
        candidates.append((float(lo) + float(hi)) / 2.0)
    if not candidates:
        return None
    return min(candidates)


def _extract_reversal_target_upside(
    scenario: dict, current_price: float | None,
    *, max_multiple: float = MAX_UPSIDE_MULTIPLE,
) -> float | None:
    """r3 CORRECTION_DONE_DOWN entry path:取 expected_fib_zones 內 midpoint
    在 (current_price, current × max_multiple] 區間最近的 zone。

    Rationale:
    - r5 揭露 r1/r2 用 `source_ratio ∈ [1.382, 2.618]` 篩 fib zones(= Impulse
      W3/W5 expansion 投影),對 Zigzag/Flat 修正完成後反彈 target 完全錯方向
      (production 233/237 case target_below_current)。
    - r6 改為「方向 + 量級 sanity」filter:fib midpoint 必須在現價之上,且
      不超過現價 × max_multiple(預設 2.0 = 100% upside 上限),過濾 7780 type
      outlier(target 879 vs entry 18 → RR=1105 異常)。
    - 取 MIN of qualifying = 最近的 upside target(對齊 NEoWave「最近 fib 投影
      最可能命中」)。
    """
    if current_price is None:
        return None
    cp = float(current_price)
    cap = cp * max_multiple
    candidates: list[float] = []
    for z in scenario.get("expected_fib_zones") or []:
        if not isinstance(z, dict):
            continue
        lo, hi = z.get("low"), z.get("high")
        if not isinstance(lo, (int, float)) or not isinstance(hi, (int, float)):
            continue
        if isinstance(lo, bool) or isinstance(hi, bool):
            continue
        mid = (float(lo) + float(hi)) / 2.0
        if cp < mid <= cap:
            candidates.append(mid)
    if not candidates:
        return None
    return min(candidates)


def _extract_below_invalidation(scenario: dict) -> float | None:
    """r1/r2 W3 entry path:取 MAX of below triggers(最緊 stop)。

    r3 corrective entry 不該用此函式 — 走 _extract_correction_stop(MIN)。
    """
    thresholds = _extract_all_invalidation_thresholds(scenario)
    below = [v for k, v in thresholds if k == "below"]
    return max(below) if below else None


def _extract_correction_stop(scenario: dict) -> float | None:
    """r3 CORRECTION_DONE_DOWN entry path:取 MIN of below triggers
    (對 corrective bottom 抽 stop loss,= 修正實際低點)。

    Rationale:NEoWave 對 Zigzag/Flat scenario emit 多筆 PriceBreakBelow
    triggers 對應不同 sub-hypothesis(e.g. W1 low / W3 low / extended target)。
    對 corrective 完成後 buy entry,正確 stop 是「最低點」(MIN)— 不是最緊。
    取 MAX 會抓到 corrective 開始的高點,落在現價之上 → invalid_rr_geometry。
    """
    thresholds = _extract_all_invalidation_thresholds(scenario)
    below = [v for k, v in thresholds if k == "below"]
    return min(below) if below else None


# ────────────────────────────────────────────────────────────
# 主入口
# ────────────────────────────────────────────────────────────


def _fetch_all_latest_prices(db: Any, *, market: str = "TW") -> dict[str, float]:
    """全市場各股最新 close ≤ today(對齊 fetch_close_series 但 batch)。"""
    rows = db.query(
        """
        SELECT DISTINCT ON (stock_id) stock_id, close::float8 AS close
          FROM price_daily_fwd
         WHERE market = %s
         ORDER BY stock_id, date DESC
        """,
        [market],
    )
    return {r["stock_id"]: r["close"] for r in rows if r.get("close") is not None}


def _fetch_corrective_bottoms(
    db: Any, lookups: list[tuple[str, date]], *, market: str = "TW",
) -> dict[tuple[str, date], float]:
    """r5 batch lookup:對 (stock_id, rightmost_end_date) 取該日 close
    (= corrective C-wave 終點實際收盤,= 多單 stop loss 正確 anchor)。

    對應 r4 揭露的 root cause:NEoWave 對「已完成」Zigzag/Flat 多 emit
    PriceBreakAbove(invalidation = 價格漲回起點)而非 PriceBreakBelow。
    corrective bottom 必須從 price_daily_fwd 查實際價,不能依賴 triggers。

    Args:
        lookups: [(stock_id, target_date), ...]

    Returns: {(stock_id, date): close} dict
    """
    if not lookups:
        return {}
    # Build VALUES list 對齊 PostgreSQL batch lookup pattern
    # 對 date 加 cast(VALUES 內若全是 NULL/text PG 推不出 DATE 型別 → JOIN 比較炸)
    placeholders = ",".join(["(%s, %s::date)"] * len(lookups))
    params: list[Any] = []
    for sid, d in lookups:
        params.extend([sid, d])
    params.append(market)   # for the LATERAL subquery 的 market filter
    sql = f"""
        SELECT t.stock_id, t.date, p.close::float8 AS close
          FROM (VALUES {placeholders}) AS t(stock_id, date)
          LEFT JOIN LATERAL (
              SELECT close FROM price_daily_fwd
               WHERE market = %s AND stock_id = t.stock_id AND date <= t.date
               ORDER BY date DESC LIMIT 1
          ) AS p ON TRUE
    """
    rows = db.query(sql, params)
    out: dict[tuple[str, date], float] = {}
    for r in rows:
        sid = r.get("stock_id")
        d = r.get("date")
        c = r.get("close")
        if sid is not None and d is not None and c is not None:
            out[(sid, d)] = float(c)
    return out


def _fetch_structural_snapshots(
    db: Any, *, market: str = "TW",
) -> list[dict[str, Any]]:
    """全市場各 (stock_id, timeframe) 取最新 neely_core snapshot。

    Returns:
        [{stock_id, snapshot_date, timeframe, snapshot}, ...]
    """
    rows = db.query(
        """
        SELECT DISTINCT ON (stock_id, timeframe)
               stock_id, snapshot_date, timeframe, snapshot
          FROM structural_snapshots
         WHERE core_name = 'neely_core'
         ORDER BY stock_id, timeframe, snapshot_date DESC
        """,
        [],
    )
    # psycopg3 JSONB 自動 parse 成 dict;若是 str(舊 driver / fixture)走 json.loads
    out: list[dict[str, Any]] = []
    for r in rows:
        snap = r.get("snapshot")
        if isinstance(snap, str):
            import json
            try:
                snap = json.loads(snap)
            except Exception:
                snap = {}
        if not isinstance(snap, dict):
            continue
        out.append({
            "stock_id": r.get("stock_id"),
            "snapshot_date": r.get("snapshot_date"),
            "timeframe": r.get("timeframe"),
            "snapshot": snap,
        })
    return out


def _build_row(
    *,
    stock_id: str,
    target_date: Any,
    timeframe: str,
    snapshot: dict | None,
    current_price: float | None,
    excluded_reason: str | None,
    snapshot_date: date | None = None,
) -> dict[str, Any]:
    """組裝 single row。r3 pivot:r3 picker 找 recent correction,phase 反映
    correction 完成度 + 方向。"""
    extras: dict[str, Any] = {
        "timeframe": timeframe,
        "phase": None, "wave_number": None, "pattern_kind": None,
        "direction": None, "effective_degree": None, "structure_label": None,
        "confidence_level": "loose",
        "entry_price": None, "target_price": None, "invalidation_price": None,
        "rr_ratio": None, "cross_tf_aligned": False,
        "impulse_rank": None, "is_candidate": False, "detail": None,
    }
    if excluded_reason is not None or snapshot is None:
        return empty_row(stock_id, target_date,
                         excluded_reason=excluded_reason or "no_snapshot",
                         extras=extras)

    forest = snapshot.get("scenario_forest") or []

    # snapshot_date(for recency 計算);若 None 用 target_date fallback
    eff_snapshot_date: date | None = snapshot_date
    if eff_snapshot_date is None:
        if isinstance(target_date, date):
            eff_snapshot_date = target_date
        elif isinstance(target_date, str):
            try:
                eff_snapshot_date = date.fromisoformat(target_date[:10])
            except ValueError:
                eff_snapshot_date = None
    if eff_snapshot_date is None:
        return empty_row(stock_id, target_date,
                         excluded_reason="no_snapshot_date", extras=extras)

    # r3 picker:找最近完成的 Zigzag/Flat → fallback Impulse → fallback _pick_primary
    primary = _pick_recent_correction(forest, eff_snapshot_date)
    if primary is None:
        extras["confidence_level"] = "loose"
        return empty_row(stock_id, target_date,
                         excluded_reason="empty_forest", extras=extras)

    # pattern_kind label
    _is_corr, pt_label = _pattern_kind_ok(primary)
    extras["pattern_kind"] = pt_label or _pattern_type_label(primary.get("pattern_type"))

    # 浪位判定(r3 logic — Zigzag/Flat correction-aware)
    pos = current_wave_position(primary, eff_snapshot_date)
    extras["phase"]            = pos["phase"]
    extras["confidence_level"] = pos["confidence_level"]
    extras["structure_label"]  = pos["axis_b_label"]
    extras["direction"]        = pos["direction"]    # r3:from rightmost label
    extras["effective_degree"] = _effective_degree(primary)

    # detail JSONB
    extras["detail"] = {
        "pattern_type_full": primary.get("pattern_type"),
        "power_rating": _power_rating_label(primary.get("power_rating")),
        "power_strength": _power_rating_strength(primary.get("power_rating")),
        "scenario_count": len(forest),
        "snapshot_date": str(snapshot.get("date") or ""),
        "rightmost_end": pos["rightmost_end"].isoformat()
                          if pos.get("rightmost_end") else None,
        "days_since_completion": pos.get("days_since"),
    }

    is_candidate = bool(pos["is_candidate"])
    excluded: str | None = pos["excluded_reason"]

    # R/R 計算(only candidate = CORRECTION_DONE_DOWN — 預期 UP 反轉)
    # r4 用 _extract_correction_stop(MIN of below triggers)取 corrective bottom,
    # 配合幾何 sanity check(target > current > invalidation)
    if is_candidate and current_price is not None:
        invalidation = _extract_correction_stop(primary)
        # r6:target 走「nearest upside fib zone within max_multiple」
        target = _extract_reversal_target_upside(primary, current_price)
        extras["entry_price"] = float(current_price)
        extras["invalidation_price"] = invalidation
        extras["target_price"] = target

        if target is None:
            is_candidate = False
            excluded = "no_target"
        elif invalidation is None:
            is_candidate = False
            excluded = "no_invalidation"
        elif target <= current_price:
            is_candidate = False
            excluded = "target_below_current"
        elif invalidation >= current_price:
            is_candidate = False
            excluded = "stop_above_current"
        else:
            rr = (target - current_price) / (current_price - invalidation)
            extras["rr_ratio"] = round(rr, 4) if rr > 0 else None
            if extras["rr_ratio"] is None or extras["rr_ratio"] < RR_MIN:
                is_candidate = False
                excluded = "rr_below_threshold"
    elif is_candidate:  # current_price None
        is_candidate = False
        excluded = "no_current_price"

    extras["is_candidate"] = is_candidate
    return empty_row(stock_id, target_date,
                     excluded_reason=excluded, extras=extras)


def _apply_cross_tf_alignment(rows: list[dict[str, Any]]) -> None:
    """第二輪 pass:同股 daily+weekly 同向 W3 → cross_tf_aligned=True(in-place)。

    對齊 plan §5 軟對齊。
    """
    rows.sort(key=lambda r: (r.get("stock_id") or ""))
    for sid, sid_rows_iter in groupby(rows, key=lambda r: r.get("stock_id")):
        sid_rows = list(sid_rows_iter)
        daily = next((r for r in sid_rows if r.get("timeframe") == "daily"), None)
        weekly = next((r for r in sid_rows if r.get("timeframe") == "weekly"), None)
        if not (daily and weekly):
            continue
        d_phase = daily.get("phase")
        w_phase = weekly.get("phase")
        if d_phase not in _CANDIDATE_PHASES or w_phase not in _CANDIDATE_PHASES:
            continue
        if daily.get("direction") != weekly.get("direction"):
            continue
        for r in sid_rows:
            r["cross_tf_aligned"] = True


def _assign_impulse_ranks(rows: list[dict[str, Any]]) -> None:
    """對 is_candidate=True 的 row 按 (cross_tf_aligned↓, rr_ratio↓, power_strength↓) 排,
    top_n 標 is_top_n(in-place)。"""
    eligibles = [r for r in rows if r.get("is_candidate") is True]
    # per timeframe 各自 rank(對齊 plan §7;不同 tf 量級不同不混合)
    eligibles.sort(key=lambda r: (r.get("timeframe") or "",))
    for tf, tf_iter in groupby(eligibles, key=lambda r: r.get("timeframe")):
        tf_rows = list(tf_iter)
        tf_rows.sort(
            key=lambda r: (
                int(bool(r.get("cross_tf_aligned"))),
                float(r.get("rr_ratio") or 0),
                int((r.get("detail") or {}).get("power_strength") or 0),
            ),
            reverse=True,
        )
        n = len(tf_rows)
        for i, r in enumerate(tf_rows, 1):
            r["impulse_rank"] = i
            r["universe_size"] = n
        for r in tf_rows[:TOP_N]:
            r["is_top_n"] = True


def _populate_corrective_bottoms_and_rescore(
    db: Any, rows: list[dict[str, Any]],
) -> None:
    """r5 pass:對 phase=CORRECTION_DONE_DOWN + excluded=no_invalidation 的 row,
    從 price_daily_fwd 查 rightmost_end date 的 close 當 invalidation。

    對應 r4 揭露 — NEoWave 對「已完成」Zigzag/Flat 多不 emit PriceBreakBelow
    trigger。corrective bottom 必須查實際價,不能依賴 invalidation_triggers。

    In-place 更新 rows:重設 invalidation_price / rr_ratio / is_candidate /
    excluded_reason / detail.invalidation_source。
    """
    # 收集需要 lookup 的 (stock_id, rightmost_end) pairs
    needs_lookup: list[tuple[int, str, date]] = []  # (row_idx, stock_id, end_date)
    for idx, r in enumerate(rows):
        if r.get("phase") != PHASE_CORRECTION_DONE_DOWN:
            continue
        if r.get("excluded_reason") != "no_invalidation":
            continue
        sid = r.get("stock_id")
        end_iso = (r.get("detail") or {}).get("rightmost_end")
        if not sid or not end_iso:
            continue
        try:
            end_date = date.fromisoformat(end_iso[:10])
        except ValueError:
            continue
        needs_lookup.append((idx, sid, end_date))

    if not needs_lookup:
        return

    # Batch fetch close at rightmost_end
    lookup_keys = [(sid, d) for _, sid, d in needs_lookup]
    bottoms = _fetch_corrective_bottoms(db, lookup_keys)

    # Re-score 那些拿到 bottom 的 row
    for idx, sid, end_date in needs_lookup:
        bottom = bottoms.get((sid, end_date))
        if bottom is None:
            rows[idx]["excluded_reason"] = "no_price_at_correction_end"
            continue
        r = rows[idx]
        current = r.get("entry_price")
        target = r.get("target_price")
        if current is None or target is None:
            r["excluded_reason"] = "missing_entry_or_target"
            continue
        # 加 1% buffer:invalidation 設 corrective bottom × 0.99 對齊 NEoWave
        # 「不可剛好觸碰」實務 + 防 intraday wick
        invalidation = bottom * 0.99
        r["invalidation_price"] = round(invalidation, 4)
        detail = r.get("detail") or {}
        detail["invalidation_source"] = "price_daily_fwd_at_rightmost_end"
        detail["corrective_bottom_close"] = round(bottom, 4)
        r["detail"] = detail
        # 重評 geometry + rr
        if target <= current:
            r["excluded_reason"] = "target_below_current"
            continue
        if invalidation >= current:
            r["excluded_reason"] = "stop_above_current"
            continue
        rr = (target - current) / (current - invalidation)
        r["rr_ratio"] = round(rr, 4) if rr > 0 else None
        if r["rr_ratio"] is None or r["rr_ratio"] < RR_MIN:
            r["excluded_reason"] = "rr_below_threshold"
            continue
        r["is_candidate"] = True
        r["excluded_reason"] = None


def run(
    db: Any,
    stock_ids: list[str] | None = None,
    full_rebuild: bool = False,
    lookback_days: int | None = None,
) -> dict[str, Any]:
    start = time.monotonic()

    # 全市場 universe + 最新 close
    universe = fetch_universe_filter(db)
    prices = _fetch_all_latest_prices(db)

    # 全部 (stock, tf) latest snapshot
    snapshots_raw = _fetch_structural_snapshots(db)
    snap_by_key: dict[tuple[str, str], dict[str, Any]] = {
        (s["stock_id"], s["timeframe"]): s for s in snapshots_raw
    }

    # 用最新 snapshot_date 作為 target_date(若沒任何 snapshot → empty)
    if not snapshots_raw:
        elapsed_ms = int((time.monotonic() - start) * 1000)
        logger.info(f"[{NAME}] no structural_snapshots, skip ({elapsed_ms}ms)")
        return {"name": NAME, "rows_read": 0, "rows_written": 0,
                "elapsed_ms": elapsed_ms}
    target_date = max(s["snapshot_date"] for s in snapshots_raw)

    rows: list[dict[str, Any]] = []
    for sid, excluded in universe.items():
        if stock_ids and sid not in stock_ids:
            continue
        current_price = prices.get(sid)
        for tf in TIMEFRAMES:
            snap_entry = snap_by_key.get((sid, tf))
            snap = snap_entry["snapshot"] if snap_entry else None
            tf_snap_date = snap_entry["snapshot_date"] if snap_entry else target_date
            row_excluded = excluded if excluded is not None else None
            row = _build_row(
                stock_id=sid, target_date=target_date, timeframe=tf,
                snapshot=snap, current_price=current_price,
                excluded_reason=row_excluded,
                snapshot_date=tf_snap_date,
            )
            rows.append(row)

    # r5:對 CORRECTION_DONE_DOWN 但 no_invalidation 的 row,batch lookup 真實
    # corrective bottom(close at rightmost_end date),然後重新評 R/R
    _populate_corrective_bottoms_and_rescore(db, rows)

    # 第二輪:cross-tf 軟對齊
    _apply_cross_tf_alignment(rows)

    # 排名 + is_top_n
    _assign_impulse_ranks(rows)

    written = upsert_silver(db, OUTPUT_TABLE, rows,
                            pk_cols=["market", "stock_id", "date", "timeframe"])
    elapsed_ms = int((time.monotonic() - start) * 1000)
    candidates = sum(1 for r in rows if r.get("is_candidate") is True)
    logger.info(
        f"[{NAME}] rows={len(rows)} written={written} "
        f"candidates={candidates} ({elapsed_ms}ms)"
    )
    return {"name": NAME, "rows_read": len(rows), "rows_written": written,
            "candidates": candidates, "elapsed_ms": elapsed_ms}
