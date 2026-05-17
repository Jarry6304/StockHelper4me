"""
silver/builders/block_trade.py
==============================
block_trade_tw (Bronze;PK 含 trade_type) → block_trade_derived (Silver;SUM by trade_type
per stock,date)。

對齊 m3Spec/chip_cores.md §11.2 拍版設計:
- 同 (stock, date) 可能多筆 trade_type → Silver 聚合為單一 row
- 主欄:total_volume / total_trading_money / matching_* / largest_single_trade_money /
  trade_type_count
- JSONB detail:per-trade_type breakdown(volume / trading_money)

matching_share 用於 MatchingTradeSpike EventKind 偵測(配對交易單日佔比 > 0.80)。
"""

from __future__ import annotations

import logging
import time
from collections import defaultdict
from typing import Any

from .._common import fetch_bronze, upsert_silver


logger = logging.getLogger("collector.silver.builders.block_trade")


NAME          = "block_trade"
SILVER_TABLE  = "block_trade_derived"
BRONZE_TABLES = ["block_trade_tw"]

MATCHING_TRADE_TYPE = "配對交易"  # FinMind 真實字串(從 v3.19 probe 確認)


def _build_silver_rows(bronze_rows: list[dict[str, Any]]) -> list[dict[str, Any]]:
    """SUM by (market, stock_id, date) 聚合 trade_type 多筆。"""
    grouped: dict[tuple, list[dict[str, Any]]] = defaultdict(list)
    for row in bronze_rows:
        key = (row.get("market"), row.get("stock_id"), row.get("date"))
        grouped[key].append(row)

    out: list[dict[str, Any]] = []
    for (market, stock_id, date), trades in grouped.items():
        total_vol = sum(int(t.get("volume") or 0) for t in trades)
        total_money = sum(int(t.get("trading_money") or 0) for t in trades)
        matching = [t for t in trades if t.get("trade_type") == MATCHING_TRADE_TYPE]
        matching_vol = sum(int(t.get("volume") or 0) for t in matching)
        matching_money = sum(int(t.get("trading_money") or 0) for t in matching)
        matching_share = (matching_vol / total_vol) if total_vol > 0 else None
        largest = max((int(t.get("trading_money") or 0) for t in trades), default=0)
        distinct_types = len({t.get("trade_type") for t in trades})

        # per-trade_type breakdown for detail JSONB
        per_type: dict[str, dict[str, int]] = defaultdict(lambda: {"volume": 0, "trading_money": 0})
        for t in trades:
            tt = t.get("trade_type") or "unknown"
            per_type[tt]["volume"] += int(t.get("volume") or 0)
            per_type[tt]["trading_money"] += int(t.get("trading_money") or 0)

        out.append({
            "market":   market,
            "stock_id": stock_id,
            "date":     date,
            "total_volume":               total_vol,
            "total_trading_money":        total_money,
            "matching_volume":            matching_vol,
            "matching_trading_money":     matching_money,
            "matching_share":             matching_share,
            "largest_single_trade_money": largest,
            "trade_type_count":           distinct_types,
            "detail":                     dict(per_type),
        })
    return out


def run(
    db: Any,
    stock_ids: list[str] | None = None,
    full_rebuild: bool = False,
) -> dict[str, Any]:
    start = time.monotonic()

    bronze = fetch_bronze(db, "block_trade_tw", stock_ids=stock_ids)
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
