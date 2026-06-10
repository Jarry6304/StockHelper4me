"""V2 跨股表 WAVE 欄批次摘要端點(sync handler;拍版 (a),v2-wave-endpoint)。

`GET /waves/summary?stock_ids=2330,1101&date=…&timeframe=daily`
一次回整頁(~30 檔)的 wave digest — 伺服端抽取,不送 forest(對齊 spec CL5
summary-only)。抽取邏輯單一真相源在 src/fusion/wave_summary.py。
"""

from __future__ import annotations

from datetime import date
from typing import Any

from fastapi import APIRouter, Depends, HTTPException, Query
from fastapi.encoders import jsonable_encoder
from fastapi.responses import JSONResponse

from fusion.wave_summary import wave_summary_rows
from web_api.pool import db_conn

router = APIRouter(prefix="/waves", tags=["waves"])

# 防 runaway query 上界(對齊 screens top_n le=500 的顯式 422 風格;一頁 30 檔,
# 100 留 headroom)
_MAX_STOCK_IDS = 100
_VALID_TIMEFRAMES = ("daily", "weekly", "monthly")


@router.get("/summary")
def waves_summary(
    stock_ids: str = Query(..., description="逗號分隔股票清單(例 2330,1101)"),
    as_of: date = Query(..., alias="date"),
    timeframe: str = Query("daily"),
    conn: Any = Depends(db_conn),
):
    """批次 WAVE 摘要:每檔 {label, direction, certainty, scenario_count, sparkline,
    resonance, staleness_days};無資料 / 引擎無法判斷 → insufficient=true(不 404)。"""
    ids = [s.strip() for s in stock_ids.split(",") if s.strip()]
    if not ids:
        raise HTTPException(status_code=422, detail="stock_ids 不可為空")
    if len(ids) > _MAX_STOCK_IDS:
        raise HTTPException(
            status_code=422,
            detail=f"stock_ids 超過上限 {_MAX_STOCK_IDS}(收到 {len(ids)})",
        )
    if timeframe not in _VALID_TIMEFRAMES:
        raise HTTPException(
            status_code=422,
            detail=f"timeframe {timeframe!r} 不在 {list(_VALID_TIMEFRAMES)}",
        )

    rows = wave_summary_rows(conn, ids, as_of, timeframe=timeframe)
    return JSONResponse(content=jsonable_encoder({
        "as_of": as_of.isoformat(), "timeframe": timeframe, "rows": rows,
    }))
