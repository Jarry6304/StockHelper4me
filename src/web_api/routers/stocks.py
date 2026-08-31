"""個股 Golden 讀:neely forest / levels / resonance / 任一 core snapshot(sync handlers)。"""

from __future__ import annotations

from datetime import date
from typing import Any

from fastapi import APIRouter, Depends

from web_api import _passthrough as pt
from web_api.pool import db_conn

router = APIRouter(prefix="/stocks", tags=["stocks"])


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


@router.get("/{stock_id}/traditional/forest")
def traditional_forest(
    stock_id: str, timeframe: str = "daily", conn: Any = Depends(db_conn),
):
    """traditional_core forest 完整 passthrough(獨立 vertical;latest per (stock, timeframe))。

    讀自有表 traditional_snapshots(非 structural_snapshots);與 /neely/forest 並排不整合。
    """
    text = pt.fetch_traditional_forest_text(conn, stock_id=stock_id, timeframe=timeframe)
    return pt.raw_json_response(text)


@router.get("/{stock_id}/waves")
def waves(
    stock_id: str, as_of: date, timeframe: str = "daily", conn: Any = Depends(db_conn),
):
    """邊緣組裝:`{ neely, traditional }` 並排呈現(**不合併、無 consensus**)。

    neely 取 as_of <= 最新 structural_snapshots;traditional 取 latest traditional_snapshots
    (該表無 snapshot_date,as_of 僅作用於 neely 側)。
    """
    neely_text = pt.fetch_snapshot_text(
        conn, stock_id=stock_id, as_of=as_of, core_name="neely_core", timeframe=timeframe,
    )
    trad_text = pt.fetch_traditional_forest_text(conn, stock_id=stock_id, timeframe=timeframe)
    # v4.39 additive(wave_judgment_loop §4):dossier 段(候選 anchor_key /
    # active judgment);current_price 前端自算,invalidation 機械面交 is_invalidated=false
    import json as _json

    from fusion.judgment import build_dossier

    try:
        dossier = build_dossier(conn, stock_id=stock_id, as_of=as_of, current_price=None)
        dossier_text = _json.dumps(dossier, ensure_ascii=False, default=str)
    except Exception:  # dossier 失敗不擋 raw 波浪(額外段,graceful)
        dossier_text = None
    return pt.compose_waves(neely_text, trad_text, dossier_text)
