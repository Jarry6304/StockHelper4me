"""
silver/builders/holding_shares_per.py
=====================================
holding_shares_per (Bronze) → holding_shares_per_derived (Silver)。

Bronze 是 raw 1 row per (stock × date × HoldingSharesLevel),Silver pack 成
1 row per (stock × date) + detail JSONB packing 各 level 的 {people, percent, unit}。

對應 src/aggregators.py:aggregate_holding_shares 的正向 pack 邏輯;Bronze 已 PR #18.5
落地(smoke test 3 stocks 通過),PR #R3(alembic t9u0v1w2x3y4)後升格去 `_tw` 後綴。

Bronze schema:
  PK (market, stock_id, date, holding_shares_level)
  cols: people / percent / unit

Silver schema:
  PK (market, stock_id, date)
  cols: detail JSONB:{level_str: {people, percent, unit}, ...}
"""

from __future__ import annotations

import logging
import time
from collections import defaultdict
from typing import Any

from .._common import fetch_bronze, upsert_silver


logger = logging.getLogger("collector.silver.builders.holding_shares_per")


NAME          = "holding_shares_per"
SILVER_TABLE  = "holding_shares_per_derived"
BRONZE_TABLES = ["holding_shares_per"]


def _pack(bronze_rows: list[dict[str, Any]]) -> list[dict[str, Any]]:
    """Bronze 多 row → Silver 1 row,detail JSONB 收集 levels。"""
    grouped: dict[tuple, dict[str, Any]] = {}

    for row in bronze_rows:
        key = (row.get("market"), row.get("stock_id"), row.get("date"))
        if key not in grouped:
            grouped[key] = {
                "market":   row.get("market"),
                "stock_id": row.get("stock_id"),
                "date":     row.get("date"),
                "detail":   {},
            }

        level = row.get("holding_shares_level", "unknown")
        grouped[key]["detail"][level] = {
            "people":  row.get("people"),
            "percent": row.get("percent"),
            "unit":    row.get("unit"),
        }

    return list(grouped.values())


def run(
    db: Any,
    stock_ids: list[str] | None = None,
    full_rebuild: bool = False,
) -> dict[str, Any]:
    start = time.monotonic()

    bronze = fetch_bronze(
        db, "holding_shares_per",
        stock_ids=stock_ids,
        order_by="market, stock_id, date, holding_shares_level",
    )
    silver = _pack(bronze)
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
