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
    "upsert_forecast_batch",
    "update_settlement",
    "update_settlement_batch",
    "fetch_unresolved",
    "fetch_resolved",
]

# 批次 UNNEST chunk 大小(對齊 Rust writers.rs:write_forecast_log BATCH_SIZE=4000)。
_FORECAST_BATCH_SIZE = 4000


def upsert_forecast(conn, row: dict[str, Any]) -> None:
    """Upsert a single forecast row into forecast_log.

    Required keys:
        stock_id, forecast_date, horizon_days, confidence, source_core
    Optional keys:
        lower, upper, point, calibrated (default False),
        internal_only (default False — 對齊 dual_track_resonance §七 B-4 機制丙;
            neely_fib emitter 傳 True,其他 emitter 維持 False),
        regime_tag, params_hash,
        logic_version (B1 default 'b1' — backtest segmentation;**不進** ON CONFLICT
            唯一鍵;已 settle row(resolved_date IS NOT NULL)永不被覆寫)

    Settlement columns (resolved_date, realized_price, hit, pinball_loss) are
    not touched by this function — see `settlement.resolve_pending`.
    """
    sql = """
        INSERT INTO forecast_log (
            stock_id, forecast_date, horizon_days, lower, upper, point,
            confidence, calibrated, internal_only, source_core, regime_tag,
            params_hash, logic_version
        ) VALUES (
            %(stock_id)s, %(forecast_date)s, %(horizon_days)s,
            %(lower)s, %(upper)s, %(point)s,
            %(confidence)s, %(calibrated)s, %(internal_only)s,
            %(source_core)s, %(regime_tag)s, %(params_hash)s,
            %(logic_version)s
        )
        ON CONFLICT (stock_id, forecast_date, horizon_days, source_core, confidence)
        DO UPDATE SET
            lower         = EXCLUDED.lower,
            upper         = EXCLUDED.upper,
            point         = EXCLUDED.point,
            calibrated    = EXCLUDED.calibrated,
            internal_only = EXCLUDED.internal_only,
            regime_tag    = EXCLUDED.regime_tag,
            params_hash   = EXCLUDED.params_hash,
            -- B1 idempotent guard:已 settle row(resolved_date IS NOT NULL)永不被
            -- 覆寫 logic_version,確保 backtest 證據隨 settlement 凍結。
            logic_version = CASE
                WHEN forecast_log.resolved_date IS NULL THEN EXCLUDED.logic_version
                ELSE forecast_log.logic_version
            END
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
        # B1:新寫入預設 'b1';caller 可覆寫(歷史 backfill / replay 等)
        "logic_version": row.get("logic_version", "b1"),
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


# ─── 批次寫入(對標 Rust writers.rs UNNEST batch)──────────────────────────────
#
# 動機(forecast 校準流水線批次優化):conformalize / settle 原本逐筆 upsert/update,
# 在 autocommit=True 連線下每筆各一次 commit→fsync,且 N+1 round-trip → recalibrate
# 每批 ~1hr。批次版用 UNNEST 一次寫多列;caller 負責用 `with conn.transaction():`
# 把整塊包成單一 transaction(autocommit 下仍發顯式 BEGIN/COMMIT)→ 每塊 1 次 fsync。
#
# **語意與單筆版位元一致**:per-row 預設(calibrated/internal_only False、
# logic_version 'b1')+ ON CONFLICT 子句(含 B1 logic_version CASE guard)逐字相同。


def _forecast_batch_columns(rows: list[dict[str, Any]]) -> list[list[Any]]:
    """把 row dict list 攤成 13 條 parallel column lists(對齊 upsert_forecast payload
    的 per-row 預設)。"""
    return [
        [r["stock_id"] for r in rows],
        [r["forecast_date"] for r in rows],
        [r["horizon_days"] for r in rows],
        [r.get("lower") for r in rows],
        [r.get("upper") for r in rows],
        [r.get("point") for r in rows],
        [r["confidence"] for r in rows],
        [bool(r.get("calibrated", False)) for r in rows],
        [bool(r.get("internal_only", False)) for r in rows],
        [r["source_core"] for r in rows],
        [r.get("regime_tag") for r in rows],
        [r.get("params_hash") for r in rows],
        [r.get("logic_version", "b1") for r in rows],
    ]


# UNNEST 批次 upsert SQL。ON CONFLICT 子句逐字對齊單筆 upsert_forecast
# (含 B1 logic_version CASE guard:已 settle row 不被覆寫 logic_version)。
_UPSERT_FORECAST_BATCH_SQL = """
    INSERT INTO forecast_log (
        stock_id, forecast_date, horizon_days, lower, upper, point,
        confidence, calibrated, internal_only, source_core, regime_tag,
        params_hash, logic_version
    )
    SELECT * FROM UNNEST(
        %s::text[], %s::date[], %s::smallint[],
        %s::numeric[], %s::numeric[], %s::numeric[],
        %s::numeric[], %s::bool[], %s::bool[],
        %s::text[], %s::text[], %s::text[], %s::text[]
    )
    ON CONFLICT (stock_id, forecast_date, horizon_days, source_core, confidence)
    DO UPDATE SET
        lower         = EXCLUDED.lower,
        upper         = EXCLUDED.upper,
        point         = EXCLUDED.point,
        calibrated    = EXCLUDED.calibrated,
        internal_only = EXCLUDED.internal_only,
        regime_tag    = EXCLUDED.regime_tag,
        params_hash   = EXCLUDED.params_hash,
        logic_version = CASE
            WHEN forecast_log.resolved_date IS NULL THEN EXCLUDED.logic_version
            ELSE forecast_log.logic_version
        END
"""


def upsert_forecast_batch(conn, rows: list[dict[str, Any]]) -> int:
    """批次 upsert 多筆 forecast row(UNNEST,語意同 upsert_forecast)。

    Caller 負責 transaction(本函式不 commit;在 autocommit=True 下每 execute 自動
    commit,故請以 `with conn.transaction():` 包整塊降 fsync 次數)。

    Returns: 處理的 row 數(送入的列數)。
    """
    if not rows:
        return 0
    total = 0
    with conn.cursor() as cur:
        for i in range(0, len(rows), _FORECAST_BATCH_SIZE):
            chunk = rows[i : i + _FORECAST_BATCH_SIZE]
            cur.execute(_UPSERT_FORECAST_BATCH_SQL, _forecast_batch_columns(chunk))
            total += len(chunk)
    return total


_UPDATE_SETTLEMENT_BATCH_SQL = """
    UPDATE forecast_log AS f
       SET resolved_date  = u.resolved_date,
           realized_price = u.realized_price,
           hit            = u.hit,
           pinball_loss   = u.pinball_loss
      FROM (
        SELECT * FROM UNNEST(
            %s::bigint[], %s::date[], %s::numeric[], %s::bool[], %s::numeric[]
        ) AS t(row_id, resolved_date, realized_price, hit, pinball_loss)
      ) AS u
     WHERE f.id = u.row_id
"""


def update_settlement_batch(conn, updates: list[dict[str, Any]]) -> int:
    """批次寫結算欄位(UPDATE ... FROM UNNEST,語意同 update_settlement)。

    每個 update dict 需含:row_id, resolved_date, realized_price, hit, pinball_loss。
    Caller 負責 transaction(同 upsert_forecast_batch)。

    Returns: 處理的 row 數。
    """
    if not updates:
        return 0
    total = 0
    with conn.cursor() as cur:
        for i in range(0, len(updates), _FORECAST_BATCH_SIZE):
            chunk = updates[i : i + _FORECAST_BATCH_SIZE]
            cur.execute(
                _UPDATE_SETTLEMENT_BATCH_SQL,
                [
                    [u["row_id"] for u in chunk],
                    [u["resolved_date"] for u in chunk],
                    [u["realized_price"] for u in chunk],
                    [bool(u["hit"]) for u in chunk],
                    [u["pinball_loss"] for u in chunk],
                ],
            )
            total += len(chunk)
    return total
