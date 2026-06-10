"""時序切片:price OHLCV(price_daily_fwd)+ Kalman series(indicator_values)。sync handlers。"""

from __future__ import annotations

from datetime import date
from typing import Any

from fastapi import APIRouter, Depends, Query
from fastapi.encoders import jsonable_encoder
from fastapi.responses import JSONResponse

from web_api import _passthrough as pt
from web_api.pool import db_conn

router = APIRouter(prefix="/stocks", tags=["series"])


@router.get("/{stock_id}/ohlc")
def ohlc(
    stock_id: str,
    from_: date = Query(..., alias="from"),
    to: date = Query(...),
    conn: Any = Depends(db_conn),
):
    """後復權 OHLCV 切片(price_daily_fwd,ORDER BY date ASC)。

    過濾條件鏡射引擎 loader(cores_shared/ohlcv_loader::load_daily):
    is_dirty=FALSE + OHLC 非 NULL,另加 close > 0(FinMind 無成交日的 0 值列,
    上圖會把線打到 0);不過濾 market(loader 同款 — 等值過濾曾因值域不一致
    造成切片整段缺列)。
    """
    sql = (
        "SELECT date, open, high, low, close, volume FROM price_daily_fwd "
        "WHERE stock_id = %s AND is_dirty = FALSE "
        "  AND date BETWEEN %s AND %s "
        "  AND open IS NOT NULL AND high IS NOT NULL "
        "  AND low IS NOT NULL AND close IS NOT NULL AND close > 0 "
        "ORDER BY date ASC"
    )
    with conn.cursor() as cur:
        cur.execute(sql, [stock_id, from_, to])
        rows = cur.fetchall()
    return JSONResponse(content=jsonable_encoder({"stock_id": stock_id, "rows": rows}))


@router.get("/{stock_id}/kalman/series")
def kalman_series(
    stock_id: str, as_of: date, timeframe: str = "daily", conn: Any = Depends(db_conn),
):
    """Kalman 最新 indicator value 原文(含 multi-horizon series),value_date <= as_of。"""
    sql = (
        "SELECT value::text AS j FROM indicator_values "
        "WHERE stock_id = %s AND source_core = 'kalman_filter_core' "
        "  AND value_date <= %s AND timeframe = %s "
        "ORDER BY value_date DESC LIMIT 1"
    )
    with conn.cursor() as cur:
        cur.execute(sql, [stock_id, as_of, timeframe])
        row = cur.fetchone()
    return pt.raw_json_response(row["j"] if row else None)
