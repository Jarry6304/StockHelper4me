"""chip_forecast_core — institutional flow z-score + margin balance contrarian drift.

M8 sprint 第三個非 price-only forecast core(2026-05-24)。對齊 CLAUDE.md v4.23
fusion future work 提案:chip 信號(法人籌碼 + 融資餘額動能)誤差獨立於 price /
macro / fundamental,讓 fusion 真正收 Bates-Granger 1969 變異數縮減。

訊號設計
========

兩個 chip 信號合成 per-stock drift:

1. **Institutional flow z-score**(primary,weight 0.7):
   - net_flow_t = Σ(buy_t) − Σ(sell_t) 跨 5 類法人
     (foreign / foreign_dealer_self / investment_trust / dealer / dealer_hedging)
   - rolling 20-day mean(net_flow_recent),60-day mean/std baseline
   - z = (net_flow_recent − mean_60d) / std_60d
   - score = clamp(z / 2.0, -1, +1)  # |z|=2 對應滿格 signal
   - z > +1 → 法人累積中 → 偏多訊號

2. **Margin balance contrarian**(secondary,weight 0.3):
   - margin_balance 20-day rate of change
   - **負號方向**(contrarian:retail 過度槓桿 → 短線見頂)
   - score = -clamp(roc / 0.10, -1, +1)  # 10% margin 動能對應滿格
   - 對應 Hong & Sraer 2016 JFE leverage cycle / Adrian-Etula-Muir 2014 JF
     broker-dealer leverage cycle

合成:`chip_score = 0.7 * inst_score + 0.3 * margin_score` ∈ [-1, +1]

漂移映射:
    drift_horizon = clamp(
        chip_score * fade_factor * (horizon / 252),
        -DRIFT_CAP, +DRIFT_CAP,
    )

`fade_factor=0.35`(介於 fundamental 0.3 與 macro 0.4 之間 — chip 訊號對台股
單日報酬有相關但持續性不長)。`DRIFT_CAP=0.18`(略嚴於 fundamental,留 buffer)。

PIT-safety:institutional + margin 都是同日盤後公布(T 結束時可拿到 date=T),
SQL `date <= asof_t` 直接過濾。
"""

from __future__ import annotations

import hashlib
import json
import math
from datetime import date, timedelta
from typing import Any

import numpy as np


_DEFAULT_FADE_FACTOR = 0.35
_DEFAULT_DRIFT_CAP = 0.18
_DEFAULT_VOL_LOOKBACK_DAYS = 60
_DEFAULT_INST_RECENT_DAYS = 20
_DEFAULT_INST_BASELINE_DAYS = 60
_DEFAULT_INST_Z_SATURATION = 2.0
_DEFAULT_MARGIN_ROC_DAYS = 20
_DEFAULT_MARGIN_SATURATION_PCT = 0.10
_DEFAULT_INST_WEIGHT = 0.7
_DEFAULT_MARGIN_WEIGHT = 0.3


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


def _net_flow(row: dict[str, Any]) -> float | None:
    """Compute net institutional flow for one institutional_daily_derived row.

    sum of all 5 categories' (buy - sell). NULL treated as 0 (對齊 v3.14 SQL
    SUM 行為)。Returns None if row is empty/unparseable.
    """
    if not row:
        return None
    cols = [
        ("foreign_buy", "foreign_sell"),
        ("foreign_dealer_self_buy", "foreign_dealer_self_sell"),
        ("investment_trust_buy", "investment_trust_sell"),
        ("dealer_buy", "dealer_sell"),
        ("dealer_hedging_buy", "dealer_hedging_sell"),
    ]
    total = 0.0
    saw_any = False
    for b_col, s_col in cols:
        b = row.get(b_col)
        s = row.get(s_col)
        if b is not None or s is not None:
            saw_any = True
        total += (float(b) if b is not None else 0.0) \
              - (float(s) if s is not None else 0.0)
    if not saw_any:
        return None
    return total


def _compute_inst_score(
    inst_rows: list[dict[str, Any]],
    recent_days: int = _DEFAULT_INST_RECENT_DAYS,
    baseline_days: int = _DEFAULT_INST_BASELINE_DAYS,
    z_saturation: float = _DEFAULT_INST_Z_SATURATION,
) -> float | None:
    """Net institutional flow z-score(規一到 [-1, +1])。

    Rows must have institutional buy/sell columns and `date` key. We sort by date.
    """
    if not inst_rows:
        return None
    rows_sorted = sorted(inst_rows, key=lambda r: r["date"])
    flows = [(_net_flow(r),) for r in rows_sorted]
    flows_clean = [f[0] for f in flows if f[0] is not None]
    if len(flows_clean) < baseline_days:
        return None

    arr = np.asarray(flows_clean, dtype=float)
    baseline = arr[-baseline_days:]
    recent = arr[-recent_days:]
    mean = float(np.mean(baseline))
    std = float(np.std(baseline, ddof=1))
    if not np.isfinite(std) or std <= 0:
        return None
    z = (float(np.mean(recent)) - mean) / std
    score = z / z_saturation
    return max(-1.0, min(1.0, score))


def _compute_margin_score(
    margin_rows: list[dict[str, Any]],
    roc_days: int = _DEFAULT_MARGIN_ROC_DAYS,
    saturation_pct: float = _DEFAULT_MARGIN_SATURATION_PCT,
) -> float | None:
    """Margin balance rate-of-change → contrarian score(注意負號).

    margin_balance 上升 = 散戶加槓桿 = 偏空(contrarian);所以返回 -roc / saturation。
    """
    if not margin_rows:
        return None
    rows_sorted = sorted(margin_rows, key=lambda r: r["date"])
    balances = [
        float(r["margin_balance"]) for r in rows_sorted
        if r.get("margin_balance") is not None and float(r["margin_balance"]) > 0
    ]
    if len(balances) < roc_days + 1:
        return None
    cur = balances[-1]
    base = balances[-(roc_days + 1)]
    roc = (cur - base) / base
    # Contrarian: positive roc → negative score
    score = -roc / saturation_pct
    return max(-1.0, min(1.0, float(score)))


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


def _fetch_inst_rows(conn, stock_id: str, asof_t: date,
                     lookback_days: int = 180,
                     market: str = "TW") -> list[dict[str, Any]]:
    """PIT-safe: date <= asof_t(同日盤後公布)."""
    earliest = asof_t - timedelta(days=lookback_days)
    with conn.cursor() as cur:
        cur.execute(
            """SELECT date, foreign_buy, foreign_sell,
                      foreign_dealer_self_buy, foreign_dealer_self_sell,
                      investment_trust_buy, investment_trust_sell,
                      dealer_buy, dealer_sell,
                      dealer_hedging_buy, dealer_hedging_sell
                 FROM institutional_daily_derived
                WHERE market = %s AND stock_id = %s
                  AND date BETWEEN %s AND %s
                ORDER BY date""",
            (market, stock_id, earliest, asof_t),
        )
        return list(cur.fetchall())


def _fetch_margin_rows(conn, stock_id: str, asof_t: date,
                       lookback_days: int = 90,
                       market: str = "TW") -> list[dict[str, Any]]:
    """PIT-safe: date <= asof_t."""
    earliest = asof_t - timedelta(days=lookback_days)
    with conn.cursor() as cur:
        cur.execute(
            """SELECT date, margin_balance
                 FROM margin_daily_derived
                WHERE market = %s AND stock_id = %s
                  AND date BETWEEN %s AND %s
                ORDER BY date""",
            (market, stock_id, earliest, asof_t),
        )
        return list(cur.fetchall())


def make_chip_forecast(
    series: list[dict[str, Any]],
    forecast_date: date,
    horizon: int,
    confidence: float = 0.80,
    *,
    conn=None,
    stock_id: str | None = None,
    inst_rows: list[dict[str, Any]] | None = None,
    margin_rows: list[dict[str, Any]] | None = None,
    fade_factor: float = _DEFAULT_FADE_FACTOR,
    drift_cap: float = _DEFAULT_DRIFT_CAP,
    vol_lookback: int = _DEFAULT_VOL_LOOKBACK_DAYS,
    inst_weight: float = _DEFAULT_INST_WEIGHT,
    margin_weight: float = _DEFAULT_MARGIN_WEIGHT,
    market: str = "TW",
) -> dict[str, Any] | None:
    """Construct a chip forecast row.

    Args:
        series: price series for spot price + realized vol calibration.
        forecast_date, horizon, confidence:as usual。
        conn: optional PG connection — fetches via PIT-safe queries if chip
              rows are None.
        stock_id: required when fetching via conn.
        inst_rows / margin_rows: optional pre-fetched(tests)。
        fade_factor / drift_cap / vol_lookback:calibration knobs。
        inst_weight / margin_weight:合成權重(預設 0.7 / 0.3)。
        market: default "TW".

    Returns:
        Row dict for upsert_forecast, or None if both chip signals missing /
        no price series / vol uncomputable.
    """
    # 1. Fetch chip rows
    if inst_rows is None:
        if conn is None or stock_id is None:
            return None
        inst_rows = _fetch_inst_rows(
            conn, stock_id=stock_id, asof_t=forecast_date,
            market=market,
        )
    if margin_rows is None:
        if conn is None or stock_id is None:
            margin_rows = []  # margin optional;inst may still work
        else:
            margin_rows = _fetch_margin_rows(
                conn, stock_id=stock_id, asof_t=forecast_date,
                market=market,
            )

    inst_score = _compute_inst_score(inst_rows)
    margin_score = _compute_margin_score(margin_rows)
    if inst_score is None and margin_score is None:
        return None

    # If one missing, normalize remaining weight
    if inst_score is None:
        chip_score = margin_score
    elif margin_score is None:
        chip_score = inst_score
    else:
        # Re-normalize weights to sum=1 in case caller overrode (defensive)
        total_w = inst_weight + margin_weight
        if total_w <= 0:
            return None
        chip_score = (
            inst_weight * inst_score + margin_weight * margin_score
        ) / total_w

    # 2. Spot price
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
    drift_raw = chip_score * fade_factor * (horizon / 252.0)
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
        "inst_recent_days": _DEFAULT_INST_RECENT_DAYS,
        "inst_baseline_days": _DEFAULT_INST_BASELINE_DAYS,
        "inst_z_saturation": _DEFAULT_INST_Z_SATURATION,
        "margin_roc_days": _DEFAULT_MARGIN_ROC_DAYS,
        "margin_saturation_pct": _DEFAULT_MARGIN_SATURATION_PCT,
        "inst_weight": inst_weight,
        "margin_weight": margin_weight,
    })

    regime_parts = []
    if inst_score is not None:
        regime_parts.append(f"inst={inst_score:+.2f}")
    if margin_score is not None:
        regime_parts.append(f"margin={margin_score:+.2f}")
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
        "source_core": "chip_forecast_core",
        "regime_tag": regime_tag,
        "params_hash": params_hash,
    }
