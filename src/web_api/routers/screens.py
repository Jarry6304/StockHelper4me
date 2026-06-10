"""跨股排行榜:*_ranked_derived(重用既有 sync fetch_cross_stock_ranked)。sync handler。"""

from __future__ import annotations

from datetime import date
from typing import Any

from fastapi import APIRouter, Depends, HTTPException, Query
from fastapi.encoders import jsonable_encoder
from fastapi.responses import JSONResponse

from fusion.raw import ALLOWED_RANKED_TABLES, fetch_cross_stock_ranked
from web_api.pool import db_conn

router = APIRouter(prefix="/screens", tags=["screens"])

# 標準 (combined_rank, is_top_30) schema 的 ranked 表白名單(防 SQL injection)。
# wave_impulse_screen / monthly_trigger schema 不同 → 各有專屬 MCP 工具,不在此通用端口。
_ALLOWED: dict[str, str] = {
    "magic_formula": "magic_formula_ranked_derived",
    "persistent_momentum": "persistent_momentum_ranked_derived",
    "revenue_momentum": "revenue_momentum_ranked_derived",
    "institutional_concert": "institutional_concert_ranked_derived",
    "f_score": "f_score_ranked_derived",
    "low_volatility": "low_volatility_ranked_derived",
    "industry_adj_gp": "industry_adj_gp_ranked_derived",
    "long_term_low_vol": "long_term_low_vol_ranked_derived",
    "dividend_yield": "dividend_yield_ranked_derived",
    "mom_12_1": "mom_12_1_ranked_derived",
}
# 與 library 層權威白名單(fusion.raw)對齊,防兩處 drift(import-time 即炸)。
assert set(_ALLOWED.values()) <= ALLOWED_RANKED_TABLES, (
    "screens._ALLOWED 含 fusion.raw.ALLOWED_RANKED_TABLES 未涵蓋的表"
)


@router.get("/{toolkit}")
def screen(
    toolkit: str,
    as_of: date = Query(..., alias="date"),
    top_n: int = Query(30, ge=1, le=500),
    offset: int = Query(0, ge=0, le=100_000),
    conn: Any = Depends(db_conn),
):
    """某 toolkit 在 latest ranking_date <= date 的 top_n(offset 分頁)。"""
    table = _ALLOWED.get(toolkit)
    if table is None:
        raise HTTPException(
            status_code=404,
            detail=f"unknown screen toolkit '{toolkit}'. allowed: {sorted(_ALLOWED)}",
        )

    ranking_date, rows = fetch_cross_stock_ranked(
        conn, source_table=table, as_of=as_of, top_n=top_n + offset,
    )
    rows = rows[offset:offset + top_n]
    return JSONResponse(content=jsonable_encoder({
        "toolkit": toolkit, "ranking_date": ranking_date,
        "top_n": top_n, "offset": offset, "rows": rows,
    }))
