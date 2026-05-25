"""CQR / ACI calibration layer.

對齊 user v0.3 區間預測 spec phase 4 + plan 文件 phase 4。

Wraps a raw forecast core's output(e.g. kalman_forecast_core, calibrated=False)
into intervals with empirical coverage guarantees.  Writes new rows under a
calibrated source_core(e.g. 'kalman_cqr')with calibrated=True.

Theory references(spec §「理論依據」):
  - Romano, Patterson & Candès (2019) NeurIPS 32  — Conformalized Quantile Regression
  - Gibbs & Candès (2021) NeurIPS 34              — Adaptive Conformal Inference
  - Vovk, Gammerman & Shafer (2005)               — split-conformal framework

CQR(Conformalized Quantile Regression):
  Given a settled calibration set {(L_i, U_i, y_i)}_{i=1..n} from the raw core:
    nonconformity:  e_i = max(L_i - y_i, y_i - U_i)
  Take q = ceil((n+1)(1-α))/n quantile of e_i  (finite-sample correction).
  Calibrated interval for current forecast (L, U):  L' = L - q,  U' = U + q.

ACI(Adaptive Conformal Inference):
  Online α update each step:  α_{t+1} = α_t + γ·(target_coverage - hit_t)
  Pulls α towards true coverage; γ ≈ 0.05 typical.
"""

from __future__ import annotations

import math
from collections import defaultdict
from datetime import date, timedelta
from typing import Any

from forecast._db import upsert_forecast


__all__ = ["nonconformity_score", "cqr_quantile", "conformalize_one", "conformalize_batch"]


# ─── Core CQR math ───────────────────────────────────────────────────────────


def nonconformity_score(realized: float, lower: float, upper: float) -> float:
    """e = max(L - y, y - U).

    Positive when realized is outside [L, U]; negative when inside.
    For two-sided CQR this is the standard split-conformal score.
    """
    return max(lower - realized, realized - upper)


def cqr_quantile(scores: list[float], confidence: float) -> float:
    """Compute the (1-α) quantile of nonconformity scores with finite-sample
    correction(Romano 2019 eq. 2).

    Args:
        scores: list of nonconformity_score values from calibration set
        confidence: target coverage(e.g. 0.80)

    Returns:
        q: a non-negative scalar.  L' = L - q, U' = U + q.

    Empty input returns +inf(caller should treat as "not enough data").
    """
    n = len(scores)
    if n == 0:
        return math.inf
    alpha = 1.0 - confidence
    # Finite-sample-corrected quantile level
    k = math.ceil((n + 1) * (1.0 - alpha))
    if k > n:
        # Not enough samples — Romano 2019 says return +inf(no guarantee)
        return math.inf
    sorted_scores = sorted(scores)
    # k is 1-indexed in the formula; convert to 0-indexed
    return float(sorted_scores[k - 1])


# ─── DB lookups ──────────────────────────────────────────────────────────────


def _fetch_raw_forecast(
    conn,
    stock_id: str,
    forecast_date: date,
    horizon_days: int,
    confidence: float,
    source_core: str,
) -> dict | None:
    sql = """
        SELECT lower, upper, point, confidence, params_hash
          FROM forecast_log
         WHERE stock_id     = %s
           AND forecast_date= %s
           AND horizon_days = %s
           AND source_core  = %s
           AND ABS(confidence - %s) < 1e-6
           AND internal_only = FALSE
         LIMIT 1
    """
    with conn.cursor() as cur:
        cur.execute(sql, (stock_id, forecast_date, horizon_days, source_core, confidence))
        rows = cur.fetchall()
    return rows[0] if rows else None


def _fetch_calibration_set(
    conn,
    stock_id: str,
    asof: date,
    horizon_days: int,
    confidence: float,
    source_core: str,
    window: int,
) -> list[dict]:
    """Pull most-recent `window` settled rows with forecast_date < asof.

    Filtered on (stock, horizon, confidence, source_core).
    對齊 dual_track_resonance §七:預設過濾 internal_only=FALSE(防 neely_fib
    對齊影子混入 CQR 校準輸入)。
    """
    sql = """
        SELECT lower, upper, realized_price, forecast_date
          FROM forecast_log
         WHERE stock_id      = %s
           AND horizon_days  = %s
           AND source_core   = %s
           AND ABS(confidence - %s) < 1e-6
           AND resolved_date IS NOT NULL
           AND realized_price IS NOT NULL
           AND forecast_date < %s
           AND internal_only = FALSE
         ORDER BY forecast_date DESC
         LIMIT %s
    """
    with conn.cursor() as cur:
        cur.execute(sql, (stock_id, horizon_days, source_core, confidence, asof, window))
        return list(cur.fetchall())


# ─── CQR public API ──────────────────────────────────────────────────────────


def conformalize_one(
    conn,
    *,
    raw_core: str = "kalman_forecast_core",
    target_core: str = "kalman_cqr",
    stock_id: str,
    asof: date,
    horizon_days: int,
    confidence: float,
    calibration_window: int = 500,
    min_calibration_size: int = 30,
) -> dict[str, Any]:
    """Calibrate one (stock, asof, horizon, confidence) tuple via CQR.

    Returns a status dict:
        {status: 'written'|'no_raw'|'insufficient_calibration'|'noninf_quantile',
         q: float | None, n: int}
    """
    raw = _fetch_raw_forecast(
        conn, stock_id, asof, horizon_days, confidence, raw_core
    )
    if raw is None:
        return {"status": "no_raw", "q": None, "n": 0}

    cal = _fetch_calibration_set(
        conn, stock_id, asof, horizon_days, confidence, raw_core,
        calibration_window,
    )
    n = len(cal)
    if n < min_calibration_size:
        return {"status": "insufficient_calibration", "q": None, "n": n}

    scores = [
        nonconformity_score(
            realized=float(r["realized_price"]),
            lower=float(r["lower"]),
            upper=float(r["upper"]),
        )
        for r in cal
    ]
    q = cqr_quantile(scores, confidence)
    if not math.isfinite(q):
        return {"status": "noninf_quantile", "q": None, "n": n}

    raw_lower = float(raw["lower"]) if raw.get("lower") is not None else None
    raw_upper = float(raw["upper"]) if raw.get("upper") is not None else None
    raw_point = float(raw["point"]) if raw.get("point") is not None else None
    if raw_lower is None or raw_upper is None:
        return {"status": "no_raw_bounds", "q": None, "n": n}

    cal_lower = raw_lower - q
    cal_upper = raw_upper + q

    upsert_forecast(
        conn,
        {
            "stock_id": stock_id,
            "forecast_date": asof,
            "horizon_days": horizon_days,
            "lower": round(cal_lower, 4),
            "upper": round(cal_upper, 4),
            "point": round(raw_point, 4) if raw_point is not None else None,
            "confidence": confidence,
            "calibrated": True,
            "source_core": target_core,
            "regime_tag": None,
            "params_hash": (raw.get("params_hash") or "") + f"|cqr_n={n}",
        },
    )
    return {"status": "written", "q": q, "n": n}


def conformalize_batch(
    conn,
    *,
    raw_core: str = "kalman_forecast_core",
    target_core: str = "kalman_cqr",
    stock_ids: list[str],
    start: date,
    end: date,
    horizons: list[int] | None = None,
    confidences: list[float] | None = None,
    calibration_window: int = 500,
    min_calibration_size: int = 30,
) -> dict[str, int]:
    """For each (stock × trading day T × horizon × confidence) in range, run
    conformalize_one.  Returns summary counts.

    Note: this iterates over CALENDAR days, not trading days.  For dense
    backfill use the trading_date_ref-aware wrapper in CLI.
    """
    horizons = horizons or [21, 63, 126]
    confidences = confidences or [0.50, 0.80, 0.95]

    totals: dict[str, int] = defaultdict(int)
    cur_d = start
    while cur_d <= end:
        for sid in stock_ids:
            for h in horizons:
                for c in confidences:
                    res = conformalize_one(
                        conn,
                        raw_core=raw_core,
                        target_core=target_core,
                        stock_id=sid,
                        asof=cur_d,
                        horizon_days=h,
                        confidence=c,
                        calibration_window=calibration_window,
                        min_calibration_size=min_calibration_size,
                    )
                    totals[res["status"]] += 1
        cur_d += timedelta(days=1)
    return dict(totals)
