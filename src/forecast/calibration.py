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
from bisect import bisect_left
from collections import defaultdict
from datetime import date
from typing import Any

from forecast._db import upsert_forecast, upsert_forecast_batch


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


# ─── Pure CQR row builder(single source of truth for conformalize_one + batch)─


def _build_calibrated_row(
    raw: dict | None,
    cal: list[dict],
    *,
    stock_id: str,
    asof: date,
    horizon_days: int,
    confidence: float,
    target_core: str,
    min_calibration_size: int,
) -> tuple[dict[str, Any], dict[str, Any] | None]:
    """Pure CQR computation:given a raw forecast + settled calibration set,
    return (status_dict, calibrated_row | None).

    這是 `conformalize_one` 與 `conformalize_batch` **唯一共用**的 CQR 數學;兩條路徑
    位元一致即由此保證(批次版只改「資料怎麼進出 DB」,不碰這裡的算式)。
    """
    if raw is None:
        return {"status": "no_raw", "q": None, "n": 0}, None

    n = len(cal)
    if n < min_calibration_size:
        return {"status": "insufficient_calibration", "q": None, "n": n}, None

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
        return {"status": "noninf_quantile", "q": None, "n": n}, None

    raw_lower = float(raw["lower"]) if raw.get("lower") is not None else None
    raw_upper = float(raw["upper"]) if raw.get("upper") is not None else None
    raw_point = float(raw["point"]) if raw.get("point") is not None else None
    if raw_lower is None or raw_upper is None:
        return {"status": "no_raw_bounds", "q": None, "n": n}, None

    cal_lower = raw_lower - q
    cal_upper = raw_upper + q
    row = {
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
    }
    return {"status": "written", "q": q, "n": n}, row


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


# ─── 批次預載 lookups(conformalize_batch 用,取代逐筆 N+1)─────────────────────


def _fetch_trading_days(conn, start: date, end: date, market: str = "TW") -> list[date]:
    """[start, end] 的交易日(升序)。對齊 settlement / Rust backtest 既有
    `SELECT date FROM trading_date_ref WHERE market=... AND date BETWEEN ...`。"""
    sql = """
        SELECT date FROM trading_date_ref
         WHERE market = %s AND date BETWEEN %s AND %s
         ORDER BY date ASC
    """
    with conn.cursor() as cur:
        cur.execute(sql, (market, start, end))
        return [r["date"] for r in cur.fetchall()]


def _fetch_raw_forecasts_range(
    conn,
    stock_id: str,
    start: date,
    end: date,
    horizon_days: int,
    confidence: float,
    source_core: str,
) -> dict[date, dict]:
    """一次撈 [start, end] 全部 raw forecast,key by forecast_date。

    取代 per-asof 的 `_fetch_raw_forecast`(LIMIT 1 by exact key)。forecast_log 唯一鍵
    `(stock_id, forecast_date, horizon_days, source_core, confidence)` 保證每 asof 至多
    一筆 → dict lookup 與單筆 SELECT 位元等價。
    """
    sql = """
        SELECT forecast_date, lower, upper, point, confidence, params_hash
          FROM forecast_log
         WHERE stock_id     = %s
           AND horizon_days = %s
           AND source_core  = %s
           AND ABS(confidence - %s) < 1e-6
           AND internal_only = FALSE
           AND forecast_date BETWEEN %s AND %s
    """
    with conn.cursor() as cur:
        cur.execute(sql, (stock_id, horizon_days, source_core, confidence, start, end))
        rows = cur.fetchall()
    return {r["forecast_date"]: r for r in rows}


def _fetch_calibration_history(
    conn,
    stock_id: str,
    horizon_days: int,
    confidence: float,
    source_core: str,
    before: date,
) -> list[dict]:
    """一次撈 forecast_date < `before` 的**全部** settled 校準史(同 `_fetch_calibration_set`
    濾條件,但不分頁 / 無 per-asof LIMIT)。caller 在記憶體中對每個 asof 做
    `forecast_date < asof` 切片取最近 window 筆 → 與逐筆 SQL 切點位元等價(forecast_date
    唯一 → 無 tie 歧義;cqr_quantile 對 scores 排序 → 切片 set 相同即 q 相同)。
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
    """
    with conn.cursor() as cur:
        cur.execute(sql, (stock_id, horizon_days, source_core, confidence, before))
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
    status, row = _build_calibrated_row(
        raw, cal,
        stock_id=stock_id, asof=asof, horizon_days=horizon_days,
        confidence=confidence, target_core=target_core,
        min_calibration_size=min_calibration_size,
    )
    if row is not None:
        upsert_forecast(conn, row)
    return status


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
    market: str = "TW",
) -> dict[str, int]:
    """批次 CQR 校準 [start, end] 內每 (stock × 交易日 × horizon × confidence)。

    與逐筆 `conformalize_one` **寫出位元相同的 calibrated rows**(共用
    `_build_calibrated_row` 數學),但 IO 模式改批次:
      - 只迭代**交易日**(`trading_date_ref`),不跑日曆天(raw_core forecast 僅在
        交易日 emit → 不漏任何 written row;砍掉非交易日的空轉)。
      - 每 (stock, h, c) **2 次 SELECT** 預載 raw + 校準史,per-asof 在記憶體切片
        (取代 per-asof N+1)。
      - 每股累積 calibrated rows → `upsert_forecast_batch` + 單一 `conn.transaction()`
        commit(每股 1 次 fsync,取代 per-row autocommit fsync)。

    Returns summary counts(status → count;只計交易日,故 no_raw 數比舊日曆天版小,
    但 written rows 完全一致)。
    """
    horizons = horizons or [21, 63, 126]
    confidences = confidences or [0.50, 0.80, 0.95]

    totals: dict[str, int] = defaultdict(int)
    trading_days = _fetch_trading_days(conn, start, end, market=market)
    if not trading_days:
        return dict(totals)

    for sid in stock_ids:
        pending_rows: list[dict[str, Any]] = []
        for h in horizons:
            for c in confidences:
                raw_by_date = _fetch_raw_forecasts_range(
                    conn, sid, start, end, h, c, raw_core
                )
                hist = _fetch_calibration_history(conn, sid, h, c, raw_core, end)
                hist_sorted = sorted(hist, key=lambda r: r["forecast_date"])
                dates_asc = [r["forecast_date"] for r in hist_sorted]

                for asof in trading_days:
                    raw = raw_by_date.get(asof)
                    if raw is None:
                        # 對齊 conformalize_one:raw None → no_raw(不查校準集)
                        totals["no_raw"] += 1
                        continue
                    # 記憶體切片 = SQL `forecast_date < asof ORDER BY DESC LIMIT window`
                    idx = bisect_left(dates_asc, asof)
                    lo = max(0, idx - calibration_window)
                    cal = hist_sorted[lo:idx]
                    status, row = _build_calibrated_row(
                        raw, cal,
                        stock_id=sid, asof=asof, horizon_days=h,
                        confidence=c, target_core=target_core,
                        min_calibration_size=min_calibration_size,
                    )
                    totals[status["status"]] += 1
                    if row is not None:
                        pending_rows.append(row)

        if pending_rows:
            with conn.transaction():
                upsert_forecast_batch(conn, pending_rows)

    return dict(totals)
