"""Data tools — 回 JSON(text content),包 src/agg/ aggregation layer。

對齊 plan Phase D §Tool surface(Data tools)。

4 個 tools:
- as_of_snapshot:as_of(stock_id, date) 主路徑
- find_facts:跨股搜尋當日 fact(對齊 §9.4 use case)
- list_cores:23 cores 清單 + priority/kind
- fetch_ohlc:price_daily_fwd OHLC 序列
"""

from __future__ import annotations

from datetime import date as Date
from typing import Any


def _parse_date(value: str | Date) -> Date:
    """ISO 字串 → date。已是 date 直接 pass through。"""
    if isinstance(value, Date):
        return value
    return Date.fromisoformat(value)


# ────────────────────────────────────────────────────────────
# Data tools
# ────────────────────────────────────────────────────────────


def as_of_snapshot(
    stock_id: str,
    date: str,
    lookback_days: int = 90,
    include_market: bool = True,
    cores: list[str] | None = None,
    timeframes: list[str] | None = None,
) -> dict[str, Any]:
    """查詢個股在指定日期的 aggregation snapshot。

    回:
      - facts: 該股 lookback 期間的 fact events(已過 look-ahead bias 防衛)
      - indicator_latest: 各 indicator core 在 as_of <= date 最新一筆
      - structural: structural_snapshots(neely_core scenario_forest 等)
      - market: 5 個保留字 stock_id 的 market-level facts(若 include_market=True)
      - metadata: query 參數 + as_of(reproducibility)

    Args:
        stock_id: 股票代號(例 "2330";或保留字 "_index_taiex_" / "_us_spy_" / "_us_vix_"
            / "_market_fear_greed_" / "_market_margin_")
        date: as_of 查詢日 ISO 字串(例 "2026-05-13")
        lookback_days: facts 期間天數。預設 90
        include_market: 是否並排 market-level facts。預設 True
        cores: 限制 source_core(例 ["macd_core", "rsi_core"])。None=全 23 cores
        timeframes: 限制 indicator timeframe(例 ["daily", "weekly"])。None=全部

    Returns:
        AsOfSnapshot dict — date 欄全部 ISO 字串(JSON-serializable)
    """
    from agg import as_of

    snapshot = as_of(
        stock_id,
        _parse_date(date),
        lookback_days=lookback_days,
        include_market=include_market,
        cores=cores,
        timeframes=timeframes,
    )
    return snapshot.to_dict()


def find_facts(
    date: str,
    source_core: str | None = None,
    kind: str | None = None,
) -> list[dict[str, Any]]:
    """跨 stock 搜尋:今天有哪些股票觸發某 fact。

    對齊 m3Spec/aggregation_layer.md §9.4 use case:選股 / 篩標的。

    Args:
        date: 查詢日 ISO 字串(例 "2026-05-13")
        source_core: 限制 source_core(例 "rsi_core")。None=全 cores
        kind: 限制 metadata.kind(例 "RsiOversold" / "GoldenCross")。
            走 JSONB 過濾,需配 source_core 才有效收斂

    Returns:
        當日該 fact 的 list[dict] — 每筆 fact 含 stock_id / fact_date /
        source_core / statement / metadata 等
    """
    from agg import find_facts_today

    rows = find_facts_today(
        _parse_date(date),
        source_core=source_core,
        kind=kind,
    )
    return [r.to_dict() for r in rows]


# 23 cores 對齊 rust_compute/cores/ 實際 Cargo crates。
# kind:Wave / Indicator / Chip / Fundamental / Environment(對齊
# cores_overview.md §8)。
_CORES: list[dict[str, str]] = [
    # Wave (1)
    {"name": "neely_core",                "kind": "Wave",        "priority": "P0"},
    # Indicator (8)
    {"name": "ma_core",                   "kind": "Indicator",   "priority": "P1"},
    {"name": "macd_core",                 "kind": "Indicator",   "priority": "P1"},
    {"name": "rsi_core",                  "kind": "Indicator",   "priority": "P1"},
    {"name": "kd_core",                   "kind": "Indicator",   "priority": "P1"},
    {"name": "adx_core",                  "kind": "Indicator",   "priority": "P1"},
    {"name": "atr_core",                  "kind": "Indicator",   "priority": "P1"},
    {"name": "bollinger_core",            "kind": "Indicator",   "priority": "P1"},
    {"name": "obv_core",                  "kind": "Indicator",   "priority": "P1"},
    # Chip (5)
    {"name": "institutional_core",        "kind": "Chip",        "priority": "P2"},
    {"name": "margin_core",               "kind": "Chip",        "priority": "P2"},
    {"name": "foreign_holding_core",      "kind": "Chip",        "priority": "P2"},
    {"name": "day_trading_core",          "kind": "Chip",        "priority": "P2"},
    {"name": "shareholder_core",          "kind": "Chip",        "priority": "P2"},
    # Fundamental (3)
    {"name": "revenue_core",              "kind": "Fundamental", "priority": "P2"},
    {"name": "valuation_core",            "kind": "Fundamental", "priority": "P2"},
    {"name": "financial_statement_core",  "kind": "Fundamental", "priority": "P2"},
    # Environment (6)
    {"name": "taiex_core",                "kind": "Environment", "priority": "P2"},
    {"name": "us_market_core",            "kind": "Environment", "priority": "P2"},
    {"name": "exchange_rate_core",        "kind": "Environment", "priority": "P2"},
    {"name": "fear_greed_core",           "kind": "Environment", "priority": "P2"},
    {"name": "market_margin_core",        "kind": "Environment", "priority": "P2"},
    {"name": "business_indicator_core",   "kind": "Environment", "priority": "P2"},
]


def list_cores() -> dict[str, Any]:
    """列出 23 個 cores + priority/kind/version。

    Returns:
        {
          "total": 23,
          "by_kind": {"Wave": 1, "Indicator": 8, "Chip": 5, "Fundamental": 3, "Environment": 6},
          "cores": [{name, kind, priority}, ...]
        }
    """
    by_kind: dict[str, int] = {}
    for c in _CORES:
        by_kind[c["kind"]] = by_kind.get(c["kind"], 0) + 1
    return {
        "total": len(_CORES),
        "by_kind": by_kind,
        "cores": _CORES,
    }


def fetch_ohlc(
    stock_id: str,
    date: str,
    lookback_days: int = 90,
) -> list[dict[str, Any]]:
    """從 price_daily_fwd 撈 OHLC + volume 序列(後復權)。

    Args:
        stock_id: 股票代號(支援 _index_taiex_ 等保留字)
        date: 上界 ISO 字串
        lookback_days: 期間天數

    Returns:
        list[dict] {date, open, high, low, close, volume},ORDER BY date ASC。
        date 欄全 ISO 字串。
    """
    from agg._db import fetch_ohlc as _fetch, get_connection

    conn = get_connection()
    try:
        rows = _fetch(
            conn,
            stock_id=stock_id,
            as_of=_parse_date(date),
            lookback_days=lookback_days,
        )
        # date object → ISO + Decimal → float(JSON-serializable)
        out = []
        for r in rows:
            out.append({
                "date":   r["date"].isoformat() if r["date"] else None,
                "open":   float(r["open"])   if r["open"]   is not None else None,
                "high":   float(r["high"])   if r["high"]   is not None else None,
                "low":    float(r["low"])    if r["low"]    is not None else None,
                "close":  float(r["close"])  if r["close"]  is not None else None,
                "volume": float(r["volume"]) if r["volume"] is not None else None,
            })
        return out
    finally:
        conn.close()
