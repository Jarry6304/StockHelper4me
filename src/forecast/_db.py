"""forecast_log table accessors.

Reuses `src.fusion.raw._db.get_connection` for single-entry connection management.
"""

from __future__ import annotations

from datetime import date
from typing import Any, Iterable

from fusion.raw._db import get_connection  # re-export

__all__ = [
    "get_connection",
    "upsert_forecast",
    "fetch_unresolved",
    "fetch_resolved",
]


def upsert_forecast(conn, row: dict[str, Any]) -> None:
    """Upsert a single forecast row into forecast_log.

    Required keys:
        stock_id, forecast_date, horizon_days, confidence, source_core
    Optional keys:
        lower, upper, point, calibrated (default False),
        internal_only (default False — 對齊 dual_track_resonance §七 B-4 機制丙;
            neely_fib emitter 傳 True,其他 emitter 維持 False),
        regime_tag, params_hash

    Settlement columns (resolved_date, realized_price, hit, pinball_loss) are
    not touched by this function — see `settlement.resolve_pending`.
    """
    sql = """
        INSERT INTO forecast_log (
            stock_id, forecast_date, horizon_days, lower, upper, point,
            confidence, calibrated, internal_only, source_core, regime_tag,
            params_hash
        ) VALUES (
            %(stock_id)s, %(forecast_date)s, %(horizon_days)s,
            %(lower)s, %(upper)s, %(point)s,
            %(confidence)s, %(calibrated)s, %(internal_only)s,
            %(source_core)s, %(regime_tag)s, %(params_hash)s
        )
        ON CONFLICT (stock_id, forecast_date, horizon_days, source_core, confidence)
        DO UPDATE SET
            lower         = EXCLUDED.lower,
            upper         = EXCLUDED.upper,
            point         = EXCLUDED.point,
            calibrated    = EXCLUDED.calibrated,
            internal_only = EXCLUDED.internal_only,
            regime_tag    = EXCLUDED.regime_tag,
            params_hash   = EXCLUDED.params_hash
    """
    payload = {
        "stock_id": row["stock_id"],
        "forecast_date": row["forecast_date"],
        "horizon_days": row["horizon_days"],
        "lower": row.get("lower"),
        "upper": row.get("upper"),
        "point": row.get("point"),
        "confidence": row["confidence"],
        "calibrated": bool(row.get("calibrated", False)),
        "internal_only": bool(row.get("internal_only", False)),
        "source_core": row["source_core"],
        "regime_tag": row.get("regime_tag"),
        "params_hash": row.get("params_hash"),
    }
    with conn.cursor() as cur:
        cur.execute(sql, payload)


def fetch_unresolved(
    conn,
    *,
    asof: date,
    source_core: str | None = None,
    stock_id: str | None = None,
    include_internal: bool = True,
) -> list[dict[str, Any]]:
    """Fetch forecast_log rows that are due for settlement.

    "Due" = resolved_date IS NULL AND forecast_date + horizon_days ≤ asof.
    The +horizon comparison uses INTERVAL arithmetic; rows whose nominal
    settlement day is in the past (or today) are returned.

    Args:
        include_internal: default True — settlement should resolve **all** rows
            including internal_only=True(否則對齊影子 row 永遠 unresolved 堆積)。
            scorer / display 走 fetch_resolved 預設 False,讓 internal_only 不
            leak 到對外面。
    """
    sql = """
        SELECT id, stock_id, forecast_date, horizon_days,
               lower, upper, point, confidence, calibrated, internal_only,
               source_core, regime_tag, params_hash
        FROM forecast_log
        WHERE resolved_date IS NULL
          AND forecast_date + (horizon_days * INTERVAL '1 day') <= %s
    """
    params: list[Any] = [asof]
    if not include_internal:
        sql += " AND internal_only = FALSE"
    if source_core is not None:
        sql += " AND source_core = %s"
        params.append(source_core)
    if stock_id is not None:
        sql += " AND stock_id = %s"
        params.append(stock_id)
    sql += " ORDER BY stock_id, forecast_date, horizon_days, source_core"
    with conn.cursor() as cur:
        cur.execute(sql, params)
        return list(cur.fetchall())


def fetch_resolved(
    conn,
    *,
    source_core: str | None = None,
    horizon_days: int | None = None,
    stock_id: str | None = None,
    since: date | None = None,
    include_internal: bool = False,
) -> list[dict[str, Any]]:
    """Fetch settled (scorable) forecast_log rows.

    "Settled" = resolved_date IS NOT NULL AND realized_price IS NOT NULL.

    Args:
        include_internal: default False — scorer / display / 對外路徑預設過濾
            internal_only=TRUE row(對齊 dual_track_resonance §七 B-4 機制丙)。
            audit / 內部對齊 explicitly 傳 True 才看到 neely_fib 對齊影子。
    """
    sql = """
        SELECT id, stock_id, forecast_date, horizon_days,
               lower, upper, point, confidence, calibrated, internal_only,
               source_core, regime_tag, params_hash, resolved_date,
               realized_price, hit, pinball_loss
        FROM forecast_log
        WHERE resolved_date IS NOT NULL AND realized_price IS NOT NULL
    """
    params: list[Any] = []
    if not include_internal:
        sql += " AND internal_only = FALSE"
    if source_core is not None:
        sql += " AND source_core = %s"
        params.append(source_core)
    if horizon_days is not None:
        sql += " AND horizon_days = %s"
        params.append(horizon_days)
    if stock_id is not None:
        sql += " AND stock_id = %s"
        params.append(stock_id)
    if since is not None:
        sql += " AND forecast_date >= %s"
        params.append(since)
    sql += " ORDER BY stock_id, forecast_date, horizon_days, source_core"
    with conn.cursor() as cur:
        cur.execute(sql, params)
        return list(cur.fetchall())


def update_settlement(
    conn,
    *,
    row_id: int,
    resolved_date: date,
    realized_price: float,
    hit: bool,
    pinball_loss: float,
) -> None:
    """Write settlement results for a single forecast_log row."""
    with conn.cursor() as cur:
        cur.execute(
            """UPDATE forecast_log
                  SET resolved_date  = %s,
                      realized_price = %s,
                      hit            = %s,
                      pinball_loss   = %s
                WHERE id = %s""",
            (resolved_date, realized_price, hit, pinball_loss, row_id),
        )
