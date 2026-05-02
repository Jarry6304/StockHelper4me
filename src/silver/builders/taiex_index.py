"""
silver/builders/taiex_index.py
==============================
market_ohlcv_tw (Bronze) → taiex_index_derived (Silver)。

對應 tw_market_core 的 TAIEX / TPEx 大盤指數;Bronze 已 PR #11 落地(B-1/B-2)。

留 **PR #19c** 動工。OHLCV 1:1 直拷,可能需 join market_index_tw 補 TotalReturnIndex
的 close 對齊(per blueprint §四「TaiwanStockTotalReturnIndex(close)+
TaiwanVariousIndicators5Seconds」雙來源 merge)。
"""

from __future__ import annotations

from typing import Any


NAME          = "taiex_index"
SILVER_TABLE  = "taiex_index_derived"
BRONZE_TABLES = ["market_ohlcv_tw"]


def run(
    db: Any,
    stock_ids: list[str] | None = None,
    full_rebuild: bool = False,
) -> dict[str, Any]:
    raise NotImplementedError(
        f"{NAME} builder 留 PR #19c 動工。market_ohlcv_tw → taiex_index_derived 1:1。"
    )
