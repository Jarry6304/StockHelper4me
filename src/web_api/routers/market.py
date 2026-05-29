"""大盤 Golden 讀:climate_fusion(marketwide)。"""

from __future__ import annotations

from datetime import date
from typing import Any

from fastapi import APIRouter, Depends

from web_api import _passthrough as pt
from web_api.pool import get_pool

router = APIRouter(tags=["market"])


@router.get("/market/climate")
async def climate(as_of: date, pool: Any = Depends(get_pool)):
    """climate_fusion(stock_id=_market_,哨兵 tf _all_)。"""
    text = await pt.fetch_snapshot_text(
        pool, stock_id="_market_", as_of=as_of, core_name="climate_fusion", timeframe="_all_",
    )
    return pt.raw_json_response(text)
