"""
bronze/aggregators/pack_financial.py
====================================
財報 N 科目 → 1 row + detail JSONB pack。
"""

import json
import logging
from typing import Any

logger = logging.getLogger("collector.bronze.aggregators.pack_financial")


def aggregate_financial(
    rows: list[dict[str, Any]],
    stmt_type: str,
) -> list[dict[str, Any]]:
    """財報 API 每日 N 筆(per 科目)→ 1 筆 + detail JSONB。

    FinMind 財報 API 格式(每筆 = 1 個財務科目):
      {date, stock_id, type=科目名稱, value=金額, origin_name=英文科目名}

    打包後格式:
      {date, stock_id, type=stmt_type, detail=JSON{origin_name: value, ...}}

    Args:
        rows:      field_mapper 輸出的資料列
        stmt_type: 報表類型識別字串,"income" | "balance" | "cashflow"
    """
    grouped: dict[tuple, dict[str, Any]] = {}

    for row in rows:
        key = (row.get("date"), row.get("stock_id"))
        if key not in grouped:
            grouped[key] = {
                "date":     row.get("date"),
                "stock_id": row.get("stock_id"),
                "type":     stmt_type,
                "detail":   {},           # 先用 dict,最後序列化
                "market":   row.get("market", "TW"),
                "source":   row.get("source", "finmind"),
            }

        # origin_name 為英文科目名,type 為中文科目名
        # 優先用 origin_name 作為 detail 的 key,fallback 用 type
        item_key   = row.get("origin_name") or row.get("type") or "unknown"
        item_value = row.get("value")
        grouped[key]["detail"][item_key] = item_value

    result = []
    for agg in grouped.values():
        agg["detail"] = json.dumps(agg["detail"], ensure_ascii=False)
        result.append(agg)

    logger.debug(f"financial pack({stmt_type}):{len(rows)} 筆 → {len(result)} 筆")
    return result
