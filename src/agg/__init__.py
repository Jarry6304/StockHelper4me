"""Aggregation Layer — M3 即時請求路徑層。

對齊 m3Spec/aggregation_layer.md r1。

核心 API:`as_of(stock_id, date)` 回傳 AsOfSnapshot,內含:
- facts(已過 look-ahead bias 防衛)
- indicator_latest(各 indicator core 最新值)
- structural(neely scenario forest 等)
- market(5 個保留字 stock_id 並排)

並排呈現,不整合(對齊 cores_overview §九 / §十一)。
"""

from agg._types import (
    AsOfSnapshot,
    FactRow,
    IndicatorRow,
    StructuralRow,
    QueryMetadata,
)
from agg.query import as_of, as_of_with_ohlc, find_facts_today, health_check

__all__ = [
    "as_of",
    "as_of_with_ohlc",
    "find_facts_today",
    "health_check",
    "AsOfSnapshot",
    "FactRow",
    "IndicatorRow",
    "StructuralRow",
    "QueryMetadata",
]
