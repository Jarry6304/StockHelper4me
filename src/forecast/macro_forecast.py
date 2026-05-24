"""macro_forecast_core — TWD/USD momentum + business indicator color drift.

M8 sprint 第二個非 price-only forecast core。對齊 CLAUDE.md v4.23 fusion future
work 提案:macro 信號(FX + 景氣)誤差與 price-based cores 低相關,讓 fusion
真正能收 Bates-Granger 1969 變異數縮減。

訊號設計
========

兩個 macro 信號合成 single market-wide drift(V1 簡化:不做 per-stock beta):

1. **TWD/USD momentum**(21d % change):
   - rate up = TWD weakening = 出口型企業受惠(主要台股)= 偏多訊號
   - score = clamp(roc_21d / 0.02, -1, +1)  # 2% 動能對應滿格 signal

2. **Business indicator monitoring color**(國發會景氣燈號):
   - 對映:Blue(-2)/Yellow-Blue(-1)/Green(0)/Yellow-Red(+1)/Red(+2)
   - 中文「藍/黃藍/綠/黃紅/紅」+ 縮寫對映,對齊 business_indicator_core
   - score = color_value / 2.0  # 規一到 [-1, +1]

合成:`macro_score = 0.5 * twd_score + 0.5 * business_score` ∈ [-1, +1]

漂移映射:
    drift_horizon = clamp(
        macro_score * fade_factor * (horizon / 252),
        -DRIFT_CAP, +DRIFT_CAP,
    )

`fade_factor=0.4`(略高於 fundamental 的 0.3 — macro signal 對 broad market
beta 接近 1)。`DRIFT_CAP=0.15`(macro 信號穩定 → drift cap 略嚴於 fundamental
的 0.20 防 outlier 個股)。

V1 限制:per-stock 套用 same macro_score。V2 可加 sector beta(電子/金融/傳產
對 TWD 反應不同)。
"""

from __future__ import annotations

import hashlib
import json
import math
from datetime import date, timedelta
from typing import Any

import numpy as np


_DEFAULT_FADE_FACTOR = 0.4
_DEFAULT_DRIFT_CAP = 0.15
_DEFAULT_VOL_LOOKBACK_DAYS = 60
_DEFAULT_TWD_LOOKBACK_DAYS = 21
_DEFAULT_TWD_SATURATION_PCT = 0.02


# 對齊 business_indicator_core::MonitoringColor::from_label 解析
# (英文全名 / 縮寫 / 中文 + 燈)→ 數值
_COLOR_MAP: dict[str, int] = {
    # English
    "blue": -2, "yellow_blue": -1, "green": 0, "yellow_red": 1, "red": 2,
    # Abbreviations(v3.21 schema 契約)
    "b": -2, "yb": -1, "g": 0, "yr": 1, "r": 2,
    # Chinese
    "藍": -2, "黃藍": -1, "綠": 0, "黃紅": 1, "紅": 2,
    "藍燈": -2, "黃藍燈": -1, "綠燈": 0, "黃紅燈": 1, "紅燈": 2,
}


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


def _parse_color_score(label: Any) -> int | None:
    """color label → integer score in [-2, +2], or None if unparseable."""
    if label is None:
        return None
    key = str(label).strip().lower()
    # Try exact, then strip Chinese trailing "燈" character handled in map
    if key in _COLOR_MAP:
        return _COLOR_MAP[key]
    return None


def _compute_twd_momentum_score(
    fx_rows: list[dict[str, Any]],
    lookback_days: int = _DEFAULT_TWD_LOOKBACK_DAYS,
    saturation_pct: float = _DEFAULT_TWD_SATURATION_PCT,
) -> float | None:
    """TWD/USD `saturation_pct` 動能規一到 [-1, +1]。

    Rows must have `date` and `rate` keys. Sort by date ascending.
    """
    rates = [
        (r["date"], float(r["rate"]))
        for r in fx_rows
        if r.get("date") and r.get("rate") is not None and float(r["rate"]) > 0
    ]
    if len(rates) < lookback_days + 1:
        return None
    rates.sort(key=lambda x: x[0])
    rate_now = rates[-1][1]
    rate_then = rates[-(lookback_days + 1)][1]
    roc = (rate_now - rate_then) / rate_then
    score = roc / saturation_pct
    return max(-1.0, min(1.0, float(score)))


def _compute_business_indicator_score(
    indicator_rows: list[dict[str, Any]],
) -> float | None:
    """Business indicator 最新一筆 monitoring_color 規一到 [-1, +1]。

    Rows are sorted by date ascending; we take the latest non-null color.
    """
    if not indicator_rows:
        return None
    # Reverse iterate to find latest valid color
    for r in reversed(indicator_rows):
        score = _parse_color_score(r.get("monitoring_color"))
        if score is not None:
            return float(score) / 2.0
    return None


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


def _fetch_fx_rows(conn, asof_t: date, currency: str = "USD",
                   lookback_days: int = 90, market: str = "tw") -> list[dict[str, Any]]:
    """Fetch FX rate rows for currency through asof_t.

    PIT-safety: `date <= asof_t - 1 day`(spot rate published next morning per
    BoT release schedule;對 backtest 保守起見扣 1 天)。
    """
    cutoff = asof_t - timedelta(days=1)
    earliest = asof_t - timedelta(days=lookback_days)
    with conn.cursor() as cur:
        cur.execute(
            """SELECT date, rate
                 FROM exchange_rate
                WHERE market = %s AND currency = %s
                  AND date BETWEEN %s AND %s
                ORDER BY date""",
            (market, currency, earliest, cutoff),
        )
        return list(cur.fetchall())


def make_macro_forecast(
    series: list[dict[str, Any]],
    forecast_date: date,
    horizon: int,
    confidence: float = 0.80,
    *,
    conn=None,
    stock_id: str | None = None,
    fx_rows: list[dict[str, Any]] | None = None,
    business_rows: list[dict[str, Any]] | None = None,
    fade_factor: float = _DEFAULT_FADE_FACTOR,
    drift_cap: float = _DEFAULT_DRIFT_CAP,
    vol_lookback: int = _DEFAULT_VOL_LOOKBACK_DAYS,
    twd_lookback: int = _DEFAULT_TWD_LOOKBACK_DAYS,
    twd_saturation: float = _DEFAULT_TWD_SATURATION_PCT,
    fx_currency: str = "USD",
    market: str = "TW",
) -> dict[str, Any] | None:
    """Construct a macro forecast row.

    Macro signal is market-wide (V1):per-stock 共用 same score。

    Args:
        series: price series (asof_close_series output) — used for spot price +
                realized vol calibration.
        forecast_date: T.
        horizon: 21 / 63 / 126.
        confidence: 0 < c < 1.
        conn: optional PG connection. If provided + macro rows are None,
              we fetch via PIT-safe queries.
        stock_id: e.g. "2330" (used only for row output identification).
        fx_rows: optional pre-fetched FX rows (for tests).
        business_rows: optional pre-fetched business_indicator rows (for tests).
        fade_factor / drift_cap / vol_lookback / twd_lookback / twd_saturation:
            calibration knobs(見 module docstring §訊號設計).
        fx_currency: default "USD".
        market: passed to pit fetcher / SQL; default "TW" for forecast,
                business indicator uses lowercase "tw" internally.

    Returns:
        Row dict for upsert_forecast, or None if either macro signal unavailable.
    """
    # 1. Fetch macro signals if not provided
    if fx_rows is None:
        if conn is None:
            return None
        fx_rows = _fetch_fx_rows(
            conn, asof_t=forecast_date, currency=fx_currency,
            lookback_days=max(twd_lookback * 3, 90),
            market="tw",
        )
    if business_rows is None:
        if conn is None:
            return None
        try:
            from pit.fundamental import asof_business_indicator
        except ImportError:
            return None
        business_rows = asof_business_indicator(
            conn, asof_t=forecast_date, market="tw",
        )

    twd_score = _compute_twd_momentum_score(
        fx_rows, lookback_days=twd_lookback, saturation_pct=twd_saturation,
    )
    biz_score = _compute_business_indicator_score(business_rows)
    if twd_score is None and biz_score is None:
        return None

    # If only one available, use it alone (V1 design — better than nothing)
    if twd_score is None:
        macro_score = biz_score
    elif biz_score is None:
        macro_score = twd_score
    else:
        macro_score = 0.5 * twd_score + 0.5 * biz_score

    # 2. Get current price
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

    # 4. Drift
    drift_raw = macro_score * fade_factor * (horizon / 252.0)
    drift = max(-drift_cap, min(drift_cap, drift_raw))

    # 5. Interval
    z = _z_two_sided(confidence)
    point = current_price * (1.0 + drift)
    half_width = current_price * sigma_daily * math.sqrt(horizon) * z

    lower = point - half_width
    upper = point + half_width
    if lower < current_price * 0.01:
        lower = current_price * 0.01

    params_hash = _stable_hash({
        "fade_factor": fade_factor,
        "drift_cap": drift_cap,
        "vol_lookback": vol_lookback,
        "twd_lookback": twd_lookback,
        "twd_saturation": twd_saturation,
        "fx_currency": fx_currency,
    })

    regime_parts = []
    if twd_score is not None:
        regime_parts.append(f"twd={twd_score:+.2f}")
    if biz_score is not None:
        regime_parts.append(f"biz={biz_score:+.2f}")
    regime_tag = "/".join(regime_parts)

    return {
        "stock_id": stock_id,
        "forecast_date": forecast_date,
        "horizon_days": horizon,
        "lower": round(lower, 4),
        "upper": round(upper, 4),
        "point": round(point, 4),
        "confidence": confidence,
        "calibrated": False,
        "source_core": "macro_forecast_core",
        "regime_tag": regime_tag,
        "params_hash": params_hash,
    }
