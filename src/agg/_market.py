"""Market-level facts 並排組合(對齊 m3Spec/aggregation_layer.md §七)。

保留字 stock_id(對齊 cores_overview §6.2.1):
- _index_taiex_       TAIEX 加權指數
- _index_us_market_   美股 SPY / VIX
- _index_business_    景氣指標
- _market_            市場層級籌碼
- _global_            全球性指標(匯率、fear_greed)
"""

from __future__ import annotations

from datetime import date
from typing import Any

MARKET_RESERVED_STOCK_IDS = [
    "_index_taiex_",
    "_index_us_market_",
    "_index_business_",
    "_market_",
    "_global_",
]


def fetch_market_facts(
    conn,
    *,
    as_of: date,
    lookback_days: int,
    cores: list[str] | None = None,
    apply_lookahead_filter: bool = True,
) -> dict[str, list[dict[str, Any]]]:
    """撈 5 個保留字 stock_id 的 facts,以 stock_id grouped 回傳。

    Args:
        conn: psycopg connection(dict_row factory)
        as_of: 查詢日(look-ahead 上界)
        lookback_days: 期間天數
        cores: 限制 source_core 範圍。None = 全部
        apply_lookahead_filter: 預設 True 內建 look-ahead 過濾(對齊 query.as_of() 行為)。
            設 False 拿 raw rows(對齊舊 API,僅供 debug)

    Returns:
        {
            "_index_taiex_": [facts...],
            "_index_us_market_": [facts...],
            ...
        }
    """
    from agg._db import fetch_facts
    from agg._lookahead import filter_visible

    rows = fetch_facts(
        conn,
        stock_ids=MARKET_RESERVED_STOCK_IDS,
        as_of=as_of,
        lookback_days=lookback_days,
        cores=cores,
    )
    grouped: dict[str, list[dict[str, Any]]] = {sid: [] for sid in MARKET_RESERVED_STOCK_IDS}
    for r in rows:
        sid = r["stock_id"]
        if sid in grouped:
            grouped[sid].append(r)
    if apply_lookahead_filter:
        grouped = {sid: filter_visible(fs, as_of) for sid, fs in grouped.items()}
    return grouped
