"""
silver/builders/business_indicator.py
=====================================
business_indicator_tw (Bronze) → business_indicator_derived (Silver)。

景氣指標(月頻 reference 級別,非 Core)。Bronze 已 PR #14 落地(B-6)。
spec §6.3:Silver 等同 Bronze 加 dirty,**不做衍生計算**(streak/3m_avg/changed
都由 Aggregation Layer 即時算)。

欄名對映:Bronze 與 Silver 都用 `leading_indicator` / `coincident_indicator` /
        `lagging_indicator`(避 PG 保留字 LEADING,PR #19a hotfix 對齊 Bronze)。
        Builder 不需做 rename。

PK 差異:
- Bronze business_indicator_tw PK = (market, date)— 市場級表,無 stock_id 欄
- Silver business_indicator_derived PK = (market, stock_id, date)
  (per spec §6.3:stock_id DEFAULT '_market_' 維持 Silver 統一 PK 格式)
- Builder 寫 Silver 時注入 stock_id = '_market_'
"""

from __future__ import annotations

import logging
import time
from typing import Any

from .._common import fetch_bronze, upsert_silver


logger = logging.getLogger("collector.silver.builders.business_indicator")


NAME          = "business_indicator"
SILVER_TABLE  = "business_indicator_derived"
BRONZE_TABLES = ["business_indicator_tw"]

STORED_COLS = (
    "leading_indicator", "coincident_indicator", "lagging_indicator",
    "monitoring", "monitoring_color",
)

MARKET_SENTINEL = "_market_"   # per spec §6.3


def _build_silver_rows(bronze_rows: list[dict[str, Any]]) -> list[dict[str, Any]]:
    out: list[dict[str, Any]] = []
    for row in bronze_rows:
        s: dict[str, Any] = {
            "market":   row.get("market", "tw"),
            "stock_id": MARKET_SENTINEL,
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
    """注意 stock_ids 對 market-level 表無效,一律全讀。"""
    start = time.monotonic()

    bronze = fetch_bronze(db, "business_indicator_tw", order_by="market, date")
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
