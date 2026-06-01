"""跨股排行榜:*_ranked_derived(重用既有 sync fetch_cross_stock_ranked)。sync handler。"""

from __future__ import annotations

from datetime import date
from typing import Any

from fastapi import APIRouter, Depends, HTTPException, Query
from fastapi.encoders import jsonable_encoder
from fastapi.responses import JSONResponse

from web_api.pool import db_conn

router = APIRouter(prefix="/screens", tags=["screens"])

# 白名單(防 SQL injection):toolkit → (table, rank_col)。
# rank 欄 per-toolkit(各 builder 自己定:magic_formula=combined_rank,其餘各自命名);
# top 旗標 canonical 統一 is_top_n(v4.35 對齊後 12 個 ranked 表一致)。
# wave_impulse_screen / monthly_trigger schema 不同 → 各有專屬 MCP 工具,不在此通用端口。
_ALLOWED: dict[str, tuple[str, str]] = {
    "magic_formula":         ("magic_formula_ranked_derived",         "combined_rank"),
    "persistent_momentum":   ("persistent_momentum_ranked_derived",   "momentum_rank"),
    "revenue_momentum":      ("revenue_momentum_ranked_derived",      "revenue_rank"),
    "institutional_concert": ("institutional_concert_ranked_derived", "concert_rank"),
    "f_score":               ("f_score_ranked_derived",               "score_rank"),
    "low_volatility":        ("low_volatility_ranked_derived",        "vol_rank"),
    "industry_adj_gp":       ("industry_adj_gp_ranked_derived",       "gp_rank"),
    "long_term_low_vol":     ("long_term_low_vol_ranked_derived",     "vol_rank"),
    "dividend_yield":        ("dividend_yield_ranked_derived",        "yield_rank"),
    "mom_12_1":              ("mom_12_1_ranked_derived",              "mom_rank"),
}


@router.get("/{toolkit}")
def screen(
    toolkit: str,
    as_of: date = Query(..., alias="date"),
    top_n: int = 30,
    offset: int = 0,
    conn: Any = Depends(db_conn),
):
    """某 toolkit 在 latest ranking_date <= date 的 top_n(offset 分頁)。"""
    entry = _ALLOWED.get(toolkit)
    if entry is None:
        raise HTTPException(
            status_code=404,
            detail=f"unknown screen toolkit '{toolkit}'. allowed: {sorted(_ALLOWED)}",
        )
    table, rank_col = entry

    from fusion.raw._db import fetch_cross_stock_ranked

    ranking_date, rows = fetch_cross_stock_ranked(
        conn, source_table=table, as_of=as_of, rank_col=rank_col, top_n=top_n + offset,
    )
    rows = rows[offset:offset + top_n]
    return JSONResponse(content=jsonable_encoder({
        "toolkit": toolkit, "ranking_date": ranking_date,
        "top_n": top_n, "offset": offset, "rows": rows,
    }))
