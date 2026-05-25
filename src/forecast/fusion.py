"""Zero-parameter forecast fusion.

對齊 user v0.3 區間預測 spec phase 7 + plan 文件 phase 7。

純規則式組合(無 learned weight)— spec rule:
    intersection 為主:lower_f = max(lower_i), upper_f = min(upper_i)
    若 intersection 為空(upper_f < lower_f):
        centroid    = mean(midpoints_i)
        half_width  = max(half_widths_i) + std(midpoints_i)
        lower_f / upper_f = centroid ± half_width

Eligibility gate(spec §「強制規則」):
    任何 source_core 進 fusion 須 (1) calibrated=True 且 (2) 過去 N 筆已結算
    pinball 平均 < baseline pinball 平均(在相同 horizon × confidence)。

Excluded source_cores(全是 calibrated=False — 強制規則:未校準者不得宣稱
覆蓋率,fusion 不取):
    baseline / log_channel / kalman_forecast_core / neely_fib / manual / fib

通常 eligible 名單:kalman_cqr(M4 校準)+ 後續其他 conformalized cores。

實證限制(2026-05-23 production verify,6 stocks × 1549 days × 3h × 3conf):
    Fusion **strictly dominated by kalman_cqr** on pinball / sharpness /
    reliability 三項 across 21/63/126 天 horizons。Root cause:現有 3 個 cores
    (baseline / kalman_cqr / log_channel_cqr)全部基於 daily close price →
    誤差高度相關 → 違反 Bates-Granger 1969 forecast combination puzzle 前提
    (multi forecaster 等權平均勝單一者的前提是「誤差 uncorrelated」)。
    Intersection 路徑實際上等同 kalman_cqr 的 pass-through + selection bias
    (子集偏難 — 只在 kalman_cqr 過去勝 baseline 才寫)。

    保留 fusion 是因為(1) spec compliance(2) 理論上正確(3) 未來性 — 加入
    非 price 信號源 cores 後 fusion 將真正展現變異數縮減。

Future work(M8+,獨立 sprint):
    加非 price-based forecast cores 讓誤差 uncorrelated:
      - chip_forecast_core      ← institutional flow + margin / loan_collateral
      - macro_forecast_core     ← FX / commodity / business_indicator
      - fundamental_forecast_core ← revenue YoY + financial_statement
    每個 follow 既有 5 接點 pattern(寫 forecast_log calibrated=False →
    conformalize 加 X / X_cqr → 進 eligible_cores 自動 picked up)。完整紀錄
    見 CLAUDE.md §v4.23。
"""

from __future__ import annotations

import math
from datetime import date
from typing import Any

from forecast._db import upsert_forecast


__all__ = ["eligible_cores", "fuse_one", "fuse_batch"]


_BASELINE_CORE = "baseline"
_FUSION_CORE = "fusion"
_DEFAULT_ELIGIBILITY_WINDOW = 100  # last N settled forecasts to compare


# ─── Eligibility gate ────────────────────────────────────────────────────────


def _mean_pinball(
    conn,
    *,
    source_core: str,
    horizon_days: int,
    confidence: float,
    asof: date,
    window: int,
    calibrated_only: bool = True,
) -> tuple[float | None, int]:
    """Mean pinball loss for the last `window` settled rows of source_core.

    Returns (mean, n).  None if no rows.
    """
    sql = """
        SELECT AVG(pinball_loss) AS mean_pl, COUNT(*) AS n
          FROM (
            SELECT pinball_loss
              FROM forecast_log
             WHERE source_core   = %s
               AND horizon_days  = %s
               AND ABS(confidence - %s) < 1e-6
               AND resolved_date IS NOT NULL
               AND pinball_loss  IS NOT NULL
               AND forecast_date < %s
               AND internal_only = FALSE
    """
    params: list[Any] = [source_core, horizon_days, confidence, asof]
    if calibrated_only:
        # baseline is excluded from calibrated_only=True intentionally —
        # baseline is the reference, not a fusion candidate
        sql += "       AND calibrated = TRUE\n"
    sql += """
             ORDER BY forecast_date DESC
             LIMIT %s
          ) recent
    """
    params.append(window)
    with conn.cursor() as cur:
        cur.execute(sql, params)
        rows = cur.fetchall()
    if not rows:
        return None, 0
    row = rows[0]
    mean = row.get("mean_pl")
    n = int(row.get("n") or 0)
    return (float(mean) if mean is not None else None, n)


def eligible_cores(
    conn,
    *,
    asof: date,
    horizon_days: int,
    confidence: float,
    window: int = _DEFAULT_ELIGIBILITY_WINDOW,
    min_samples: int = 30,
) -> list[str]:
    """Return calibrated source_cores whose recent mean pinball beats baseline.

    Excludes 'baseline' itself (it's the reference) and 'fusion' (no recursion).
    """
    # Baseline reference
    baseline_pinball, baseline_n = _mean_pinball(
        conn,
        source_core=_BASELINE_CORE,
        horizon_days=horizon_days,
        confidence=confidence,
        asof=asof,
        window=window,
        calibrated_only=False,  # baseline is the comparison anchor
    )
    if baseline_pinball is None or baseline_n < min_samples:
        # No baseline reference yet — can't gate, return empty
        return []

    # Find all candidate source_cores (calibrated=True) with recent settled rows
    sql = """
        SELECT DISTINCT source_core
          FROM forecast_log
         WHERE calibrated = TRUE
           AND resolved_date IS NOT NULL
           AND horizon_days   = %s
           AND ABS(confidence - %s) < 1e-6
           AND forecast_date  < %s
           AND source_core    NOT IN (%s, %s)
           AND internal_only  = FALSE
    """
    with conn.cursor() as cur:
        cur.execute(
            sql,
            (horizon_days, confidence, asof, _FUSION_CORE, _BASELINE_CORE),
        )
        cores = [r["source_core"] for r in cur.fetchall()]

    out: list[str] = []
    for core in cores:
        mean_pl, n = _mean_pinball(
            conn,
            source_core=core,
            horizon_days=horizon_days,
            confidence=confidence,
            asof=asof,
            window=window,
            calibrated_only=True,
        )
        if mean_pl is None or n < min_samples:
            continue
        if mean_pl < baseline_pinball:
            out.append(core)
    return out


# ─── Fusion math ─────────────────────────────────────────────────────────────


def _fuse_intervals(
    intervals: list[tuple[float, float]],
) -> tuple[float, float, float]:
    """Pure function: zero-param fusion of N intervals.

    intersection:  L = max(L_i), U = min(U_i)
    if empty (U < L):
        centroid   = mean(midpoints)
        half_width = max(half_widths_i) + std(midpoints)
        L, U       = centroid ∓ half_width

    Returns (lower, upper, point).
    """
    if not intervals:
        raise ValueError("_fuse_intervals: empty intervals")

    lowers = [iv[0] for iv in intervals]
    uppers = [iv[1] for iv in intervals]
    midpoints = [(iv[0] + iv[1]) / 2.0 for iv in intervals]
    half_widths = [(iv[1] - iv[0]) / 2.0 for iv in intervals]

    intersection_lower = max(lowers)
    intersection_upper = min(uppers)

    if intersection_upper >= intersection_lower:
        # Valid intersection — use it
        lower = intersection_lower
        upper = intersection_upper
        point = (lower + upper) / 2.0
    else:
        # Empty intersection — divergence-driven fallback
        centroid = sum(midpoints) / len(midpoints)
        max_hw = max(half_widths)
        if len(midpoints) > 1:
            mean_m = centroid
            var_m = sum((m - mean_m) ** 2 for m in midpoints) / (len(midpoints) - 1)
            std_m = math.sqrt(var_m)
        else:
            std_m = 0.0
        half_w = max_hw + std_m
        lower = centroid - half_w
        upper = centroid + half_w
        point = centroid

    return lower, upper, point


# ─── DB lookups ──────────────────────────────────────────────────────────────


def _fetch_eligible_forecasts(
    conn,
    *,
    stock_id: str,
    forecast_date: date,
    horizon_days: int,
    confidence: float,
    cores: list[str],
) -> list[dict]:
    if not cores:
        return []
    placeholders = ",".join(["%s"] * len(cores))
    sql = f"""
        SELECT source_core, lower, upper, point
          FROM forecast_log
         WHERE stock_id      = %s
           AND forecast_date = %s
           AND horizon_days  = %s
           AND ABS(confidence - %s) < 1e-6
           AND calibrated    = TRUE
           AND internal_only = FALSE
           AND source_core  IN ({placeholders})
    """
    with conn.cursor() as cur:
        cur.execute(sql, [stock_id, forecast_date, horizon_days, confidence, *cores])
        return list(cur.fetchall())


# ─── Public API ──────────────────────────────────────────────────────────────


def fuse_one(
    conn,
    *,
    stock_id: str,
    forecast_date: date,
    horizon_days: int,
    confidence: float,
    eligibility_window: int = _DEFAULT_ELIGIBILITY_WINDOW,
    min_samples: int = 30,
) -> dict[str, Any]:
    """Run zero-param fusion for one (stock, date, horizon, confidence) tuple.

    Returns status dict including the fused interval if written.
    """
    cores = eligible_cores(
        conn,
        asof=forecast_date,
        horizon_days=horizon_days,
        confidence=confidence,
        window=eligibility_window,
        min_samples=min_samples,
    )
    if not cores:
        return {"status": "no_eligible_cores", "eligible_cores": []}

    rows = _fetch_eligible_forecasts(
        conn,
        stock_id=stock_id,
        forecast_date=forecast_date,
        horizon_days=horizon_days,
        confidence=confidence,
        cores=cores,
    )
    intervals: list[tuple[float, float]] = []
    contributing = []
    for r in rows:
        l, u = r.get("lower"), r.get("upper")
        if l is None or u is None:
            continue
        intervals.append((float(l), float(u)))
        contributing.append(r["source_core"])

    if not intervals:
        return {
            "status": "no_calibrated_inputs_for_date",
            "eligible_cores": cores,
        }

    lower, upper, point = _fuse_intervals(intervals)

    upsert_forecast(
        conn,
        {
            "stock_id": stock_id,
            "forecast_date": forecast_date,
            "horizon_days": horizon_days,
            "lower": round(lower, 4),
            "upper": round(upper, 4),
            "point": round(point, 4),
            "confidence": confidence,
            "calibrated": True,  # all inputs are calibrated → fused is too
            "source_core": _FUSION_CORE,
            "regime_tag": None,
            "params_hash": "fusion|" + ",".join(sorted(contributing)),
        },
    )
    return {
        "status": "written",
        "eligible_cores": cores,
        "contributing_cores": contributing,
        "interval": (round(lower, 4), round(upper, 4)),
        "point": round(point, 4),
        "intersection_valid": max(iv[0] for iv in intervals) <= min(iv[1] for iv in intervals),
    }


def fuse_batch(
    conn,
    *,
    stock_ids: list[str],
    forecast_dates: list[date],
    horizons: list[int] | None = None,
    confidences: list[float] | None = None,
    eligibility_window: int = _DEFAULT_ELIGIBILITY_WINDOW,
    min_samples: int = 30,
) -> dict[str, int]:
    """Cartesian fuse over (stock × forecast_date × horizon × confidence)."""
    horizons = horizons or [21, 63, 126]
    confidences = confidences or [0.50, 0.80, 0.95]
    from collections import defaultdict
    totals: dict[str, int] = defaultdict(int)
    for sid in stock_ids:
        for fd in forecast_dates:
            for h in horizons:
                for c in confidences:
                    res = fuse_one(
                        conn,
                        stock_id=sid,
                        forecast_date=fd,
                        horizon_days=h,
                        confidence=c,
                        eligibility_window=eligibility_window,
                        min_samples=min_samples,
                    )
                    totals[res["status"]] += 1
    return dict(totals)
