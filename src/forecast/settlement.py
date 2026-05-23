"""Forecast settlement — fills realized_price + hit + pinball_loss columns.

Settlement uses `src.pit.asof_close_series` to get the realized price at
forecast_date + horizon_days.  Critically, the realized price is computed at
the *settlement-day* asof — NOT read from `price_daily_fwd` (which bakes in
events later than the settlement day).
"""

from __future__ import annotations

from datetime import date, timedelta
from typing import Any

from forecast._db import (
    fetch_unresolved,
    update_settlement,
)
from forecast.scorer import interval_pinball
from pit.ohlcv import asof_close_series


def _settlement_date(forecast_date: date, horizon_days: int) -> date:
    """Forecast becomes due for settlement on forecast_date + horizon_days (calendar)."""
    return forecast_date + timedelta(days=horizon_days)


def _realized_close(
    conn,
    stock_id: str,
    settle_date: date,
    market: str = "TW",
) -> float | None:
    """Get realized as-of-settle_date close (adjusted to settle_date view).

    Looks back up to 21 calendar days to handle weekends/holidays.  Returns
    the last available adjusted close on or before settle_date.
    """
    series = asof_close_series(
        conn,
        stock_id=stock_id,
        asof_t=settle_date,
        lookback_days=21,
        market=market,
    )
    if not series:
        return None
    # Take last available row ≤ settle_date
    return float(series[-1]["asof_adj_close"])


def resolve_pending(
    conn,
    asof: date,
    *,
    source_core: str | None = None,
    stock_id: str | None = None,
    market: str = "TW",
) -> dict[str, int]:
    """Resolve all pending forecasts whose horizon has elapsed by `asof`.

    Returns a summary dict: {settled, missing_realized, errored}.
    """
    pending = fetch_unresolved(
        conn, asof=asof, source_core=source_core, stock_id=stock_id
    )
    settled = 0
    missing = 0
    errored = 0
    for row in pending:
        settle_date = _settlement_date(row["forecast_date"], row["horizon_days"])
        try:
            realized = _realized_close(
                conn, row["stock_id"], settle_date, market=market
            )
        except Exception:
            errored += 1
            continue
        if realized is None:
            missing += 1
            continue
        lower = row.get("lower")
        upper = row.get("upper")
        if lower is None or upper is None:
            errored += 1
            continue
        lower_f = float(lower)
        upper_f = float(upper)
        hit = lower_f <= realized <= upper_f
        pinball = interval_pinball(
            realized=realized,
            lower=lower_f,
            upper=upper_f,
            confidence=float(row["confidence"]),
        )
        update_settlement(
            conn,
            row_id=row["id"],
            resolved_date=settle_date,
            realized_price=realized,
            hit=hit,
            pinball_loss=pinball,
        )
        settled += 1
    return {"settled": settled, "missing_realized": missing, "errored": errored}
