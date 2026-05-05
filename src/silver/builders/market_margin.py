"""
silver/builders/market_margin.py
================================
market_margin_maintenance + total_margin_purchase_short_sale_tw (Bronze) →
                            market_margin_maintenance_derived (Silver)。

PK = (market, date)— 市場級表,無 stock_id 欄。

Silver 衍生欄(per spec §2.6.3):
  - total_margin_purchase_balance(整體市場融資餘額)
  - total_short_sale_balance(整體市場融券餘額)
PR #21-B 落地:從 total_margin_purchase_short_sale_tw Bronze
(FinMind dataset TaiwanStockTotalMarginPurchaseShortSale)讀取後 LEFT JOIN
by (market, date) 補進 Silver。Bronze 缺對應 (market, date) → 兩欄 NULL。

Bronze 欄位:market / date / ratio
Silver 1:1 直拷 ratio + 2 衍生欄(LEFT JOIN total_margin Bronze)+ dirty 欄。
"""

from __future__ import annotations

import logging
import time
from typing import Any

from .._common import fetch_bronze, upsert_silver


logger = logging.getLogger("collector.silver.builders.market_margin")


NAME          = "market_margin"
SILVER_TABLE  = "market_margin_maintenance_derived"
BRONZE_TABLES = ["market_margin_maintenance", "total_margin_purchase_short_sale_tw"]


def _build_total_margin_lookup(
    bronze_rows: list[dict[str, Any]],
) -> dict[tuple, dict[str, Any]]:
    """{(market, date): {total_margin_purchase_balance, total_short_sale_balance}}。"""
    out: dict[tuple, dict[str, Any]] = {}
    for row in bronze_rows:
        key = (row.get("market"), row.get("date"))
        out[key] = {
            "total_margin_purchase_balance": row.get("total_margin_purchase_balance"),
            "total_short_sale_balance":      row.get("total_short_sale_balance"),
        }
    return out


def _build_silver_rows(
    bronze_rows: list[dict[str, Any]],
    total_margin_lookup: dict[tuple, dict[str, Any]],
) -> list[dict[str, Any]]:
    out: list[dict[str, Any]] = []
    for row in bronze_rows:
        key = (row.get("market"), row.get("date"))
        tm = total_margin_lookup.get(key, {})
        out.append({
            "market": row.get("market"),
            "date":   row.get("date"),
            "ratio":  row.get("ratio"),
            "total_margin_purchase_balance": tm.get("total_margin_purchase_balance"),
            "total_short_sale_balance":      tm.get("total_short_sale_balance"),
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
    total_margin = fetch_bronze(
        db, "total_margin_purchase_short_sale_tw", order_by="market, date",
    )
    total_margin_lookup = _build_total_margin_lookup(total_margin)

    silver = _build_silver_rows(bronze, total_margin_lookup)
    written = upsert_silver(
        db, SILVER_TABLE, silver,
        pk_cols=["market", "date"],
    )

    elapsed_ms = int((time.monotonic() - start) * 1000)
    logger.info(
        f"[{NAME}] read={len(bronze)} margin + {len(total_margin)} total_margin → "
        f"wrote={written}({elapsed_ms}ms)"
    )
    return {
        "name":         NAME,
        "rows_read":    len(bronze),
        "rows_written": written,
        "elapsed_ms":   elapsed_ms,
    }
