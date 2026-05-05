"""
silver/builders/market_margin.py
================================
market_margin_maintenance (legacy v2.0) → market_margin_maintenance_derived (Silver)。

PK = (market, date)— 市場級表,無 stock_id 欄。

Silver 衍生欄(per spec §2.6.3):
  - total_margin_purchase_balance(整體市場融資餘額)
  - total_short_sale_balance(整體市場融券餘額)
這 2 欄需從 TaiwanStockTotalMarginPurchaseShortSale API 抓(目前 collector.toml
無該 entry)。PR #19c-1 階段:這 2 欄 = NULL,留 PR #19c-2 動工時新增 Bronze 來源
+ join 補。

Bronze 欄位:market / date / ratio
Silver 1:1 直拷 ratio + 2 衍生欄 NULL + dirty 欄。
"""

from __future__ import annotations

import logging
import time
from typing import Any

from .._common import fetch_bronze, upsert_silver


logger = logging.getLogger("collector.silver.builders.market_margin")


NAME          = "market_margin"
SILVER_TABLE  = "market_margin_maintenance_derived"
BRONZE_TABLES = ["market_margin_maintenance"]   # v3.2 後可能 rename


def _build_silver_rows(bronze_rows: list[dict[str, Any]]) -> list[dict[str, Any]]:
    out: list[dict[str, Any]] = []
    for row in bronze_rows:
        out.append({
            "market": row.get("market"),
            "date":   row.get("date"),
            "ratio":  row.get("ratio"),
            # 2 衍生欄 — PR #19c-2 接 TotalMarginPurchaseShortSale Bronze 後填
            "total_margin_purchase_balance": None,
            "total_short_sale_balance":      None,
        })
    return out


def run(
    db: Any,
    stock_ids: list[str] | None = None,
    full_rebuild: bool = False,
) -> dict[str, Any]:
    """注意 stock_ids 對 market-level 表無效,一律全讀。"""
    start = time.monotonic()

    bronze = fetch_bronze(db, "market_margin_maintenance", order_by="market, date")
    silver = _build_silver_rows(bronze)
    written = upsert_silver(
        db, SILVER_TABLE, silver,
        pk_cols=["market", "date"],
    )

    elapsed_ms = int((time.monotonic() - start) * 1000)
    logger.info(f"[{NAME}] read={len(bronze)} → wrote={written}({elapsed_ms}ms)")
    return {
        "name":         NAME,
        "rows_read":    len(bronze),
        "rows_written": written,
        "elapsed_ms":   elapsed_ms,
    }
