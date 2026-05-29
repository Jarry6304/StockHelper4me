"""時序切片:price OHLCV(price_daily_fwd)+ Kalman series(indicator_values)。"""

from __future__ import annotations

from datetime import date
from typing import Any

from fastapi import APIRouter, Depends, Query
from fastapi.encoders import jsonable_encoder
from fastapi.responses import JSONResponse

from web_api import _passthrough as pt
from web_api.pool import get_pool

router = APIRouter(prefix="/stocks", tags=["series"])


@router.get("/{stock_id}/ohlc")
async def ohlc(
    stock_id: str,
    from_: date = Query(..., alias="from"),
    to: date = Query(...),
    pool: Any = Depends(get_pool),
):
    """後復權 OHLCV 切片(price_daily_fwd,ORDER BY date ASC)。"""
    sql = (
        "SELECT date, open, high, low, close, volume FROM price_daily_fwd "
        "WHERE market = 'TW' AND stock_id = %s AND date BETWEEN %s AND %s "
        "ORDER BY date ASC"
    )
    async with pool.connection() as conn:
        async with conn.cursor() as cur:
            await cur.execute(sql, [stock_id, from_, to])
            rows = await cur.fetchall()
    return JSONResponse(content=jsonable_encoder({"stock_id": stock_id, "rows": rows}))


@router.get("/{stock_id}/kalman/series")
async def kalman_series(
    stock_id: str, as_of: date, timeframe: str = "daily", pool: Any = Depends(get_pool),
):
    """Kalman 最新 indicator value 原文(含 multi-horizon series),snapshot_date <= as_of。"""
    sql = (
        "SELECT value::text AS j FROM indicator_values "
        "WHERE stock_id = %s AND source_core = 'kalman_filter_core' "
        "  AND value_date <= %s AND timeframe = %s "
        "ORDER BY value_date DESC LIMIT 1"
    )
    async with pool.connection() as conn:
        async with conn.cursor() as cur:
            await cur.execute(sql, [stock_id, as_of, timeframe])
            row = await cur.fetchone()
    return pt.raw_json_response(row["j"] if row else None)
