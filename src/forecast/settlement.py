"""Forecast settlement — fills realized_price + hit + pinball_loss columns.

Settlement uses `src.pit.asof_close_series` to get the realized price at
forecast_date + horizon_days.  Critically, the realized price is computed at
the *settlement-day* asof — NOT read from `price_daily_fwd` (which bakes in
events later than the settlement day).
"""

from __future__ import annotations

from datetime import date, timedelta
from itertools import groupby
from typing import Any

from forecast._db import (
    fetch_unresolved,
    update_settlement,
    update_settlement_batch,
)
from forecast.scorer import interval_pinball
from pit.ohlcv import asof_close_series

# sentinel:_realized_close 對某 (stock, settle_date) 拋例外 → 快取此值,避免重算
# 且讓所有共用該 key 的 row 都計入 errored(與逐筆版同決策)。
_REALIZED_ERROR = object()


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

    # (stock_id, settle_date) → realized | None | _REALIZED_ERROR。同一 key 在 3 個
    # confidence(同 forecast_date+horizon)重複出現 → memo 後 PIT 讀次 ÷3。PIT 數值
    # 邏輯零改(仍逐 (stock, asof_t) 走 asof_close_series 的 event-multiplier 重建)。
    realized_cache: dict[tuple[str, date], Any] = {}

    def _realized_cached(sid: str, sdate: date) -> Any:
        key = (sid, sdate)
        if key not in realized_cache:
            try:
                realized_cache[key] = _realized_close(conn, sid, sdate, market=market)
            except Exception:
                realized_cache[key] = _REALIZED_ERROR
        return realized_cache[key]

    # fetch_unresolved 以 stock_id 為首鍵排序 → 同股 row 連續 → 每股一個 transaction。
    for _sid, group in groupby(pending, key=lambda r: r["stock_id"]):
        updates: list[dict[str, Any]] = []
        for row in group:
            settle_date = _settlement_date(row["forecast_date"], row["horizon_days"])
            realized = _realized_cached(row["stock_id"], settle_date)
            if realized is _REALIZED_ERROR:
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
            updates.append({
                "row_id": row["id"],
                "resolved_date": settle_date,
                "realized_price": realized,
                "hit": hit,
                "pinball_loss": pinball,
            })

        if updates:
            with conn.transaction():
                update_settlement_batch(conn, updates)
            settled += len(updates)

    return {"settled": settled, "missing_realized": missing, "errored": errored}
