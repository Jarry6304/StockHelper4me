"""
silver/builders/day_trading.py
==============================
day_trading_tw (Bronze) → day_trading_derived (Silver)。

PR #19b 階段:1:1 直拷 raw 部分(2 stored + detail JSONB 重 pack)。
day_trading_ratio 衍生欄(= day_trading_buy / price_daily.volume)留 PR #19c 7b 階段
跨表 join price_daily 才能算。

- 2 stored cols(1:1):day_trading_buy / day_trading_sell
- detail JSONB 從 2 個 Bronze unpack 欄重 pack:day_trading_flag / volume

Round-trip:Silver 2 stored + detail JSONB 應與 v2.0 day_trading 等值。
"""

from __future__ import annotations

import logging
import time
from typing import Any

from .._common import fetch_bronze, upsert_silver


logger = logging.getLogger("collector.silver.builders.day_trading")


NAME          = "day_trading"
SILVER_TABLE  = "day_trading_derived"
BRONZE_TABLES = ["day_trading_tw"]

STORED_COLS = ("day_trading_buy", "day_trading_sell")
DETAIL_KEYS = ("day_trading_flag", "volume")


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

    bronze = fetch_bronze(db, "day_trading_tw", stock_ids=stock_ids)
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
