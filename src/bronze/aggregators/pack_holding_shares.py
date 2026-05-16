"""
bronze/aggregators/pack_holding_shares.py
=========================================
股權分散表 N 級距 → 1 row + detail JSONB pack。
"""

import json
import logging
from typing import Any

logger = logging.getLogger("collector.bronze.aggregators.pack_holding_shares")


def aggregate_holding_shares(rows: list[dict[str, Any]]) -> list[dict[str, Any]]:
    """股權分散 API 每筆 = 1 個持股級距 → 1 筆 / 日 + detail JSONB。

    FinMind TaiwanStockHoldingSharesPer 格式:
      {date, stock_id, HoldingSharesLevel=級距, people=人數, percent=佔比, unit=股數}

    打包後格式:
      {date, stock_id, detail=JSON{HoldingSharesLevel: {people, percent, unit}, ...}}
    """
    grouped: dict[tuple, dict[str, Any]] = {}

    for row in rows:
        key = (row.get("date"), row.get("stock_id"))
        if key not in grouped:
            grouped[key] = {
                "date":     row.get("date"),
                "stock_id": row.get("stock_id"),
                "detail":   {},
                "market":   row.get("market", "TW"),
                "source":   row.get("source", "finmind"),
            }

        level = row.get("HoldingSharesLevel") or row.get("holding_shares_level", "unknown")
        grouped[key]["detail"][level] = {
            "people":  row.get("people"),
            "percent": row.get("percent"),
            "unit":    row.get("unit"),
        }

    result = []
    for agg in grouped.values():
        agg["detail"] = json.dumps(agg["detail"], ensure_ascii=False)
        result.append(agg)

    logger.debug(f"holding_shares pack:{len(rows)} 筆 → {len(result)} 筆")
    return result
