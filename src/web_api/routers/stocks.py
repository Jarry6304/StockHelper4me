"""個股 Golden 讀:neely forest / levels / resonance / 任一 core snapshot。"""

from __future__ import annotations

from datetime import date
from typing import Any

from fastapi import APIRouter, Depends

from web_api import _passthrough as pt
from web_api.pool import get_pool

router = APIRouter(prefix="/stocks", tags=["stocks"])


@router.get("/{stock_id}/neely/forest")
async def neely_forest(
    stock_id: str, as_of: date, timeframe: str = "daily", pool: Any = Depends(get_pool),
):
    """neely_core scenario_forest 完整 passthrough(N>250 → 422 完整性保險絲)。"""
    await pt.guard_forest_cap(pool, stock_id=stock_id, as_of=as_of, timeframe=timeframe)
    text = await pt.fetch_snapshot_text(
        pool, stock_id=stock_id, as_of=as_of, core_name="neely_core", timeframe=timeframe,
    )
    return pt.raw_json_response(text)


@router.get("/{stock_id}/levels")
async def levels(stock_id: str, as_of: date, pool: Any = Depends(get_pool)):
    """levels_fusion(per-stock,哨兵 tf _all_)。"""
    text = await pt.fetch_snapshot_text(
        pool, stock_id=stock_id, as_of=as_of, core_name="levels_fusion", timeframe="_all_",
    )
    return pt.raw_json_response(text)


@router.get("/{stock_id}/resonance")
async def resonance(
    stock_id: str, as_of: date, timeframe: str = "daily", pool: Any = Depends(get_pool),
):
    """resonance_fusion(per-(stock, timeframe))。"""
    text = await pt.fetch_snapshot_text(
        pool, stock_id=stock_id, as_of=as_of, core_name="resonance_fusion", timeframe=timeframe,
    )
    return pt.raw_json_response(text)


@router.get("/{stock_id}/snapshot/{core}")
async def snapshot(
    stock_id: str, core: str, as_of: date,
    timeframe: str | None = None, pool: Any = Depends(get_pool),
):
    """generic passthrough:任一 core_name 的 structural_snapshots row。"""
    if core == "neely_core":
        await pt.guard_forest_cap(
            pool, stock_id=stock_id, as_of=as_of, timeframe=timeframe or "daily",
        )
    text = await pt.fetch_snapshot_text(
        pool, stock_id=stock_id, as_of=as_of, core_name=core, timeframe=timeframe,
    )
    return pt.raw_json_response(text)
