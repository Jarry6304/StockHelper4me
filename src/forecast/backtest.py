"""Causal one-pass backtest harness.

For each trading day T in [start, end]:
  1. Reconstruct as-of-T series via `pit.asof_close_series` (lookahead-safe)
  2. For each (horizon, confidence) pair, call forecast_fn
  3. Upsert into forecast_log

Spec rule (v0.3 §「回測因果迴路」):
  "core 一次 pass 對每個歷史 bar 只用過去資料產生一筆 forecast.
   禁止 centered window、full-series 正規化、smoothed state."
"""

from __future__ import annotations

import logging
from datetime import date, timedelta
from typing import Any, Callable, Iterable

from forecast._db import upsert_forecast
from pit._calendar import trading_days_between
from pit.ohlcv import asof_close_series

logger = logging.getLogger("forecast.backtest")


# Forecast function signature: (series, forecast_date, horizon, confidence) -> dict | None
ForecastFn = Callable[[list[dict[str, Any]], date, int, float], dict[str, Any] | None]


def run_backtest(
    conn,
    stock_id: str,
    forecast_fn: ForecastFn,
    source_core: str,
    start: date,
    end: date,
    *,
    horizons: Iterable[int] = (21, 63, 126),
    confidences: Iterable[float] = (0.50, 0.80, 0.95),
    series_lookback_days: int = 365 * 2,
    market: str = "TW",
    progress_every: int = 60,
) -> dict[str, int]:
    """Run causal backtest for one stock over [start, end].

    Args:
        conn: psycopg connection.
        stock_id: e.g. "2330".
        forecast_fn: callable(series, T, horizon, confidence) -> dict | None.
                     Returned dict must include keys consumed by upsert_forecast
                     (`stock_id` will be overwritten by harness).
        source_core: tag written into forecast_log.source_core.
        start, end: inclusive date range.  Both should be trading days; the
                    harness intersects with trading_date_ref.
        horizons: forecast horizons in calendar days.
        confidences: nominal coverage probabilities.
        series_lookback_days: how far back to fetch the input series at each T.
                              Must be large enough for the forecast_fn's needs
                              (default ~2 years).
        market: default "TW".
        progress_every: log progress every N trading days.

    Returns:
        Summary dict: {trading_days, attempted, written, skipped}.
    """
    days = trading_days_between(conn, start=start, end=end, market=market)
    if not days:
        logger.info(
            "no trading days in [%s, %s] for market=%s",
            start, end, market,
        )
        return {"trading_days": 0, "attempted": 0, "written": 0, "skipped": 0}

    horizons_t = tuple(int(h) for h in horizons)
    confidences_t = tuple(float(c) for c in confidences)

    attempted = 0
    written = 0
    skipped = 0
    for i, T in enumerate(days, start=1):
        series = asof_close_series(
            conn,
            stock_id=stock_id,
            asof_t=T,
            lookback_days=series_lookback_days,
            market=market,
        )
        if not series:
            skipped += len(horizons_t) * len(confidences_t)
            continue
        for h in horizons_t:
            for c in confidences_t:
                attempted += 1
                row = forecast_fn(series, T, h, c)
                if row is None:
                    skipped += 1
                    continue
                # Harness owns stock_id + source_core (caller's forecast_fn
                # may leave them unset / inconsistent).
                row["stock_id"] = stock_id
                row["source_core"] = source_core
                row.setdefault("forecast_date", T)
                row.setdefault("horizon_days", h)
                row.setdefault("confidence", c)
                upsert_forecast(conn, row)
                written += 1
        if progress_every and i % progress_every == 0:
            logger.info(
                "backtest %s %s: %d/%d days, %d rows written",
                source_core, stock_id, i, len(days), written,
            )
    return {
        "trading_days": len(days),
        "attempted": attempted,
        "written": written,
        "skipped": skipped,
    }
