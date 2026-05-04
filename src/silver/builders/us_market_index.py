"""
silver/builders/us_market_index.py
==================================
market_index_us (legacy v2.0) → us_market_index_derived (Silver)。

注意:Bronze 來源是 v2.0 `market_index_us`(SPY / ^VIX),v3.2 計畫 rename 為
`us_market_index_tw` 但尚未實際 rename(schema_pg.sql line 454 確認)。
動工時若已 rename,把 BRONZE_TABLES 改 `us_market_index_tw` 即可。

Bronze 欄位:market / stock_id (SPY | ^VIX) / date / OHLCV / detail
Silver 1:1 直拷 + 自動補 dirty 欄。
"""

from __future__ import annotations

import logging
import time
from typing import Any

from .._common import fetch_bronze, upsert_silver


logger = logging.getLogger("collector.silver.builders.us_market_index")


NAME          = "us_market_index"
SILVER_TABLE  = "us_market_index_derived"
BRONZE_TABLES = ["market_index_us"]   # v3.2 後 rename 為 us_market_index_tw

STORED_COLS = ("open", "high", "low", "close", "volume", "detail")


def _build_silver_rows(bronze_rows: list[dict[str, Any]]) -> list[dict[str, Any]]:
    out: list[dict[str, Any]] = []
    for row in bronze_rows:
        s: dict[str, Any] = {
            "market":   row.get("market"),
            "stock_id": row.get("stock_id"),
            "date":     row.get("date"),
        }
        for c in STORED_COLS:
            s[c] = row.get(c)
        out.append(s)
    return out


def run(
    db: Any,
    stock_ids: list[str] | None = None,
    full_rebuild: bool = False,
) -> dict[str, Any]:
    """注意 stock_ids 對 market-level 表(stock_id ∈ {SPY, ^VIX})無意義,一律全讀。"""
    start = time.monotonic()

    # 不 forward stock_ids — market_index_us 是美股指數表(SPY / ^VIX sentinel)
    bronze = fetch_bronze(db, "market_index_us")
    silver = _build_silver_rows(bronze)
    written = upsert_silver(
        db, SILVER_TABLE, silver,
        pk_cols=["market", "stock_id", "date"],
    )

    elapsed_ms = int((time.monotonic() - start) * 1000)
    logger.info(f"[{NAME}] read={len(bronze)} → wrote={written}({elapsed_ms}ms)")
    return {
        "name":         NAME,
        "rows_read":    len(bronze),
        "rows_written": written,
        "elapsed_ms":   elapsed_ms,
    }
