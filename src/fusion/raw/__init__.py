"""Fusion Layer · Raw 端口 — M3 即時請求路徑層。

對齊 m3Spec/fusion_layer.md §4.1(Raw 端口 = 既有 aggregation_layer 行為,並排不整合)。

核心 API:`as_of(stock_id, date)` 回傳 AsOfSnapshot,內含:
- facts(已過 look-ahead bias 防衛)
- indicator_latest(各 indicator core 最新值)
- structural(neely scenario forest 等)
- market(5 個保留字 stock_id 並排)

並排呈現,不整合(對齊 cores_overview §九 / §十一)。
"""

from fusion.raw._types import (
    AsOfSnapshot,
    FactRow,
    IndicatorRow,
    StructuralRow,
    QueryMetadata,
)
from fusion.raw.query import as_of, as_of_with_ohlc, find_facts_today, health_check

# 公開面(P2-1):外部消費者實際使用的 _db / _market helper,不多收。
# 用 PEP 562 module __getattr__ **轉發而非綁定** — 既有測試以
# `patch("fusion.raw._db.get_connection", ...)` 打內部模組,轉發讓 patch
# 對「從公開出口 lazy import 的 caller」持續生效(eager re-export 會在
# package import 時凍結原函式物件,patch 打不進)。
_DB_FORWARDS = frozenset({
    "ALLOWED_RANKED_TABLES",
    "fetch_cross_stock_ranked",
    "fetch_latest_close",
    "fetch_ohlc",
    "get_connection",
})
_MARKET_FORWARDS = frozenset({"fetch_market_facts"})


def __getattr__(name: str):
    if name in _DB_FORWARDS:
        from fusion.raw import _db
        return getattr(_db, name)
    if name in _MARKET_FORWARDS:
        from fusion.raw import _market
        return getattr(_market, name)
    raise AttributeError(f"module 'fusion.raw' has no attribute {name!r}")


def __dir__() -> list[str]:
    return sorted(set(globals()) | set(__all__))


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
    # _db / _market 公開面(__getattr__ 轉發)
    "ALLOWED_RANKED_TABLES",
    "fetch_cross_stock_ranked",
    "fetch_latest_close",
    "fetch_market_facts",
    "fetch_ohlc",
    "get_connection",
]
