"""Dumb baseline forecast — RW + volatility cone + trend decomposition.

Spec rule (v0.3 §「強制規則」):
  「笨基準採 RW + 波動錐,趨勢分解只用幾行 numpy(取 DLinear 分解之*想法*,
    不引入模型) — Zeng et al. (2023);Makridakis M-competitions」

Decomposition idea: trend = moving_average(close[-decompose_window:]).
Cone idea: historical horizon-h return distribution at as-of-T, sampled from
trailing cone_lookback_days.  Interval is empirical quantile band of those
returns × trend.

Output `calibrated=False` — empirical quantiles, NOT conformal.  Phase 4 CQR
takes care of calibration; baseline is on the explicit "uncalibrated" whitelist
in the forecast_log CHECK constraint.
"""

from __future__ import annotations

import hashlib
import json
from datetime import date
from typing import Any

import numpy as np


_DEFAULT_DECOMPOSE_WINDOW = 20
_DEFAULT_CONE_LOOKBACK = 252  # ~1 trading year, used as a count over the series


def _stable_hash(payload: dict[str, Any]) -> str:
    blob = json.dumps(payload, sort_keys=True, default=str).encode()
    return hashlib.sha256(blob).hexdigest()[:16]


def make_baseline_forecast(
    series: list[dict[str, Any]],
    forecast_date: date,
    horizon: int,
    confidence: float = 0.80,
    *,
    decompose_window: int = _DEFAULT_DECOMPOSE_WINDOW,
    cone_lookback_days: int = _DEFAULT_CONE_LOOKBACK,
    stock_id: str | None = None,
) -> dict[str, Any] | None:
    """Construct a baseline forecast row.

    Args:
        series: ascending list of rows from `pit.asof_close_series` (must contain
                `date` and `asof_adj_close` keys).  Series must end on/before
                forecast_date — the function does not enforce, caller's contract.
        forecast_date: T (the as-of date).
        horizon: forecast horizon in calendar days (e.g. 21 / 63 / 126).
        confidence: nominal interval coverage (0 < c < 1), e.g. 0.80.
        decompose_window: trend MA window length (trading-day count over series).
        cone_lookback_days: count of trading rows used to sample horizon-h returns.
        stock_id: optional, passed through into the returned dict.

    Returns:
        Forecast row dict suitable for `_db.upsert_forecast`, or None if there
        is insufficient data (need at least decompose_window + horizon + 1 rows).
    """
    closes = [float(r["asof_adj_close"]) for r in series
              if r.get("asof_adj_close") is not None]
    n = len(closes)
    min_required = max(decompose_window, horizon) + 1
    if n < min_required:
        return None

    # Trend = MA of last decompose_window closes (uses asof-adjusted series →
    # already lookahead-clean).
    trend = float(np.mean(closes[-decompose_window:]))

    # Volatility cone: historical horizon-h returns from trailing
    # cone_lookback rows (or fewer if series shorter).
    cone_n = min(cone_lookback_days, n - horizon)
    cone_start = max(0, n - cone_lookback_days - horizon)
    arr = np.asarray(closes[cone_start:], dtype=float)
    if arr.size <= horizon:
        return None
    # arr[i + horizon] / arr[i] - 1 for i in 0 .. len(arr)-horizon-1
    base = arr[:-horizon]
    fut = arr[horizon:]
    # Guard against zero/negative base prices (degenerate stocks)
    mask = base > 0
    if not mask.any():
        return None
    returns = fut[mask] / base[mask] - 1.0
    if returns.size < 5:  # too few samples for empirical band
        return None

    alpha = 1.0 - confidence
    lower_q = float(np.quantile(returns, alpha / 2.0))
    upper_q = float(np.quantile(returns, 1.0 - alpha / 2.0))

    params_hash = _stable_hash({
        "decompose_window": decompose_window,
        "cone_lookback_days": cone_lookback_days,
    })

    return {
        "stock_id": stock_id,
        "forecast_date": forecast_date,
        "horizon_days": horizon,
        "lower": round(trend * (1.0 + lower_q), 4),
        "upper": round(trend * (1.0 + upper_q), 4),
        "point": round(trend, 4),
        "confidence": confidence,
        "calibrated": False,
        "source_core": "baseline",
        "regime_tag": None,
        "params_hash": params_hash,
    }
