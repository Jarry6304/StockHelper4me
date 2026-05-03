"""
silver/builders/margin.py
=========================
margin_purchase_short_sale_tw + securities_lending_tw (Bronze) →
                                margin_daily_derived (Silver)。

PR #19b 階段:只接 margin_purchase_short_sale_tw,SBL 6 欄留 PR #19c。

Silver 欄位來源:
- 6 stored cols(1:1 自 Bronze)
    margin_purchase / margin_sell / margin_balance /
    short_sale / short_cover / short_balance
- detail JSONB(從 Bronze 8 個 unpack 欄重 pack)
    margin_cash_repay / margin_prev_balance / margin_limit /
    short_cash_repay / short_prev_balance / short_limit /
    offset_loan_short / note
- 3 個 margin_short_sales_* 別名(per spec §2.6.1)= 對應 short_sale / short_cover /
                                                    short_balance(語意 namespacing,非新資料)
- 3 個 sbl_short_sales_* 欄 → PR #19c 接 securities_lending_tw aggregation 後填

Round-trip 驗證:Silver 6 stored + detail JSONB 應與 v2.0 margin_daily 等值
                (1e-9 容差 + None-only entry normalize,對齊 _reverse_pivot_lib 比對策略)。
"""

from __future__ import annotations

import logging
import time
from typing import Any

from .._common import fetch_bronze, upsert_silver


logger = logging.getLogger("collector.silver.builders.margin")


NAME          = "margin"
SILVER_TABLE  = "margin_daily_derived"
BRONZE_TABLES = ["margin_purchase_short_sale_tw"]   # PR #19c +securities_lending_tw

STORED_COLS = (
    "margin_purchase", "margin_sell", "margin_balance",
    "short_sale", "short_cover", "short_balance",
)

DETAIL_KEYS = (
    "margin_cash_repay", "margin_prev_balance", "margin_limit",
    "short_cash_repay", "short_prev_balance", "short_limit",
    "offset_loan_short", "note",
)


def _build_silver_rows(bronze_rows: list[dict[str, Any]]) -> list[dict[str, Any]]:
    out: list[dict[str, Any]] = []
    for row in bronze_rows:
        s: dict[str, Any] = {
            "market":   row.get("market"),
            "stock_id": row.get("stock_id"),
            "date":     row.get("date"),
        }
        # 6 stored cols(1:1)
        for c in STORED_COLS:
            s[c] = row.get(c)
        # detail JSONB 重 pack(8 keys)
        s["detail"] = {k: row.get(k) for k in DETAIL_KEYS}
        # 3 margin_short_sales_* 別名(per spec §2.6.1 — 對應 short_*,語意 namespacing)
        s["margin_short_sales_short_sales"]         = row.get("short_sale")
        s["margin_short_sales_short_covering"]      = row.get("short_cover")
        s["margin_short_sales_current_day_balance"] = row.get("short_balance")
        # 3 SBL 欄 — PR #19c 接 securities_lending_tw 後填
        s["sbl_short_sales_short_sales"]            = None
        s["sbl_short_sales_returns"]                = None
        s["sbl_short_sales_current_day_balance"]    = None
        out.append(s)
    return out


def run(
    db: Any,
    stock_ids: list[str] | None = None,
    full_rebuild: bool = False,
) -> dict[str, Any]:
    start = time.monotonic()

    bronze = fetch_bronze(db, "margin_purchase_short_sale_tw", stock_ids=stock_ids)
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
