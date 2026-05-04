"""
silver/builders/day_trading.py
==============================
day_trading_tw (Bronze) → day_trading_derived (Silver)。

PR #19b 階段:1:1 直拷 raw 部分(2 stored + detail JSONB 重 pack)。
PR #21-A:加 day_trading_ratio 衍生欄(% 單位)— 對齊 chip_cores.md §7.4。

- 2 stored cols(1:1):day_trading_buy / day_trading_sell
- day_trading_ratio = (buy + sell) × 100 / volume(volume 取 Bronze.volume,
  與 price_daily.volume 在同股同日語意一致;volume NULL 或 0 → ratio NULL)
- detail JSONB 從 2 個 Bronze unpack 欄重 pack:day_trading_flag / volume

Round-trip:Silver 2 stored + detail JSONB 應與 v2.0 day_trading 等值;
day_trading_ratio 是 PR #21-A 新增,不在 v2.0 legacy 比對範圍(verifier skip)。
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


def _compute_ratio(buy: Any, sell: Any, volume: Any) -> float | None:
    """day_trading_ratio = (buy + sell) × 100 / volume(per spec §7.4)。

    任一欄 NULL → 不可算,返 None。volume 為 0 / 負數也視為不可算。
    回 float(NUMERIC 由 psycopg 端轉)。
    """
    if buy is None or sell is None or volume is None:
        return None
    try:
        v = float(volume)
        if v <= 0:
            return None
        return (float(buy) + float(sell)) * 100.0 / v
    except (TypeError, ValueError):
        return None


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
        s["day_trading_ratio"] = _compute_ratio(
            row.get("day_trading_buy"), row.get("day_trading_sell"), row.get("volume"),
        )
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
