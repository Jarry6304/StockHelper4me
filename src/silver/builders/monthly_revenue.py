"""
silver/builders/monthly_revenue.py
==================================
monthly_revenue_tw (Bronze) → monthly_revenue_derived (Silver)。

Bronze 是 raw FinMind 欄名(revenue_year / revenue_month / country / create_time),
Silver 用 v2.0 慣用名(revenue_yoy / revenue_mom)+ detail JSONB 收 country /
create_time(對齊既有 v2.0 monthly_revenue schema 結構)。

Rename:
  revenue_year  → revenue_yoy(年增百分比)
  revenue_month → revenue_mom(月增百分比)
  country / create_time → detail JSONB

create_time:Bronze TEXT(per PR #18.5 hotfix m2n3o4p5q6r7,FinMind 對某些 row
回 "" 不是 NULL),Silver detail 直接保留 TEXT。Aggregation Layer 之後若要 cast
TIMESTAMPTZ 用 NULLIF(create_time, '')::TIMESTAMPTZ 處理空字串。

Bronze 已 PR #18.5 落地(smoke test 3 stocks 通過)。
"""

from __future__ import annotations

import logging
import time
from typing import Any

from .._common import fetch_bronze, upsert_silver


logger = logging.getLogger("collector.silver.builders.monthly_revenue")


NAME          = "monthly_revenue"
SILVER_TABLE  = "monthly_revenue_derived"
BRONZE_TABLES = ["monthly_revenue_tw"]

DETAIL_KEYS = ("country", "create_time")


def _build_silver_rows(bronze_rows: list[dict[str, Any]]) -> list[dict[str, Any]]:
    out: list[dict[str, Any]] = []
    for row in bronze_rows:
        s: dict[str, Any] = {
            "market":      row.get("market"),
            "stock_id":    row.get("stock_id"),
            "date":        row.get("date"),
            "revenue":     row.get("revenue"),
            "revenue_yoy": row.get("revenue_year"),    # rename
            "revenue_mom": row.get("revenue_month"),   # rename
            "detail":      {k: row.get(k) for k in DETAIL_KEYS},
        }
        out.append(s)
    return out


def run(
    db: Any,
    stock_ids: list[str] | None = None,
    full_rebuild: bool = False,
) -> dict[str, Any]:
    start = time.monotonic()

    bronze = fetch_bronze(db, "monthly_revenue_tw", stock_ids=stock_ids)
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
