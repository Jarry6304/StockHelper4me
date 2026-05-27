"""wave_impulse_screen 2A calibration harness — hygiene metrics aggregation.

對齊 b1 plan §「下班後 verify 流水線」§Track 2A:對 5 個 best-guess threshold
跑 hygiene calibration(count / RR 分布 / phase 比例 / excluded_reason histogram /
cross_tf_aligned 比例)。

**Phase 1 hygiene only**(本 module):
- 不需 forward outcome
- 只需 structural_snapshots 歷史深度(Path A)
- 對(date × threshold-combo)展開 → 跑 read-only `compute_screen_at_date` → 聚合
  → 一 row 一個 sample

**Phase 2 predictive**(留 future,要求 forward outcome 或 PIT backtest):
- 命中率 (hit rate)
- 到 target 平均時間
- 最大 drawdown

CLI:`python src/main.py wave-impulse-calibrate ...`
"""

from __future__ import annotations

import json
import logging
from dataclasses import asdict, replace
from datetime import date, timedelta
from itertools import product
from typing import Any, Iterable

from cross_cores.wave_impulse_screen import (
    DEFAULT_THRESHOLDS,
    PHASE_CORRECTION_DONE_DOWN,
    PHASE_CORRECTION_DONE_UP,
    PHASE_CORRECTION_ONGOING,
    PHASE_IMPULSE_COMPLETE,
    PHASE_OTHER,
    ScreenThresholds,
    compute_screen_at_date,
)


__all__ = [
    "build_threshold_combos",
    "build_date_series",
    "aggregate_hygiene_metrics",
    "calibrate_hygiene",
    "RR_OUTLIER_THRESHOLD",
]


logger = logging.getLogger("collector.cross_cores.wave_impulse_calibrate")


# RR > 此值視為 outlier(對齊 CLAUDE.md v4.26 r7 揭露 razor-thin stops
# 創造 RR > 20 cluster;hygiene check 看這數量 / 比率)
RR_OUTLIER_THRESHOLD: float = 20.0


# 6 個 threshold 的「常用 sweep 集合」— CLI default(若 user 沒 explicit sweep)
# 對應 backlog plan §2A + v4.28 RR_MAX_CAP
DEFAULT_SWEEP_RANGES: dict[str, list[float]] = {
    "recent_days": [7, 14, 21],
    "rr_min": [1.0, 1.5, 2.0, 2.5],
    "rr_max_cap": [10.0, 15.0, 20.0, 30.0],
    "max_upside_multiple": [1.5, 2.0, 3.0],
    "correction_bottom_buffer": [0.02, 0.03, 0.05],
    "min_upside_pct": [0.02, 0.03, 0.05],
}


# ────────────────────────────────────────────────────────────
# Sweep combinator
# ────────────────────────────────────────────────────────────


def build_threshold_combos(
    overrides: dict[str, list[float]] | None = None,
) -> list[ScreenThresholds]:
    """產 ScreenThresholds list — 對指定 axis 做 cartesian,未指定走 DEFAULT 單值。

    Examples:
        build_threshold_combos({})  # → [DEFAULT_THRESHOLDS](1 combo)
        build_threshold_combos({"rr_min": [1.0, 1.5, 2.0]})  # → 3 combos
        build_threshold_combos({"rr_min": [1.0, 2.0], "recent_days": [7, 14]})
            # → 2 × 2 = 4 combos cartesian

    Args:
        overrides: {axis_name: [val1, val2, ...]} for sweep。
                   未列 axis 用 DEFAULT_THRESHOLDS 對應值。
                   None / empty dict → 1 combo (DEFAULT)。

    Returns:
        list[ScreenThresholds],每 combo 都是 frozen dataclass。
    """
    overrides = overrides or {}
    if not overrides:
        return [DEFAULT_THRESHOLDS]

    # Per-axis 用 override(若提供) or DEFAULT single value
    axes = ("recent_days", "rr_min", "rr_max_cap", "max_upside_multiple",
            "correction_bottom_buffer", "min_upside_pct")
    axis_values: list[list[Any]] = []
    for axis in axes:
        if axis in overrides:
            axis_values.append(list(overrides[axis]))
        else:
            axis_values.append([getattr(DEFAULT_THRESHOLDS, axis)])

    combos: list[ScreenThresholds] = []
    for combo in product(*axis_values):
        rd, rrm, rmc, mum, cbb, mup = combo
        combos.append(ScreenThresholds(
            recent_days=int(rd),
            rr_min=float(rrm),
            rr_max_cap=float(rmc),
            max_upside_multiple=float(mum),
            correction_bottom_buffer=float(cbb),
            min_upside_pct=float(mup),
        ))
    return combos


def build_date_series(
    start: date, end: date, *, step_days: int = 7,
) -> list[date]:
    """簡單日序產生器 — 從 start 起每 step_days 一個 sample 直到 end(含)。

    Note: 沒 trading-calendar 對齊;若 sample 落非交易日,Path A 仍可 query
    (snapshot_date <= asof 拿最近的 trading-day snapshot)。
    """
    if start > end:
        return []
    if step_days <= 0:
        raise ValueError(f"step_days must be > 0, got {step_days}")
    out: list[date] = []
    cur = start
    while cur <= end:
        out.append(cur)
        cur = cur + timedelta(days=step_days)
    return out


# ────────────────────────────────────────────────────────────
# Hygiene metric aggregator
# ────────────────────────────────────────────────────────────


def _percentile(values: list[float], pct: float) -> float | None:
    """Inclusive linear-interpolation percentile(無 numpy 依賴 — 避免 forecast/
    macro_forecast 等 numpy-import 重)。pct ∈ [0, 100]。"""
    if not values:
        return None
    sorted_v = sorted(values)
    n = len(sorted_v)
    if n == 1:
        return sorted_v[0]
    # Linear interpolation: idx = (n-1) * pct / 100
    idx = (n - 1) * (pct / 100.0)
    lo = int(idx)
    hi = min(lo + 1, n - 1)
    frac = idx - lo
    return sorted_v[lo] * (1 - frac) + sorted_v[hi] * frac


def aggregate_hygiene_metrics(
    rows: list[dict[str, Any]],
    *,
    as_of: date,
    thresholds: ScreenThresholds,
    rr_outlier_threshold: float = RR_OUTLIER_THRESHOLD,
) -> dict[str, Any]:
    """從 compute_screen_at_date() 出的 rows 聚合 hygiene metrics。

    Returns:
        {
            "as_of": str,
            "thresholds": {recent_days, rr_min, ...},
            "total_rows": int,
            "candidates": int,
            "candidates_top_n": int,
            "candidates_cross_tf_aligned": int,
            # Phase breakdown(所有 row,含 excluded)
            "phase_correction_down": int,
            "phase_correction_up": int,
            "phase_correction_ongoing": int,
            "phase_impulse_complete": int,
            "phase_other": int,
            "phase_none": int,  # 沒進浪位判定 / excluded too early
            # RR distribution(僅 candidate)
            "rr_count": int,
            "rr_p50": float | None,
            "rr_p95": float | None,
            "rr_max": float | None,
            "rr_outlier_count": int,    # rr > rr_outlier_threshold
            # Top 5 excluded reasons by count
            "excluded_top": list[{"reason": str, "count": int}],
        }
    """
    total_rows = len(rows)
    candidates = [r for r in rows if r.get("is_candidate") is True]

    # Phase breakdown(全 rows 含 excluded)
    phase_counts = {
        PHASE_CORRECTION_DONE_DOWN: 0,
        PHASE_CORRECTION_DONE_UP: 0,
        PHASE_CORRECTION_ONGOING: 0,
        PHASE_IMPULSE_COMPLETE: 0,
        PHASE_OTHER: 0,
    }
    phase_none = 0
    for r in rows:
        phase = r.get("phase")
        if phase is None:
            phase_none += 1
        elif phase in phase_counts:
            phase_counts[phase] += 1
        else:
            # Unknown phase value(future-proof)
            phase_none += 1

    # RR distribution(only is_candidate=True)
    rr_values: list[float] = []
    for r in candidates:
        rr = r.get("rr_ratio")
        if isinstance(rr, (int, float)) and not isinstance(rr, bool):
            rr_values.append(float(rr))

    rr_count = len(rr_values)
    rr_outlier_count = sum(1 for rr in rr_values if rr > rr_outlier_threshold)
    rr_p50 = _percentile(rr_values, 50.0)
    rr_p95 = _percentile(rr_values, 95.0)
    rr_max = max(rr_values) if rr_values else None

    # Excluded reasons histogram (全 rows)
    excluded_hist: dict[str, int] = {}
    for r in rows:
        reason = r.get("excluded_reason")
        if reason:
            excluded_hist[reason] = excluded_hist.get(reason, 0) + 1
    excluded_top = sorted(
        ({"reason": k, "count": v} for k, v in excluded_hist.items()),
        key=lambda x: x["count"],
        reverse=True,
    )[:5]

    return {
        "as_of": as_of.isoformat() if isinstance(as_of, date) else str(as_of),
        "thresholds": asdict(thresholds),
        "total_rows": total_rows,
        "candidates": len(candidates),
        "candidates_top_n": sum(1 for r in candidates if r.get("is_top_n") is True),
        "candidates_cross_tf_aligned": sum(
            1 for r in candidates if r.get("cross_tf_aligned") is True
        ),
        # Phase
        "phase_correction_down": phase_counts[PHASE_CORRECTION_DONE_DOWN],
        "phase_correction_up": phase_counts[PHASE_CORRECTION_DONE_UP],
        "phase_correction_ongoing": phase_counts[PHASE_CORRECTION_ONGOING],
        "phase_impulse_complete": phase_counts[PHASE_IMPULSE_COMPLETE],
        "phase_other": phase_counts[PHASE_OTHER],
        "phase_none": phase_none,
        # RR
        "rr_count": rr_count,
        "rr_p50": rr_p50,
        "rr_p95": rr_p95,
        "rr_max": rr_max,
        "rr_outlier_count": rr_outlier_count,
        # Excluded
        "excluded_top": excluded_top,
    }


# ────────────────────────────────────────────────────────────
# Calibration entrypoint
# ────────────────────────────────────────────────────────────


def calibrate_hygiene(
    db: Any,
    *,
    asof_dates: Iterable[date],
    threshold_combos: Iterable[ScreenThresholds] | None = None,
    stock_ids: list[str] | None = None,
    market: str = "TW",
) -> list[dict[str, Any]]:
    """跑 (date × combo) 矩陣;每 cell 算 hygiene metrics → 一筆 sample row。

    對齊 b1 plan §2A 「hygiene calibration」:不需 forward outcome,只需
    structural_snapshots 歷史深度。

    Args:
        db: PG conn(走 fusion.raw._db.get_connection)
        asof_dates: 歷史 asof T 序列(走 build_date_series 產)
        threshold_combos: ScreenThresholds list(走 build_threshold_combos 產);
                          None → 走 DEFAULT_THRESHOLDS 單 combo
        stock_ids: 限縮股票集(None → 全 universe)
        market: TW only

    Returns:
        list of hygiene-metric dicts,每 dict 對應一個 (date, combo) sample。
        Caller 可寫 CSV / JSON 出去做分析。

    Notes on Path A vs Path B:
        本實作走 Path A(讀 structural_snapshots WHERE snapshot_date ≤ asof)。
        如果 production 沒累積足夠歷史 snapshot,大量 asof 會回 0 rows;
        caller 應先跑 count query 確認 snapshot depth(CLAUDE.md backlog §2A):
            SELECT count(DISTINCT snapshot_date), min(snapshot_date), max(snapshot_date)
              FROM structural_snapshots WHERE core_name='neely_core';
    """
    if threshold_combos is None:
        threshold_combos = [DEFAULT_THRESHOLDS]
    threshold_combos = list(threshold_combos)

    asof_list = list(asof_dates)
    if not asof_list:
        return []

    total_cells = len(asof_list) * len(threshold_combos)
    logger.info(
        f"[calibrate_hygiene] starting {len(asof_list)} dates × "
        f"{len(threshold_combos)} threshold combos = {total_cells} cells"
    )

    samples: list[dict[str, Any]] = []
    for asof in asof_list:
        for thresholds in threshold_combos:
            rows, target_used = compute_screen_at_date(
                db,
                target_date=asof,
                stock_ids=stock_ids,
                thresholds=thresholds,
                market=market,
            )
            if not rows:
                # Path A 對此 asof 找不到任何 structural_snapshot — 記 0 sample
                logger.warning(
                    f"[calibrate_hygiene] asof={asof}: 0 rows "
                    f"(structural_snapshots depth 不足?)"
                )
                samples.append({
                    "as_of": asof.isoformat(),
                    "thresholds": asdict(thresholds),
                    "total_rows": 0, "candidates": 0,
                    "candidates_top_n": 0, "candidates_cross_tf_aligned": 0,
                    "phase_correction_down": 0, "phase_correction_up": 0,
                    "phase_correction_ongoing": 0, "phase_impulse_complete": 0,
                    "phase_other": 0, "phase_none": 0,
                    "rr_count": 0, "rr_p50": None, "rr_p95": None, "rr_max": None,
                    "rr_outlier_count": 0,
                    "excluded_top": [],
                    "skipped_reason": "no_snapshots_at_or_before_asof",
                })
                continue
            sample = aggregate_hygiene_metrics(
                rows, as_of=asof, thresholds=thresholds,
            )
            samples.append(sample)

    logger.info(f"[calibrate_hygiene] completed {len(samples)} samples")
    return samples


# ────────────────────────────────────────────────────────────
# CSV / JSON output formatting
# ────────────────────────────────────────────────────────────


def samples_to_csv_rows(samples: list[dict[str, Any]]) -> list[dict[str, Any]]:
    """Flatten samples 為 CSV-friendly dict(thresholds 展平 + excluded_top JSON-string)。

    既有 sample 內含 nested dict(thresholds / excluded_top),CSV writer 不友善;
    本函式展平給 csv.DictWriter 用。
    """
    out: list[dict[str, Any]] = []
    for s in samples:
        flat = dict(s)
        # 展平 thresholds 為 thr_xxx 欄
        thresholds = flat.pop("thresholds", {})
        for k, v in thresholds.items():
            flat[f"thr_{k}"] = v
        # excluded_top → JSON string(避免 list of dict 在 CSV 內亂)
        if "excluded_top" in flat:
            flat["excluded_top_json"] = json.dumps(
                flat.pop("excluded_top"), ensure_ascii=False,
            )
        out.append(flat)
    return out
