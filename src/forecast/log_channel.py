"""log_channel forecast core — trailing-window OLS on log(close).

對齊 user v0.3 區間預測 spec phase 5 + plan 文件
/root/.claude/plans/stockhelper4me-serene-thacker.md phase 5。

對近 window 個 log(asof_adj_close) 做 OLS:
  log_p_t = β0 + β1·t + ε

殘差 std 構通道寬度。投影 t+h:
  log_p_{t+h} = β0 + β1·(t+h)
  variance(log_p_{t+h}) ≈ resid_std² · h   (random-walk-like proxy)
  lower = exp(log_p_{t+h} - z(c) · resid_std · √h)
  upper = exp(log_p_{t+h} + z(c) · resid_std · √h)

Strict spec rules (v0.3 §「強制規則」):
  - trailing window only(禁全段擬合)
  - calibrated=False(empirical regression band,phase 4 CQR 才校準)
  - source_core='log_channel'

Expected to NOT outperform baseline(spec rejection note);kept as decision
support / second opinion.  Same forecast_fn signature as baseline so the same
backtest harness works.
"""

from __future__ import annotations

import hashlib
import json
import math
from datetime import date
from typing import Any

import numpy as np


_DEFAULT_WINDOW = 252  # ~1 trading year


def _stable_hash(payload: dict[str, Any]) -> str:
    blob = json.dumps(payload, sort_keys=True, default=str).encode()
    return hashlib.sha256(blob).hexdigest()[:16]


def _z_two_sided(confidence: float) -> float:
    """Inverse standard normal at quantile (1+c)/2.

    Match the same Acklam-style approximation used in Rust kalman_forecast_core;
    here we use numpy for simplicity (avoids re-implementing).
    """
    from math import sqrt, log, erf
    # numpy has no direct inverse normal, but scipy isn't in deps.
    # Use rational approximation (Abramowitz & Stegun 26.2.23) for speed:
    p = 0.5 + confidence / 2.0
    # Beasley-Springer-Moro approx — accurate to ~1e-7
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


def make_log_channel_forecast(
    series: list[dict[str, Any]],
    forecast_date: date,
    horizon: int,
    confidence: float = 0.80,
    *,
    window: int = _DEFAULT_WINDOW,
    stock_id: str | None = None,
) -> dict[str, Any] | None:
    """Construct a log-channel forecast row.

    Args:
        series: ascending list from `pit.asof_close_series`. Must have at least
                `window` rows with non-NULL `asof_adj_close`.
        forecast_date: T.
        horizon: forecast horizon in calendar days (used for sqrt scaling).
        confidence: 0 < c < 1.
        window: trailing window length (count of trailing bars used for OLS).
        stock_id: optional pass-through.

    Returns:
        Row dict suitable for `_db.upsert_forecast`, or None if insufficient data.
    """
    closes = [
        float(r["asof_adj_close"])
        for r in series
        if r.get("asof_adj_close") is not None and float(r["asof_adj_close"]) > 0
    ]
    n = len(closes)
    if n < window:
        return None

    # Trailing window — slice last `window` entries
    trailing = np.asarray(closes[-window:], dtype=float)
    log_p = np.log(trailing)
    t = np.arange(window, dtype=float)

    # OLS: y = β0 + β1·t.  Use polyfit (degree 1) — simple and stable.
    beta = np.polyfit(t, log_p, deg=1)  # returns [β1, β0]
    slope = float(beta[0])
    intercept = float(beta[1])

    # Residual std (sample sd with dof=2 — n - 2 degrees of freedom)
    fitted = intercept + slope * t
    resid = log_p - fitted
    if window > 2:
        resid_std = float(np.sqrt(np.sum(resid ** 2) / (window - 2)))
    else:
        resid_std = float(np.std(resid))

    # Projection at t + horizon (treat trailing index t = window-1 as current,
    # then project +horizon steps).
    t_proj = (window - 1) + horizon
    log_p_proj = intercept + slope * t_proj

    # Width: random-walk-like scaling sd * sqrt(h)
    z = _z_two_sided(confidence)
    half_width_log = z * resid_std * math.sqrt(horizon)

    point = math.exp(log_p_proj)
    lower = math.exp(log_p_proj - half_width_log)
    upper = math.exp(log_p_proj + half_width_log)

    params_hash = _stable_hash({"window": window})
    return {
        "stock_id": stock_id,
        "forecast_date": forecast_date,
        "horizon_days": horizon,
        "lower": round(lower, 4),
        "upper": round(upper, 4),
        "point": round(point, 4),
        "confidence": confidence,
        "calibrated": False,
        "source_core": "log_channel",
        "regime_tag": None,
        "params_hash": params_hash,
    }
