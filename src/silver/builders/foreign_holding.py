"""
silver/builders/foreign_holding.py
==================================
foreign_investor_share_tw (Bronze) → foreign_holding_derived (Silver)。

最簡單一張(無衍生計算):
- 2 stored cols(1:1):foreign_holding_shares / foreign_holding_ratio
- detail JSONB 從 9 個 Bronze unpack 欄重 pack:
    remaining_shares / remain_ratio / upper_limit_ratio / cn_upper_limit /
    total_issued / declare_date / intl_code / stock_name / note

Round-trip:Silver 2 stored + detail JSONB 應與 v2.0 foreign_holding 等值。
"""

from __future__ import annotations

import logging
import time
from typing import Any

from .._common import fetch_bronze, upsert_silver


logger = logging.getLogger("collector.silver.builders.foreign_holding")


NAME          = "foreign_holding"
SILVER_TABLE  = "foreign_holding_derived"
BRONZE_TABLES = ["foreign_investor_share_tw"]

STORED_COLS = ("foreign_holding_shares", "foreign_holding_ratio")
DETAIL_KEYS = (
    "remaining_shares", "remain_ratio", "upper_limit_ratio",
    "cn_upper_limit", "total_issued", "declare_date",
    "intl_code", "stock_name", "note",
)


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
        s["detail"] = {k: row.get(k) for k in DETAIL_KEYS}
        out.append(s)
    return out


def run(
    db: Any,
    stock_ids: list[str] | None = None,
    full_rebuild: bool = False,
) -> dict[str, Any]:
    start = time.monotonic()

    bronze = fetch_bronze(db, "foreign_investor_share_tw", stock_ids=stock_ids)
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
