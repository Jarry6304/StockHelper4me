"""大盤 Golden 讀:climate_fusion(marketwide,sync handler)。"""

from __future__ import annotations

from datetime import date
from typing import Any

from fastapi import APIRouter, Depends

from web_api import _passthrough as pt
from web_api.pool import db_conn

router = APIRouter(tags=["market"])


@router.get("/market/climate")
def climate(as_of: date, conn: Any = Depends(db_conn)):
    """climate_fusion(stock_id=_market_,哨兵 tf _all_)。"""
    text = pt.fetch_snapshot_text(
        conn, stock_id="_market_", as_of=as_of, core_name="climate_fusion", timeframe="_all_",
    )
    return pt.raw_json_response(text)
