"""Data tools — 回 JSON(text content),包 src/agg/ aggregation layer。

對齊 plan Phase D §Tool surface(Data tools)+ MCP v2 重構 plan
(`/root/.claude/plans/hashed-foraging-pixel.md`)。

**Public tools(LLM 預設曝露,LLM-friendly 高度封裝)**:
- `market_context`:大盤環境綜合判讀(Tool 3,plan §Tool 3)
- (Tool 1 `neely_forecast` 留 Step 3)
- (Tool 2 `stock_health` 留 Step 2)

**Hidden tools(向下兼容,LLM 預設不可見;debug / direct script 用)**:
- `as_of_snapshot`:raw AsOfSnapshot(可能爆 token,LLM 用 public tools 取代)
- `find_facts`:跨股搜尋當日 fact
- `list_cores`:23 cores 清單
- `fetch_ohlc`:price_daily_fwd OHLC 序列
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


# ────────────────────────────────────────────────────────────
# Public toolkit v2(LLM 預設曝露)— 對齊 plan §3 Tool 設計
# ────────────────────────────────────────────────────────────


def neely_forecast(
    stock_id: str,
    date: str,
) -> dict[str, Any]:
    """Neely 預測:4 個時間框架(月 / 季 / 半年 / 年)+ 上漲機率 + 價位區間(plan §Tool 1)。

    內部:撈 Neely scenario_forest 取 top 5 by power_rating → Fibonacci 投影
    分 4 時間框架 → 跨 cores 加權算 prob_up → invalidation_price 從 triggers 抽。

    輸出只回結論(~2 KB / ~500 tokens),不回 raw scenario_forest。

    Args:
        stock_id: 股票代號(例 "2330")
        date: 查詢日 ISO 字串

    Returns:
        {
          "stock_id": "2330",
          "as_of": "2026-05-13",
          "current_price": 1234.5,
          "primary_scenario": {label, pattern_type, power_rating, wave_count},
          "scenario_count": int,
          "forecasts": {
            "1_month":   {"prob_up": 0.62, "range_high": [...], "range_low": [...]},
            "1_quarter": {...},
            "6_month":   {...},
            "1_year":    {...}
          },
          "key_levels": {"support": [...], "resistance": [...]},
          "invalidation_price": float | None
        }
    """
    from mcp_server._forecast import compute_neely_forecast

    return compute_neely_forecast(stock_id, _parse_date(date))


def stock_health(
    stock_id: str,
    date: str,
    lookback_days: int = 90,
) -> dict[str, Any]:
    """個股 4 維健康度評分(plan §Tool 2)。

    內部:撈 agg.as_of() 全 cores → 4 維 score(technical / chip /
    valuation / fundamental)加權 → top 5 訊號排序 → 1 句 narrative。

    輸出只回結論(~2 KB / ~500 tokens),不回 raw indicator series。

    Args:
        stock_id: 股票代號(例 "2330")
        date: 查詢日 ISO 字串
        lookback_days: facts 期間。預設 90

    Returns:
        {
          "stock_id": "2330",
          "as_of": "2026-05-13",
          "current_price": 1234.5,
          "overall_score": -100~+100,
          "dimensions": {
            "technical":   {"score": X, "trend": "bullish|bearish|mixed|quiet", ...},
            "chip":        {...},
            "valuation":   {...},
            "fundamental": {...}
          },
          "top_signals": [{date, core, kind, sign, weight}, ...],  # max 5
          "narrative": "..."
        }
    """
    from mcp_server._health import compute_stock_health

    return compute_stock_health(stock_id, _parse_date(date), lookback_days=lookback_days)


def magic_formula_screen(
    date: str,
    top_n: int = 30,
) -> dict[str, Any]:
    """Greenblatt 2005 Magic Formula 跨股篩選(v3.4 plan §Phase C)。

    內部:讀 magic_formula_ranked_derived(Silver builder 跨股 cross-rank)→
    JOIN stock_info_ref 拿公司名 / industry → top N + median EY/ROIC + 1 句 narrative。
    輸出 ~5 KB / ~1250 tokens。

    Universe:排除金融保險 + 公用事業(Greenblatt 2005 §六 原版)。
    Rank:combined_rank = ey_rank + roic_rank,愈低愈好。

    Args:
        date:  查詢日 ISO 字串(例 "2026-05-15")
        top_n: 取 top N(預設 30 對齊 Greenblatt 原版 20-30)

    Returns:
        {
          "as_of": "2026-05-15",
          "ranking_date": "...",         # 實際 ranking 日(≤ as_of 的 latest)
          "universe_size": 1432,
          "top_n": 30,
          "top_stocks": [{"rank": 1, "stock_id": "2330", "name": "...",
                          "industry": "...", "earnings_yield": 0.082,
                          "roic": 0.31, "ey_rank": 145, "roic_rank": 12,
                          "combined_rank": 157}, ...],
          "stats": {"median_ey": 0.045, "median_roic": 0.08, ...},
          "narrative": "..."
        }

    References:
      - Greenblatt, J. (2005). *The Little Book That Beats the Market*. Wiley.
      - Larkin (2009). SSRN id=1330551(OOS 1988-2007 valid)
    """
    from mcp_server._magic_formula import compute_magic_formula_screen

    return compute_magic_formula_screen(_parse_date(date), top_n=top_n)


def kalman_trend(
    stock_id: str,
    date: str,
    lookback_days: int = 180,
) -> dict[str, Any]:
    """個股 1-D Kalman trend + 5-class regime(v3.4 plan §Phase C)。

    內部:走 agg.as_of(cores=["kalman_filter_core"]) → indicator_latest 拉
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

    return compute_kalman_trend(stock_id, _parse_date(date), lookback_days=lookback_days)


def market_context(
    date: str,
    lookback_days: int = 60,
) -> dict[str, Any]:
    """大盤環境綜合判讀(plan §Tool 3)。

    內部:讀 5 個保留字 stock_id 的 market-level facts →
    6 components score(taiex / us_market / fear_greed / business /
    exchange_rate / market_margin)→ climate_score 加權平均 → systemic_risks
    觸發 → 1 句 narrative。

    輸出只回結論(~1.5 KB / ~400 tokens),不回 raw facts series。

    Args:
        date: 查詢日 ISO 字串(例 "2026-05-13")
        lookback_days: facts 期間。預設 60(覆蓋月頻 + daily 雙重)

    Returns:
        {
          "as_of": "2026-05-13",
          "overall_climate": "neutral_bullish" | ...,
          "climate_score": -100~+100,
          "components": {
            "taiex":         {"score": X, "fact_count": N},
            "us_market":     {...},
            "fear_greed":    {...},
            "business":      {...},
            "exchange_rate": {...},
            "market_margin": {...}
          },
          "systemic_risks": [...],
          "narrative": "..."
        }
    """
    from mcp_server._climate import compute_market_context

    return compute_market_context(_parse_date(date), lookback_days=lookback_days)


# ────────────────────────────────────────────────────────────
# v3.22 Public toolkit B-5(對齊 v3.21 新 4 cores)
# ────────────────────────────────────────────────────────────


def loan_collateral_snapshot(
    stock_id: str,
    date: str,
) -> dict[str, Any]:
    """5 大類借券抵押餘額快照(v3.22,對應 chip_cores.md §十 loan_collateral_core)。

    內部:SELECT loan_collateral_balance_derived 取個股 <= as_of 最新一筆 →
    5 大類 current_balance + change_pct + ratio → 集中度警示(> 70%)→ narrative。

    Args:
        stock_id: 股票代號(例 "2330")
        date:     查詢日 ISO 字串

    Returns:
        {
          "stock_id": "2330", "as_of": "...",
          "snapshot_date": "...",
          "categories": {
            "margin":             {"balance": ..., "change_pct": ..., "ratio": ...},
            "firm_loan":          {...},
            "unrestricted_loan":  {...},
            "finance_loan":       {...},
            "settlement_margin":  {...}
          },
          "total_balance": ..., "dominant_category": ...,
          "dominant_category_label": "...", "concentration_ratio": ...,
          "concentration_alert": bool, "narrative": "..."
        }

    References:
      - Basel Committee (2006), "Studies on Credit Risk Concentration" WP 15.
    """
    from mcp_server._loan_collateral import compute_loan_collateral_snapshot

    return compute_loan_collateral_snapshot(stock_id, _parse_date(date))


def block_trade_summary(
    stock_id: str,
    date: str,
    lookback_days: int = 30,
) -> dict[str, Any]:
    """大宗交易 30 天摘要 + 配對交易 spike(v3.22,對應 chip_cores.md §十一)。

    內部:SELECT block_trade_derived 個股 lookback_days 期間 → SUM volume /
    trading_money / matching_share → MatchingTradeSpike 日標記(>= 80%)→ narrative。

    Args:
        stock_id:      股票代號
        date:          查詢日上界 ISO 字串
        lookback_days: 期間天數(預設 30)

    Returns:
        {
          "stock_id": "...", "as_of": "...", "period_days": 30,
          "active_days": int, "total_volume": ..., "total_trading_money": ...,
          "matching_share_avg": ..., "largest_single_trade_money": ...,
          "matching_spike_dates": [...], "narrative": "..."
        }

    References:
      - Cao, Field & Hanka (2009), "Block Trading and Stock Prices" JEF 16:1-25.
    """
    from mcp_server._block_trade import compute_block_trade_summary

    return compute_block_trade_summary(
        stock_id, _parse_date(date), lookback_days=lookback_days,
    )


def risk_alert_status(
    stock_id: str,
    date: str,
) -> dict[str, Any]:
    """處置股風險警示狀態(v3.22,對應 chip_cores.md §十二 risk_alert_core)。

    內部:直讀 Bronze disposition_securities_period_tw → 判斷 as_of 是否在
    period_start..period_end → 解析 measure 中文 → 三級嚴重度 → 60 日 escalation 鏈。

    Args:
        stock_id: 股票代號
        date:     查詢日 ISO 字串

    Returns:
        {
          "stock_id": "...", "as_of": "...",
          "current_status": {
            "in_disposition_period": bool, "severity": "warning|disposition|cash_only",
            "severity_label": "...", "period_start": "...", "period_end": "...",
            "days_remaining": int
          },
          "history_60d": [...], "escalation_count_60d": int, "narrative": "..."
        }

    References:
      - 「證券交易所公布注意交易資訊處置作業要點」§4(2024 版)— 60 日 ≥ 2 次升級。
    """
    from mcp_server._risk_alert import compute_risk_alert_status

    return compute_risk_alert_status(stock_id, _parse_date(date))


def commodity_macro_snapshot(
    date: str,
    commodities: list[str] | None = None,
) -> dict[str, Any]:
    """商品 macro 信號快照(v3.22,對應 environment_cores.md §十 commodity_macro_core)。

    內部:SELECT commodity_price_daily_derived 對 commodities 列表各取 <= as_of 最新
    一筆 → return_pct / z-score / momentum_state / streak → spike 警戒 → narrative。

    Args:
        date:        查詢日 ISO 字串
        commodities: 要查的 commodity 清單(預設 ["GOLD"];可加 SILVER/OIL future)

    Returns:
        {
          "as_of": "...", "snapshot_date": "...",
          "commodities": [
            {"name": "GOLD", "label": "黃金", "price": ...,
             "return_pct": ..., "return_z_score": ...,
             "momentum_state": "up|down|neutral", "streak_days": int,
             "spike_alert": bool, "data_available": bool}
          ],
          "lookback_days": 60, "narrative": "..."
        }

    References:
      - Brock et al. (1992), JoF 47(5):1731-1764 — macro streak ≥ 5。
      - Hamilton (1989), Econometrica 57(2):357-384 — regime break。
    """
    from mcp_server._commodity_macro import compute_commodity_macro_snapshot

    return compute_commodity_macro_snapshot(
        _parse_date(date), commodities=commodities,
    )


# ────────────────────────────────────────────────────────────
# Hidden tools(向下兼容,LLM 預設不可見;debug / direct script 用)
# ────────────────────────────────────────────────────────────


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
