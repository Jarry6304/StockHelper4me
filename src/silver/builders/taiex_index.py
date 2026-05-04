"""
silver/builders/taiex_index.py
==============================
market_ohlcv_tw (Bronze) → taiex_index_derived (Silver)。

對應 tw_market_core 的 TAIEX / TPEx 大盤指數,Bronze 已 PR #11 落地(B-1/B-2)。
PR #19c-1 階段:OHLCV 1:1 直拷 + detail JSONB 直拷;market_index_tw 雙來源 merge
(TotalReturnIndex 補 close)的整合留 PR #19c-2 視必要動工(目前 market_ohlcv_tw
本身已含 close,簡單 case 不需 merge)。

Bronze 欄位:market / stock_id (TAIEX | TPEx) / date / open / high / low / close /
            volume / detail
Silver 加:is_dirty / dirty_at(upsert_silver 自動補 default)
"""

from __future__ import annotations

import logging
import time
from typing import Any

from .._common import fetch_bronze, upsert_silver


logger = logging.getLogger("collector.silver.builders.taiex_index")


NAME          = "taiex_index"
SILVER_TABLE  = "taiex_index_derived"
BRONZE_TABLES = ["market_ohlcv_tw"]

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
    start = time.monotonic()

    bronze = fetch_bronze(db, "market_ohlcv_tw", stock_ids=stock_ids)
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
