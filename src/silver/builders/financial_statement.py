"""
silver/builders/financial_statement.py
======================================
financial_statement (Bronze) → financial_statement_derived (Silver)。

對應 src/aggregators.py:aggregate_financial 的正向 pack:Bronze 1 row per
(stock × date × event_type × origin_name)→ Silver 1 row per (stock × date × type)
with detail JSONB packing {origin_name: value, ...}。

欄名 rename:
  Bronze.event_type → Silver.type(income / balance / cashflow)
  Bronze.value × N rows → Silver.detail JSONB 集合

Bronze 已 PR #18.5 落地(smoke test 3 stocks 通過,3 個財報 dataset 統一進
financial_statement 用 event_type 區分);PR #R3(alembic t9u0v1w2x3y4)後升格去 `_tw` 後綴。

7b 階段(orchestrator 動工後)需 monthly_revenue_derived 對齊財報季度日期映射 —
本 PR #19c-2 階段不做對齊邏輯,純 Bronze → Silver pack;對齊邏輯留 PR #19c-3
orchestrator 整合時加。
"""

from __future__ import annotations

import logging
import time
from typing import Any

from .._common import fetch_bronze, upsert_silver


logger = logging.getLogger("collector.silver.builders.financial_statement")


NAME          = "financial_statement"
SILVER_TABLE  = "financial_statement_derived"
BRONZE_TABLES = ["financial_statement"]


def _pack(bronze_rows: list[dict[str, Any]]) -> list[dict[str, Any]]:
    """Bronze N rows per (stock × date × event_type) → Silver 1 row,
    detail JSONB pack origin_name → value。"""
    grouped: dict[tuple, dict[str, Any]] = {}

    for row in bronze_rows:
        key = (
            row.get("market"),
            row.get("stock_id"),
            row.get("date"),
            row.get("event_type"),    # = Silver.type
        )
        if key not in grouped:
            grouped[key] = {
                "market":   row.get("market"),
                "stock_id": row.get("stock_id"),
                "date":     row.get("date"),
                "type":     row.get("event_type"),
                "detail":   {},
            }

        # origin_name 作為 detail key。balance/balance_per 同 origin_name 但語意不同:
        # balance type = 元值,balance_per type = % common-size。加 _per suffix 避免覆蓋。
        item_key = row.get("origin_name") or row.get("type") or "unknown"
        if (row.get("type") or "").endswith("_per"):
            item_key = f"{item_key}_per"
        grouped[key]["detail"][item_key] = row.get("value")

    return list(grouped.values())


def run(
    db: Any,
    stock_ids: list[str] | None = None,
    full_rebuild: bool = False,
) -> dict[str, Any]:
    start = time.monotonic()

    bronze = fetch_bronze(
        db, "financial_statement",
        stock_ids=stock_ids,
        order_by="market, stock_id, date, event_type, origin_name",
    )
    silver = _pack(bronze)
    written = upsert_silver(
        db, SILVER_TABLE, silver,
        pk_cols=["market", "stock_id", "date", "type"],
    )

    elapsed_ms = int((time.monotonic() - start) * 1000)
    logger.info(f"[{NAME}] read={len(bronze)} → wrote={written}({elapsed_ms}ms)")
    return {
        "name":         NAME,
        "rows_read":    len(bronze),
        "rows_written": written,
        "elapsed_ms":   elapsed_ms,
    }
