"""個股 Golden 讀:neely forest / levels / resonance / 任一 core snapshot(sync handlers)。"""

from __future__ import annotations

from datetime import date
from typing import Any

from fastapi import APIRouter, Depends

from web_api import _passthrough as pt
from web_api.pool import get_pool

router = APIRouter(prefix="/stocks", tags=["stocks"])


@router.get("/{stock_id}/neely/forest")
def neely_forest(
    stock_id: str, as_of: date, timeframe: str = "daily", pool: Any = Depends(get_pool),
):
    """neely_core scenario_forest 完整 passthrough(N>250 → 422 完整性保險絲)。"""
    pt.guard_forest_cap(pool, stock_id=stock_id, as_of=as_of, timeframe=timeframe)
    text = pt.fetch_snapshot_text(
        pool, stock_id=stock_id, as_of=as_of, core_name="neely_core", timeframe=timeframe,
    )
    return pt.raw_json_response(text)


@router.get("/{stock_id}/levels")
def levels(stock_id: str, as_of: date, pool: Any = Depends(get_pool)):
    """levels_fusion(per-stock,哨兵 tf _all_)。"""
    text = pt.fetch_snapshot_text(
        pool, stock_id=stock_id, as_of=as_of, core_name="levels_fusion", timeframe="_all_",
    )
    return pt.raw_json_response(text)


@router.get("/{stock_id}/resonance")
def resonance(
    stock_id: str, as_of: date, timeframe: str = "daily", pool: Any = Depends(get_pool),
):
    """resonance_fusion(per-(stock, timeframe))。"""
    text = pt.fetch_snapshot_text(
        pool, stock_id=stock_id, as_of=as_of, core_name="resonance_fusion", timeframe=timeframe,
    )
    return pt.raw_json_response(text)


@router.get("/{stock_id}/snapshot/{core}")
def snapshot(
    stock_id: str, core: str, as_of: date,
    timeframe: str | None = None, pool: Any = Depends(get_pool),
):
    """generic passthrough:任一 core_name 的 structural_snapshots row。"""
    if core == "neely_core":
        pt.guard_forest_cap(
            pool, stock_id=stock_id, as_of=as_of, timeframe=timeframe or "daily",
        )
    text = pt.fetch_snapshot_text(
        pool, stock_id=stock_id, as_of=as_of, core_name=core, timeframe=timeframe,
    )
    return pt.raw_json_response(text)
