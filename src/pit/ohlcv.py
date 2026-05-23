"""Point-in-time OHLCV reconstruction from Bronze.

Given (stock_id, asof_t), reconstructs the as-of-T view of price series by:
  1. Reading raw `price_daily` (never adjusted)
  2. Reading `price_adjustment_events` filtered to date ≤ asof_t
  3. For each row at date T0, applying cumprod(AF[e] for e in events where
     T0 < e.date ≤ asof_t)

Formula sources — Python mirror of
rust_compute/silver_s1_adjustment/src/main.rs:

- AF priority 1 (line 365-385): af = before_price / reference_price
  with unreliability check for pure-stock dividends where bp==rp.
- AF priority 2 (line 387-410): dividend fallback
    p_after = (bp - cash) / (1 + stock / 10);  af = bp / p_after
- AF priority 3 (line 677-690): capital_increase from detail JSONB
    (subscription_price + subscription_rate_raw)
- Forward loop (line 599-638): "先 push 再更新 multiplier" —
  row at date T0 uses multiplier = product(AF[e] for e where e.date > T0).
"""

from __future__ import annotations

from datetime import date, timedelta
from typing import Any


def _compute_event_af(event: dict, raw_prev_close: float | None) -> tuple[float, float]:
    """Compute (af, vf) for a single price_adjustment_events row.

    Mirror of rust_compute/silver_s1_adjustment/src/main.rs::derive_simple_event_af
    + patch_capital_increase_af.
    """
    event_type = event["event_type"]
    bp = event.get("before_price")
    rp = event.get("reference_price")
    cash = event.get("cash_dividend") or 0.0
    stock = event.get("stock_dividend") or 0.0
    vf = event.get("volume_factor")
    vf = 1.0 if vf is None else float(vf)

    # Priority 1: API exact values (with reliability check for pure-stock div)
    if bp is not None and rp is not None and float(rp) > 0 and float(bp) > 0:
        bp_f = float(bp)
        rp_f = float(rp)
        bp_eq_rp = abs(bp_f - rp_f) < 0.0001
        unreliable = (
            event_type == "dividend"
            and float(stock) > 0
            and float(cash) == 0
            and bp_eq_rp
        )
        if not unreliable:
            return (bp_f / rp_f, vf)

    # Priority 2: dividend fallback formula
    if event_type == "dividend":
        bp_use = float(bp) if bp is not None else raw_prev_close
        if bp_use is not None and bp_use > 0:
            cash_f = float(cash)
            stock_f = float(stock)
            if cash_f > 0 or stock_f > 0:
                p_after = (bp_use - cash_f) / (1.0 + stock_f / 10.0)
                if p_after > 0:
                    return (bp_use / p_after, vf)

    # Priority 3: capital_increase from detail JSONB
    if event_type == "capital_increase":
        detail = event.get("detail") or {}
        if isinstance(detail, str):
            import json
            try:
                detail = json.loads(detail)
            except Exception:
                detail = {}
        sub_price = detail.get("subscription_price")
        sub_rate = detail.get("subscription_rate_raw")
        if (
            sub_price is not None and float(sub_price) > 0
            and sub_rate is not None and float(sub_rate) > 0
            and raw_prev_close is not None and raw_prev_close > 0
        ):
            r = float(sub_rate) / 1000.0
            after_price = (raw_prev_close + float(sub_price) * r) / (1.0 + r)
            if after_price > 0:
                return (raw_prev_close / after_price, vf)

    # Default: no adjustment
    return (1.0, vf)


def _build_event_multipliers(
    event_rows: list[dict],
    raw_close_by_date: dict[date, float],
) -> tuple[dict[date, float], dict[date, float]]:
    """Group events by date and combine multiplicatively (mirrors Rust line 603-610)."""
    raw_dates_sorted = sorted(raw_close_by_date.keys())
    event_af: dict[date, float] = {}
    event_vf: dict[date, float] = {}
    for ev in event_rows:
        # Find raw close strictly before event date for fallback formulas
        prev_close: float | None = None
        for d in reversed(raw_dates_sorted):
            if d < ev["date"]:
                prev_close = float(raw_close_by_date[d])
                break
        af, vf = _compute_event_af(ev, prev_close)
        if abs(af - 1.0) > 1e-12:
            event_af[ev["date"]] = event_af.get(ev["date"], 1.0) * af
        if abs(vf - 1.0) > 1e-12:
            event_vf[ev["date"]] = event_vf.get(ev["date"], 1.0) * vf
    return event_af, event_vf


def asof_close_series(
    conn,
    stock_id: str,
    asof_t: date,
    lookback_days: int,
    market: str = "TW",
) -> list[dict[str, Any]]:
    """Reconstruct as-of-T close/volume series.

    Args:
        conn: psycopg connection with dict_row factory.
        stock_id: e.g. "2330".
        asof_t: observer date.  All info ≤ this date is visible; > is hidden.
        lookback_days: calendar days from asof_t to fetch.
        market: default "TW".

    Returns:
        Ascending list of {date, raw_close, asof_adj_close, volume, asof_adj_volume}.
        Empty list if no data.
    """
    rows = asof_ohlc(conn, stock_id, asof_t, lookback_days, market=market)
    return [
        {
            "date": r["date"],
            "raw_close": r["raw_close"],
            "asof_adj_close": r["asof_adj_close"],
            "volume": r["volume"],
            "asof_adj_volume": r["asof_adj_volume"],
        }
        for r in rows
    ]


def asof_ohlc(
    conn,
    stock_id: str,
    asof_t: date,
    lookback_days: int,
    market: str = "TW",
) -> list[dict[str, Any]]:
    """As asof_close_series, but returns full OHLC + volume.

    Returns: [{date, raw_open, raw_high, raw_low, raw_close, volume,
               asof_adj_open, asof_adj_high, asof_adj_low, asof_adj_close,
               asof_adj_volume}, ...]
    """
    start_date = asof_t - timedelta(days=lookback_days)

    with conn.cursor() as cur:
        cur.execute(
            """SELECT date, open::float8 AS open, high::float8 AS high,
                      low::float8 AS low, close::float8 AS close, volume
               FROM price_daily
               WHERE market = %s AND stock_id = %s
                 AND date >= %s AND date <= %s
               ORDER BY date""",
            (market, stock_id, start_date, asof_t),
        )
        raw_rows = list(cur.fetchall())

    if not raw_rows:
        return []

    earliest_raw_date = raw_rows[0]["date"]
    with conn.cursor() as cur:
        cur.execute(
            """SELECT date, event_type,
                      before_price::float8 AS before_price,
                      reference_price::float8 AS reference_price,
                      cash_dividend::float8 AS cash_dividend,
                      stock_dividend::float8 AS stock_dividend,
                      volume_factor::float8 AS volume_factor,
                      detail
               FROM price_adjustment_events
               WHERE market = %s AND stock_id = %s
                 AND date > %s AND date <= %s
               ORDER BY date""",
            (market, stock_id, earliest_raw_date, asof_t),
        )
        event_rows = list(cur.fetchall())

    raw_close_by_date = {r["date"]: r["close"] for r in raw_rows if r.get("close") is not None}
    event_af, event_vf = _build_event_multipliers(event_rows, raw_close_by_date)

    # Forward loop reversed: push current row first, then update multiplier
    # for earlier dates (per Rust line 614-634: "先 push 再更新 multiplier").
    result_reversed: list[dict[str, Any]] = []
    price_mult = 1.0
    volume_mult = 1.0
    for r in reversed(raw_rows):
        open_v = r.get("open")
        high_v = r.get("high")
        low_v = r.get("low")
        close_v = r.get("close")
        vol_v = r.get("volume")

        def adj_p(v: Any) -> float | None:
            if v is None:
                return None
            # Match Rust's 2-dp rounding (line 622-625 uses *100/100)
            return round(float(v) * price_mult * 10000) / 10000

        adj_vol = None
        if vol_v is not None and volume_mult != 0:
            adj_vol = int(round(float(vol_v) / volume_mult))

        result_reversed.append({
            "date": r["date"],
            "raw_open": open_v,
            "raw_high": high_v,
            "raw_low": low_v,
            "raw_close": close_v,
            "volume": vol_v,
            "asof_adj_open": adj_p(open_v),
            "asof_adj_high": adj_p(high_v),
            "asof_adj_low": adj_p(low_v),
            "asof_adj_close": adj_p(close_v),
            "asof_adj_volume": adj_vol,
        })
        af = event_af.get(r["date"])
        if af is not None:
            price_mult *= af
        vf = event_vf.get(r["date"])
        if vf is not None:
            volume_mult *= vf

    return list(reversed(result_reversed))
