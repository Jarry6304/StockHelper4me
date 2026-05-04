"""
silver/builders/exchange_rate.py
================================
exchange_rate (legacy v2.0) → exchange_rate_derived (Silver)。

注意 PK 含 currency 維度 (market, date, currency) — 不是 (market, stock_id, date)。
Bronze 來源是 v2.0 `exchange_rate`,v3.2 計畫 rename 為 `exchange_rate_tw` 但尚未
rename(schema_pg.sql line 470)。

stock_ids 參數對 market-level 表(無 stock_id 欄)無意義,builder 一律讀全表。
若 user 透過 CLI 帶 --stocks,orchestrator 應對 market-level builder 略過該過濾。
"""

from __future__ import annotations

import logging
import time
from typing import Any

from .._common import fetch_bronze, upsert_silver


logger = logging.getLogger("collector.silver.builders.exchange_rate")


NAME          = "exchange_rate"
SILVER_TABLE  = "exchange_rate_derived"
BRONZE_TABLES = ["exchange_rate"]   # v3.2 後可能 rename 為 exchange_rate_tw

STORED_COLS = ("rate", "detail")


def _build_silver_rows(bronze_rows: list[dict[str, Any]]) -> list[dict[str, Any]]:
    out: list[dict[str, Any]] = []
    for row in bronze_rows:
        s: dict[str, Any] = {
            "market":   row.get("market"),
            "date":     row.get("date"),
            "currency": row.get("currency"),
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
    """注意 stock_ids 對 market-level 表無效,一律全讀。"""
    start = time.monotonic()

    # 不傳 stock_ids 過濾(exchange_rate 沒有 stock_id 欄);order_by 對齊 PK
    bronze = fetch_bronze(db, "exchange_rate", order_by="market, date, currency")
    silver = _build_silver_rows(bronze)
    written = upsert_silver(
        db, SILVER_TABLE, silver,
        pk_cols=["market", "date", "currency"],
    )

    elapsed_ms = int((time.monotonic() - start) * 1000)
    logger.info(f"[{NAME}] read={len(bronze)} → wrote={written}({elapsed_ms}ms)")
    return {
        "name":         NAME,
        "rows_read":    len(bronze),
        "rows_written": written,
        "elapsed_ms":   elapsed_ms,
    }
