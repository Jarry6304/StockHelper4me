"""fundamental_forecast_core — monthly revenue YoY drift + price volatility band.

M8 sprint(2026-05-24)第一個非 price-only forecast core。對齊 CLAUDE.md v4.23
fusion future work 提案:加入 uncorrelated signal source 讓 fusion 真正能收
Bates-Granger 1969 變異數縮減。

訊號設計
========

訊號來源:`monthly_revenue` Bronze 表(PIT-safe via `pit.fundamental.asof_revenue`),
取最近 4 個月 revenue → 配 12 個月前同月份 → 算 YoY 平均。

漂移映射:
    drift_horizon = clamp(
        yoy_3m_avg * fade_factor * (horizon / 252),
        -DRIFT_CAP, +DRIFT_CAP,
    )

`fade_factor=0.3` 反映 cross-sectional 經驗:revenue YoY 對未來股價報酬有正向
但 noisy 訊號(beta ≈ 0.2-0.4),取保守中位。`DRIFT_CAP=0.20` 防極端 YoY 個股
(e.g. base-effect newly listed)算出失真區間。

變異數:price 殘差 — 取近 60 個交易日 log return std。Fusion 的價值在於
*點預測* uncorrelated;區間寬度跟其他 cores 類似屬於設計接受。

輸出 `calibrated=False`,phase 4 CQR 才校準。對應 alembic
`d0e1f2g3h4i5_m8_forecast_cores_whitelist.py` 加 `fundamental_forecast_core`
到 uncalibrated whitelist。
"""

from __future__ import annotations

import hashlib
import json
import math
from datetime import date
from typing import Any

import numpy as np


# Calibration constants — keep conservative; revenue YoY is a noisy single-factor
# proxy for stock returns. See module docstring §訊號設計.
_DEFAULT_FADE_FACTOR = 0.3
_DEFAULT_DRIFT_CAP = 0.20
_DEFAULT_VOL_LOOKBACK_DAYS = 60
_DEFAULT_YOY_AVG_MONTHS = 3
_MIN_REVENUE_ROWS = 15  # need at least one full year + 3 months of overlap


def _stable_hash(payload: dict[str, Any]) -> str:
    blob = json.dumps(payload, sort_keys=True, default=str).encode()
    return hashlib.sha256(blob).hexdigest()[:16]


def _z_two_sided(confidence: float) -> float:
    """Inverse standard normal — Beasley-Springer-Moro approximation."""
    from math import sqrt, log
    p = 0.5 + confidence / 2.0
    a = [-3.969683028665376e+01, 2.209460984245205e+02,
         -2.759285104469687e+02, 1.383577518672690e+02,
         -3.066479806614716e+01, 2.506628277459239e+00]
    b = [-5.447609879822406e+01, 1.615858368580409e+02,
         -1.556989798598866e+02, 6.680131188771972e+01,
         -1.328068155288572e+01]
    c = [-7.784894002430293e-03, -3.223964580411365e-01,
         -2.400758277161838e+00, -2.549732539343734e+00,
          4.374664141464968e+00,  2.938163982698783e+00]
    d = [ 7.784695709041462e-03,  3.224671290700398e-01,
          2.445134137142996e+00,  3.754408661907416e+00]
    p_low = 0.02425
    p_high = 1 - p_low
    if p < p_low:
        q = sqrt(-2.0 * log(p))
        return (((((c[0]*q+c[1])*q+c[2])*q+c[3])*q+c[4])*q+c[5]) / \
               ((((d[0]*q+d[1])*q+d[2])*q+d[3])*q+1)
    if p <= p_high:
        q = p - 0.5
        r = q * q
        return (((((a[0]*r+a[1])*r+a[2])*r+a[3])*r+a[4])*r+a[5]) * q / \
               (((((b[0]*r+b[1])*r+b[2])*r+b[3])*r+b[4])*r+1)
    q = sqrt(-2.0 * log(1 - p))
    return -(((((c[0]*q+c[1])*q+c[2])*q+c[3])*q+c[4])*q+c[5]) / \
            ((((d[0]*q+d[1])*q+d[2])*q+d[3])*q+1)


def _compute_yoy_3m_avg(revenue_rows: list[dict[str, Any]]) -> float | None:
    """Average YoY across the 3 most recent months that have a 12-month-ago peer.

    rows must have (revenue_year, revenue_month, revenue) keys. Order-agnostic;
    we sort by (year, month) ourselves and pick the last 3 with valid base.
    """
    if not revenue_rows:
        return None
    # Index by (year, month) → revenue
    by_period: dict[tuple[int, int], float] = {}
    for r in revenue_rows:
        y = r.get("revenue_year")
        m = r.get("revenue_month")
        rev = r.get("revenue")
        if y is None or m is None or rev is None:
            continue
        try:
            y_i = int(y)
            m_i = int(m)
            rev_f = float(rev)
        except (TypeError, ValueError):
            continue
        if rev_f <= 0:
            continue
        by_period[(y_i, m_i)] = rev_f
    if not by_period:
        return None

    # Sort periods descending (latest first)
    sorted_periods = sorted(by_period.keys(), reverse=True)

    yoys: list[float] = []
    for (y, m) in sorted_periods:
        base_key = (y - 1, m)
        if base_key not in by_period:
            continue
        cur = by_period[(y, m)]
        base = by_period[base_key]
        yoy = (cur - base) / base  # decimal: 0.20 = +20%
        yoys.append(yoy)
        if len(yoys) >= _DEFAULT_YOY_AVG_MONTHS:
            break

    if not yoys:
        return None
    return float(np.mean(yoys))


def _compute_realized_vol(closes: list[float], lookback: int) -> float | None:
    """Realized daily log-return std over trailing `lookback` bars."""
    arr = np.asarray([c for c in closes if c is not None and c > 0], dtype=float)
    if arr.size < lookback + 1:
        return None
    sample = arr[-(lookback + 1):]
    log_r = np.diff(np.log(sample))
    if log_r.size == 0:
        return None
    sigma = float(np.std(log_r, ddof=1))
    if not np.isfinite(sigma) or sigma <= 0:
        return None
    return sigma


def make_fundamental_forecast(
    series: list[dict[str, Any]],
    forecast_date: date,
    horizon: int,
    confidence: float = 0.80,
    *,
    conn=None,
    stock_id: str | None = None,
    revenue_rows: list[dict[str, Any]] | None = None,
    fade_factor: float = _DEFAULT_FADE_FACTOR,
    drift_cap: float = _DEFAULT_DRIFT_CAP,
    vol_lookback: int = _DEFAULT_VOL_LOOKBACK_DAYS,
    market: str = "TW",
) -> dict[str, Any] | None:
    """Construct a fundamental forecast row.

    Args:
        series: price series (asof_close_series output) — used for spot price +
                realized vol calibration. Must end on or before forecast_date.
        forecast_date: T (as-of).
        horizon: forecast horizon in calendar days (21 / 63 / 126).
        confidence: nominal coverage (0 < c < 1).
        conn: optional PG connection. If provided + revenue_rows is None,
              we fetch via pit.fundamental.asof_revenue.
        stock_id: e.g. "2330" — required when fetching via conn.
        revenue_rows: optional pre-fetched revenue rows (for tests).
        fade_factor: revenue-YoY → expected-return decay; default 0.3.
        drift_cap: cap on |drift|; default ±20%.
        vol_lookback: realized-vol window in trading days; default 60.
        market: passed to PIT fetcher; default "TW".

    Returns:
        Row dict for upsert_forecast, or None if revenue data unavailable / too
        few price bars / vol uncomputable.
    """
    # 1. Fetch revenue rows if not provided
    if revenue_rows is None:
        if conn is None or stock_id is None:
            return None
        try:
            from pit.fundamental import asof_revenue
        except ImportError:
            return None
        revenue_rows = asof_revenue(
            conn, stock_id=stock_id, asof_t=forecast_date, market=market,
        )
    if len(revenue_rows) < _MIN_REVENUE_ROWS:
        return None

    yoy_3m = _compute_yoy_3m_avg(revenue_rows)
    if yoy_3m is None:
        return None

    # 2. Get current price (last asof_adj_close)
    closes = [
        float(r["asof_adj_close"])
        for r in series
        if r.get("asof_adj_close") is not None and float(r["asof_adj_close"]) > 0
    ]
    if not closes:
        return None
    current_price = closes[-1]

    # 3. Realized vol
    sigma_daily = _compute_realized_vol(closes, vol_lookback)
    if sigma_daily is None:
        return None

    # 4. Compute drift
    drift_raw = yoy_3m * fade_factor * (horizon / 252.0)
    drift = max(-drift_cap, min(drift_cap, drift_raw))

    # 5. Build interval
    z = _z_two_sided(confidence)
    point = current_price * (1.0 + drift)
    half_width = current_price * sigma_daily * math.sqrt(horizon) * z

    lower = point - half_width
    upper = point + half_width
    # Floor lower to 1% of current price to avoid degenerate intervals
    if lower < current_price * 0.01:
        lower = current_price * 0.01

    params_hash = _stable_hash({
        "fade_factor": fade_factor,
        "drift_cap": drift_cap,
        "vol_lookback": vol_lookback,
        "yoy_avg_months": _DEFAULT_YOY_AVG_MONTHS,
    })

    return {
        "stock_id": stock_id,
        "forecast_date": forecast_date,
        "horizon_days": horizon,
        "lower": round(lower, 4),
        "upper": round(upper, 4),
        "point": round(point, 4),
        "confidence": confidence,
        "calibrated": False,
        "source_core": "fundamental_forecast_core",
        "regime_tag": f"yoy3m={yoy_3m:+.3f}",
        "params_hash": params_hash,
    }
