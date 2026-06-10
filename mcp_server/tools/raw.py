"""Raw / hidden 域 tools — as_of snapshot / fact 搜尋 / cores 清單 / OHLC / Kalman。

as_of_snapshot / find_facts / list_cores / fetch_ohlc 為 hidden tools(預設不
註冊 MCP;debug / direct script 用);kalman_trend 為 public。
註冊見 server.py mcp.tool() 區塊。
"""

from __future__ import annotations

from typing import Any

from mcp_server.tools._shared import _MAX_LOOKBACK_DAYS, _clamp, _parse_date


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
        cores: 限制 source_core(例 ["macd_core", "rsi_core"])。None=全部 cores(清單以 list_cores() 回傳為準)
        timeframes: 限制 indicator timeframe(例 ["daily", "weekly"])。None=全部

    Returns:
        AsOfSnapshot dict — date 欄全部 ISO 字串(JSON-serializable)
    """
    from fusion.raw import as_of

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
    from fusion.raw import find_facts_today

    rows = find_facts_today(
        _parse_date(date),
        source_core=source_core,
        kind=kind,
    )
    return [r.to_dict() for r in rows]


# 對齊 rust_compute/cores/ 實際 crates;新增 core 須同步本清單(單一真相源 = Cargo workspace)。
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


def kalman_trend(
    stock_id: str,
    date: str,
    lookback_days: int = 180,
) -> dict[str, Any]:
    """個股 1-D Kalman trend + 5-class regime(v3.4 plan §Phase C)。

    內部:走 fusion.raw.as_of(cores=["kalman_filter_core"]) → indicator_latest 拉
    smoothed_price / velocity / uncertainty / regime → facts 拉 recent
    regime transitions → 1 句 narrative。
    輸出 ~1.5 KB / ~400 tokens。

    Regime 5 類:
      stable_up / accelerating / sideway / decelerating / stable_down

    Args:
        stock_id:      股票代號(例 "2330")
        date:          as_of 查詢日 ISO 字串
        lookback_days: facts / indicator 期間。預設 180

    Returns:
        {
          "stock_id": "2330", "as_of": "...",
          "current_price": ..., "smoothed_price": ...,
          "trend_velocity": ..., "uncertainty_band": [lo, hi],
          "deviation_sigma": ..., "regime": "stable_up",
          "regime_label": "穩定上漲",
          "recent_regime_changes": [{"date": "...", "from": "...", "to": "..."}],
          "narrative": "..."
        }

    References:
      - Kalman (1960). Trans. ASME J. Basic Engineering, 82(1), 35-45.
      - Roncalli (2013). *Lectures on Risk Management*. CRC Press, §11.2.
    """
    from mcp_server._kalman import compute_kalman_trend

    return compute_kalman_trend(
        stock_id, _parse_date(date), lookback_days=_clamp(lookback_days, 1, _MAX_LOOKBACK_DAYS),
    )


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
    from fusion.raw._db import fetch_ohlc as _fetch, get_connection

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
