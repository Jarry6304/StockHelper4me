"""
silver/builders/valuation.py
============================
valuation_per_tw (Bronze) → valuation_daily_derived (Silver)。

PR #19b 階段:3 stored cols 1:1 直拷,market_value_weight = NULL(留 PR #19c 動工
時 join price_daily + stock_info_ref 的發行股數計算)。

- 3 stored cols(1:1):per / pbr / dividend_yield
- 無 detail JSONB
- market_value_weight(per spec §2.6.4):個股佔大盤市值比重
   = stock_value / SUM(market_value);PR #19c 補

Round-trip:Silver 3 stored 應與 v2.0 valuation_daily 等值。
"""

from __future__ import annotations

import logging
import time
from typing import Any

from .._common import fetch_bronze, upsert_silver


logger = logging.getLogger("collector.silver.builders.valuation")


NAME          = "valuation"
SILVER_TABLE  = "valuation_daily_derived"
BRONZE_TABLES = ["valuation_per_tw"]

STORED_COLS = ("per", "pbr", "dividend_yield")


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
        s["market_value_weight"] = None  # PR #19c 補
        out.append(s)
    return out


def run(
    db: Any,
    stock_ids: list[str] | None = None,
    full_rebuild: bool = False,
) -> dict[str, Any]:
    start = time.monotonic()

    bronze = fetch_bronze(db, "valuation_per_tw", stock_ids=stock_ids)
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
