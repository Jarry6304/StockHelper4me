"""個股 Golden 讀:neely forest / levels / resonance / 任一 core snapshot(sync handlers)。

#7 加 `GET /stocks?q=` 入口:autocomplete 搜尋 stock_id prefix / stock_name substring。
"""

from __future__ import annotations

from datetime import date
from typing import Any

from fastapi import APIRouter, Depends, HTTPException, Query
from fastapi.encoders import jsonable_encoder
from fastapi.responses import JSONResponse

from fusion.raw._db import search_stocks
from web_api import _passthrough as pt
from web_api.pool import db_conn

router = APIRouter(prefix="/stocks", tags=["stocks"])


# #7:個股入口 — `GET /stocks?q=2330` 或 `?q=台積`(prefix on stock_id + substring on
# stock_name);走 fusion.raw._db.search_stocks。回 `StockRef[]`(契約 web_api.contracts)。
# 為避免被下方 `/{stock_id}/...` 模式吞掉,先註冊本路由(FastAPI 走聲明順序)。
@router.get("")
def search(
    q: str = Query(..., min_length=1, max_length=64, description="prefix match on stock_id 或 substring on stock_name"),
    limit: int = Query(20, ge=1, le=100),
    conn: Any = Depends(db_conn),
):
    """個股搜尋:`?q=` 必填。空白字串 → 422(FastAPI Query 驗證)。"""
    q_clean = q.strip()
    if not q_clean:
        raise HTTPException(status_code=422, detail="q must be non-empty after strip")
    rows = search_stocks(conn, q=q_clean, limit=limit)
    return JSONResponse(content=jsonable_encoder(rows))


@router.get("/{stock_id}/neely/forest")
def neely_forest(
    stock_id: str, as_of: date, timeframe: str = "daily", conn: Any = Depends(db_conn),
):
    """neely_core scenario_forest 完整 passthrough(N>250 → 422 完整性保險絲)。"""
    pt.guard_forest_cap(conn, stock_id=stock_id, as_of=as_of, timeframe=timeframe)
    text = pt.fetch_snapshot_text(
        conn, stock_id=stock_id, as_of=as_of, core_name="neely_core", timeframe=timeframe,
    )
    return pt.raw_json_response(text)


@router.get("/{stock_id}/levels")
def levels(stock_id: str, as_of: date, conn: Any = Depends(db_conn)):
    """levels_fusion(per-stock,哨兵 tf _all_)。"""
    text = pt.fetch_snapshot_text(
        conn, stock_id=stock_id, as_of=as_of, core_name="levels_fusion", timeframe="_all_",
    )
    return pt.raw_json_response(text)


@router.get("/{stock_id}/resonance")
def resonance(
    stock_id: str, as_of: date, timeframe: str = "daily", conn: Any = Depends(db_conn),
):
    """resonance_fusion(per-(stock, timeframe))。"""
    text = pt.fetch_snapshot_text(
        conn, stock_id=stock_id, as_of=as_of, core_name="resonance_fusion", timeframe=timeframe,
    )
    return pt.raw_json_response(text)


@router.get("/{stock_id}/snapshot/{core}")
def snapshot(
    stock_id: str, core: str, as_of: date,
    timeframe: str | None = None, conn: Any = Depends(db_conn),
):
    """generic passthrough:任一 core_name 的 structural_snapshots row。"""
    if core == "neely_core":
        pt.guard_forest_cap(
            conn, stock_id=stock_id, as_of=as_of, timeframe=timeframe or "daily",
        )
    text = pt.fetch_snapshot_text(
        conn, stock_id=stock_id, as_of=as_of, core_name=core, timeframe=timeframe,
    )
    return pt.raw_json_response(text)
