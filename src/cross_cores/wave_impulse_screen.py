"""
cross_cores/wave_impulse_screen.py
==================================
Wave Impulse Cross-Stock Screen — 全市場掃 neely_core forest 找「正進入 W3」候選。

對齊 plan `/root/.claude/plans/wave-impulse-cross-stock-virtual-papert.md`:
- 讀 structural_snapshots(neely_core)→ 套既有 picker 收斂 primary scenario
- 雙軸驗證浪位:Axis-A regex `W(\\d+)` 抽 wave_tree rightmost child 浪數;
  Axis-B 對 monowave_structure_labels[last_n-1].labels[0].label 對 NEoWave
  structure label(`L5/F3/C3/UnknownFive/...`)交叉驗
- 對 W2_DONE / W3_ONGOING 算 R/R(target = expected_fib_zones [1.382, 2.618]
  midpoint;invalidation = `below` kind 最高 threshold)
- per-tf 獨立 row(PK 含 timeframe);第二輪 pass 算 cross_tf_aligned 軟對齊

設計約束(對齊 cores_overview §四 + §十四):
- 零耦合:只讀 structural_snapshots JSONB + price_daily_fwd,不 reach into Rust
- 不抽象:per-stock 邏輯 inline 寫,picker 函式從 fusion/dual_track/track1.py
  inline import(underscore-private 同 repo 內 OK,picker 抽 _picker.py 留下個 PR)
- best-guess thresholds(W3_EARLY_PCT=0.5 / RR_MIN=1.5)走 module 常數,
  production verify 後 calibrate(對齊 v3.32 F-Score 7→6 hotfix pattern)

Refs:
  - NEoWave Ch5(Essential Rules)/ Ch7(Compaction)/ Ch11(Wave-by-Wave)
  - Prechter & Frost (1978). "Elliott Wave Principle". Ch.2 W3 主升段定義
  - Glenn Neely (1990). "Mastering Elliott Wave". Ch7 §Three Rounds Compaction
"""

from __future__ import annotations

import logging
import time
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

# Best-guess thresholds(production verify 後 calibrate;對齊 v3.32 F-Score 7→6 pattern)
W3_EARLY_PCT = 0.5      # Axis-B 缺時 W3 一律歸 ONGOING(寬鬆);實際 elapsed 比 r2 才動
RR_MIN       = 1.5      # R/R 最小門檻
TOP_N        = 30
TIMEFRAMES   = ("daily", "weekly", "monthly")

# Axis-B 對照表(NEoWave StructureLabel ⇔ Wave Position),對齊 output.rs:1310-1346
_LABELS_THREE = {"F3", "C3", "L3", "UnknownThree", "XC3", "BC3", "BF3", "SL3"}
_LABELS_FIVE_MATURE = {"L5", "S5", "SL5"}           # Last / Special — wave 結束訊號
_LABELS_FIVE_ONGOING = {"Five", "F5", "UnknownFive"}  # First / Unknown — wave 仍進行

# Phase enum(字串常數)
PHASE_W1_ONGOING = "W1_ONGOING"
PHASE_W2_DONE    = "W2_DONE"
PHASE_W3_ONGOING = "W3_ONGOING"
PHASE_W3_MATURE  = "W3_MATURE"
PHASE_W4_DONE    = "W4_DONE"
PHASE_W5_ONGOING = "W5_ONGOING"
PHASE_W5_MATURE  = "W5_MATURE"
PHASE_OTHER      = "OTHER"

# is_candidate=True 的 phase 集合(W3 主升段)
_CANDIDATE_PHASES = {PHASE_W2_DONE, PHASE_W3_ONGOING}
# emit row 但 is_candidate=False 的 phase 集合(W5 observe 等)
_OBSERVE_PHASES   = {PHASE_W4_DONE, PHASE_W5_ONGOING, PHASE_W5_MATURE, PHASE_W3_MATURE}


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


def current_wave_position(scenario: dict) -> dict[str, Any]:
    """雙軸驗證浪位(對齊 plan §3 演算法)。

    Returns:
        {
            "phase":            PHASE_* 常數
            "wave_number":      int | None — last_n(從 wave_tree 抽)
            "axis_b_label":     str | None — Pass-2 NEoWave label
            "confidence_level": "strict" / "loose"
            "is_candidate":     bool — W3 主升段才 True
            "emit_row":         bool — 是否寫 row(預設 True,只 None / 完全 broken 不 emit)
            "excluded_reason":  str | None — 不 candidate 時記原因
        }
    """
    base = {
        "phase": PHASE_OTHER, "wave_number": None, "axis_b_label": None,
        "confidence_level": "loose", "is_candidate": False, "emit_row": True,
        "excluded_reason": None,
    }

    # Step 1 Axis-A:wave_tree.children rightmost 取 W(\d+)
    wave_tree = scenario.get("wave_tree") or {}
    children = wave_tree.get("children") or []
    if not children:
        base["excluded_reason"] = "no_children"
        return base
    last_child = children[-1] if isinstance(children[-1], dict) else None
    if last_child is None:
        base["excluded_reason"] = "no_children"
        return base
    last_label = last_child.get("label") or ""
    import re
    m = re.match(r"W(\d+)", last_label)
    if not m:
        base["excluded_reason"] = "no_W_regex"
        return base
    last_n = int(m.group(1))
    base["wave_number"] = last_n

    # Step 2 Axis-B:monowave_structure_labels lookup
    axis_b = _axis_b_label(scenario, last_n)
    base["axis_b_label"] = axis_b
    base["confidence_level"] = "strict" if axis_b else "loose"

    # Step 3 對照表(r2:last_n 為 source of truth;Axis-B 只當 mature upgrade signal)
    # r1 把 label-table mismatch 當 excluded(224 row 在 production 變 label_mismatch),
    # r2 改為:同股不同 sub-pattern 的 wave 結構在 Impulse vs Diagonal 不同(Diagonal
    # W1/W3/W5 是 :3 不是 :5),強制單一 label table 反而漏掉合法 scenario。
    # 改用 last_n 直接判定 phase;Axis-B L5/S5 出現時升級為 mature。
    if last_n == 1:
        base["phase"] = PHASE_W1_ONGOING
        base["excluded_reason"] = "too_early"
    elif last_n == 2:
        base["phase"] = PHASE_W2_DONE
        base["is_candidate"] = True
    elif last_n == 3:
        if axis_b in _LABELS_FIVE_MATURE:
            base["phase"] = PHASE_W3_MATURE
            base["excluded_reason"] = "w3_mature"
        else:
            base["phase"] = PHASE_W3_ONGOING
            base["is_candidate"] = True
    elif last_n == 4:
        base["phase"] = PHASE_W4_DONE
        base["excluded_reason"] = "w5_observe_only"
    elif last_n == 5:
        if axis_b in _LABELS_FIVE_MATURE:
            base["phase"] = PHASE_W5_MATURE
        else:
            base["phase"] = PHASE_W5_ONGOING
        base["excluded_reason"] = "w5_observe_only"
    else:
        base["phase"] = PHASE_OTHER
        base["excluded_reason"] = "wave_number_out_of_range"
    return base


def _pick_actionable(forest: list[dict]) -> dict | None:
    """wave_screen 專屬 picker — 對齊 production r2 揭露的 picker bias 修正。

    r1 的 _pick_primary(track1.py canonical)按 (degree↓, power↓, rules↓) 排,
    偏好「高 degree + rules 多 + 完整結構」的 scenario,結果 picker 永遠選到
    children=[W1..W5] 完整 5 波 → rightmost=W5 → 0 actionable W3 candidate。

    r2 改為 wave_screen 用 actionable picker:
    1. Scan forest 找 Impulse / Diagonal scenarios(對齊 pattern_kind gate)
    2. 優先 incomplete(children 長度 2-4)— 對應正在跑的 impulse,未到 W5
    3. 不完整 scenarios 內按 (degree↓, power↓, rules↓) 排
    4. 若無 incomplete → fallback _pick_primary(會挑到完整 W5,row 仍 emit
       為 observe)

    這對齊 NEoWave forest「展示式並列多假設」精神:wave_screen 想找的不是
    「最像現實」的假設,而是「最 actionable」的假設。
    """
    if not forest:
        return None
    impulse_scenarios = [s for s in forest if _pattern_kind_ok(s)[0]]

    incomplete: list[dict] = []
    for s in impulse_scenarios:
        children = (s.get("wave_tree") or {}).get("children") or []
        if 2 <= len(children) <= 4:
            incomplete.append(s)

    if incomplete:
        incomplete.sort(key=lambda s: (
            _DEGREE_RANK.get(_effective_degree(s) or "", 0),
            _power_rating_strength(s.get("power_rating")),
            int(s.get("rules_passed_count") or 0),
        ), reverse=True)
        return incomplete[0]

    # Fallback:無 incomplete → 用 canonical picker(會挑 complete W5,emit observe)
    return _pick_primary(forest)


def _pattern_kind_ok(scenario: dict) -> tuple[bool, str | None]:
    """pattern_type gate:Impulse 系(Impulse / Diagonal)通過。

    對齊 plan §3 Step 5:Diagonal Leading/Ending 同屬 NEoWave impulse 系
    (compacted_base_label==Five 為證據)。
    """
    label = _pattern_type_label(scenario.get("pattern_type"))
    if label in ("Impulse", "Diagonal"):
        return True, label
    return False, label


def _extract_target_price(scenario: dict) -> float | None:
    """從 expected_fib_zones 抽 W3/W5 target zone midpoint。

    對齊 plan §4:`source_ratio ∈ [1.382, 2.618]` heuristic;無 → None。
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
    # 取最近(最小)的 target zone midpoint(對齊 NEoWave「最近的 fib 投影最可能命中」)
    return min(candidates)


def _extract_below_invalidation(scenario: dict) -> float | None:
    """bullish scenario 取「below」kind 最高 threshold(最緊密的 stop)。"""
    thresholds = _extract_all_invalidation_thresholds(scenario)
    below = [v for k, v in thresholds if k == "below"]
    return max(below) if below else None


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
) -> dict[str, Any]:
    """組裝 single row。若 snapshot=None / forest 空 / excluded → emit 空 row。"""
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
    # r2:wave_screen 專屬 actionable picker — 優先 incomplete Impulse/Diagonal
    primary = _pick_actionable(forest)
    if primary is None:
        extras["confidence_level"] = "loose"
        return empty_row(stock_id, target_date,
                         excluded_reason="empty_forest", extras=extras)

    # pattern_type gate(Impulse / Diagonal)
    pt_ok, pt_label = _pattern_kind_ok(primary)
    extras["pattern_kind"] = pt_label
    if not pt_ok:
        extras["confidence_level"] = "loose"
        return empty_row(stock_id, target_date,
                         excluded_reason="non_impulse", extras=extras)

    # 浪位判定(雙軸驗證)
    pos = current_wave_position(primary)
    extras["phase"]            = pos["phase"]
    extras["wave_number"]      = pos["wave_number"]
    extras["confidence_level"] = pos["confidence_level"]
    extras["structure_label"]  = pos["axis_b_label"]

    # direction + degree
    direction = _direction_from_power(primary.get("power_rating"))
    extras["direction"] = direction
    extras["effective_degree"] = _effective_degree(primary)

    # detail JSONB
    extras["detail"] = {
        "pattern_type_full": primary.get("pattern_type"),
        "power_rating": _power_rating_label(primary.get("power_rating")),
        "power_strength": _power_rating_strength(primary.get("power_rating")),
        "scenario_count": len(forest),
        "snapshot_date": str(snapshot.get("date") or ""),
    }

    is_candidate = bool(pos["is_candidate"])
    excluded: str | None = pos["excluded_reason"]

    # Direction gate:只 bullish 入 candidate(避免方向混亂)
    if is_candidate and direction != "bullish":
        is_candidate = False
        excluded = "non_bullish_direction"

    # R/R 計算(only candidate phases)
    if is_candidate and current_price is not None:
        invalidation = _extract_below_invalidation(primary)
        target = _extract_target_price(primary)
        extras["entry_price"] = float(current_price)
        extras["invalidation_price"] = invalidation
        extras["target_price"] = target
        if target is not None and invalidation is not None and current_price > invalidation:
            rr = (target - current_price) / (current_price - invalidation)
            extras["rr_ratio"] = round(rr, 4) if rr > 0 else None
            if extras["rr_ratio"] is None or extras["rr_ratio"] < RR_MIN:
                is_candidate = False
                excluded = "rr_below_threshold"
        elif target is None:
            is_candidate = False
            excluded = "no_target"
        else:
            is_candidate = False
            excluded = "invalid_rr_geometry"
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
            row_excluded = excluded if excluded is not None else None
            row = _build_row(
                stock_id=sid, target_date=target_date, timeframe=tf,
                snapshot=snap, current_price=current_price,
                excluded_reason=row_excluded,
            )
            rows.append(row)

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
