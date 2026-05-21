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

    內部:撈 fusion.raw.as_of() 全 cores → 4 維 score(technical / chip /
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


def stock_snapshot(
    stock_id: str,
    date: str,
    *,
    database_url: str | None = None,
) -> dict[str, Any]:
    """Fusion A 視角:個股 10-in-1 當下快照。

    10 sections:health / loan_collateral / block_trade / risk_alert /
    market_context / commodity_macro(6 既有)+ fundamentals / institutional /
    shareholder / technical_summary(4 新)+ narrative。各 section 獨立
    graceful degradation — 某段失敗 → 該 section = {"error": ...}。

    Returns:
        {stock_id, as_of, <10 sections>, narrative}
    """
    from fusion.snapshot import stock_snapshot as _stock_snapshot

    return _stock_snapshot(stock_id, _parse_date(date), database_url=database_url)


# ────────────────────────────────────────────────────────────
# v3.32 Cross-Stock Factor Screens(4 個 toolkit MCP wrappers)
# ────────────────────────────────────────────────────────────


def monthly_screen(
    date: str,
    top_n: int = 30,
    *,
    database_url: str | None = None,
) -> dict[str, Any]:
    """v3.32 Toolkit A:Monthly screen — 3 factors + Barroso-Santa-Clara vol overlay。

    對齊 v1.1 提案 §四 Toolkit A:
      - A1 Persistent Momentum(Chen-Chou-Hsieh 2023 JFM)
      - A2 Revenue Momentum 3-consec(Hung-Lu-Yang 2025 RQFA)
      - A3 Institutional Concert(Sias 2004 / 周賓凰-池祥麟 2014)
      - Vol-managed overlay(Barroso-Santa-Clara 2015 JFE)

    Args:
        date:    ISO 字串(例 "2026-05-15")
        top_n:   每 factor 取 top N(預設 30)

    Returns:
        {as_of, top_n, toolkit, factors: {3 sub-factor 各 top_stocks + narrative},
         vol_managed_overlay: {scale, rationale}, narrative}
    """
    from mcp_server._screens import compute_monthly_screen

    return compute_monthly_screen(_parse_date(date), top_n=top_n,
                                   database_url=database_url)


def quarterly_screen(
    date: str,
    top_n: int = 30,
    *,
    database_url: str | None = None,
) -> dict[str, Any]:
    """v3.32 Toolkit B:Quarterly screen — F-Score + Low Vol + Industry-Adj GP。

    對齊 v1.1 提案 §四 Toolkit B:
      - B1 Piotroski F-Score ≥ 7(Piotroski 2000 JAR / Walkshäusl 2020 JAM)
      - B2 Low Volatility 252d(Ang et al 2009 JFE / Blitz-van Vliet 2007 JPM)
      - B3 Industry-Adjusted GP(Novy-Marx 2013 JFE / Ng-Shen 2020 A&F)

    Returns:
        {as_of, top_n, toolkit, factors: {3 sub-factor 各 top_stocks + narrative}, narrative}
    """
    from mcp_server._screens import compute_quarterly_screen

    return compute_quarterly_screen(_parse_date(date), top_n=top_n,
                                     database_url=database_url)


def annual_low_risk_screen(
    date: str,
    top_n: int = 30,
    *,
    database_url: str | None = None,
) -> dict[str, Any]:
    """v3.32 Toolkit C:Annual low-risk screen — Long-Term Low Vol + Dividend Yield + 12-1 Momentum。

    對齊 v1.1 提案 §四 Toolkit C:
      - C1 Long-Term Low Vol 36M(Blitz-van Vliet 2007)
      - C2 Cash Dividend Yield + yield trap filter(Boudoukh 2007;提案 v1.1 新增 12M return > -20%
        + 5y 至少 3y 配息 filter)
      - C3 12-1 Momentum(Jegadeesh-Titman 1993 JF)

    Returns:
        {as_of, top_n, toolkit, factors: {3 sub-factor 各 top_stocks + narrative}, narrative}
    """
    from mcp_server._screens import compute_annual_low_risk_screen

    return compute_annual_low_risk_screen(_parse_date(date), top_n=top_n,
                                          database_url=database_url)


def monthly_trigger_scan(
    date: str,
    stock_id: str | None = None,
    top_n_per_type: int = 20,
    *,
    database_url: str | None = None,
) -> dict[str, Any]:
    """v3.32 Layer 5:Monthly trigger scan(實驗性 conviction adjustment)。

    對齊 v1.1 提案 §四 Layer 5:
      - Positive trigger:月營收 YoY > +30% + 過去 20D 法人累積買超 → 部位 +20% hint
      - Negative trigger:月營收 YoY < -20% + 法人賣超 > 流通股數 1% → 部位 -50% hint

    v3.32 hotfix(2026-05-18):原全攤 ~400+ triggers → ~94KB payload 爆量。修法:
      - stock_id(可選):指定某股,只回該股 trigger(0-2 筆,payload 小)
      - top_n_per_type(預設 20):全市場 scan 時 per trigger_type 取 yoy 最強 N 個
        (counts 仍回 total 不被截斷)

    底層因子 A 級(Hung-Lu-Yang 2025 月營收揭露 alpha + Sias 2004),
    Trigger 架構 C 級(自創 conviction adjustment),需實盤驗證。

    Returns:
        {as_of, signal_date, toolkit, stock_filter, counts: {positive_total, negative_total},
         positive_triggers: [...], negative_triggers: [...], narrative}
    """
    from mcp_server._screens import compute_monthly_trigger_scan

    return compute_monthly_trigger_scan(
        _parse_date(date),
        stock_id=stock_id, top_n_per_type=top_n_per_type,
        database_url=database_url,
    )


# ────────────────────────────────────────────────────────────
# Fusion Layer · Integration 端口 tools(P1.4)
# ────────────────────────────────────────────────────────────


def market_events(
    start_date: str,
    end_date: str,
    severity_min: str = "info",
    *,
    database_url: str | None = None,
) -> dict[str, Any]:
    """Fusion D 視角:大盤環境事件時間軸。

    撈 7 個 environment cores(taiex / us_market / exchange_rate / fear_greed /
    market_margin / business_indicator / commodity_macro)寫進 facts 的事件,
    依日期區間 [start_date, end_date] + 最低嚴重度 filter,以統一 Event schema
    回傳時間軸。

    severity_min:info / notable / warning / critical(預設 info = 全收)。
    嚴重度由各 core 寫入 fact 時決定,本層只 filter 不二次判斷。

    Returns:
        {start_date, end_date, severity_min, event_count, by_severity,
         events: [{date, source, kind, severity, statement, value, metadata}, ...]}
        events 依 (date DESC, severity DESC) 排序。
    """
    from fusion.market_events import market_events as _market_events

    return _market_events(
        _parse_date(start_date),
        _parse_date(end_date),
        severity_min=severity_min,
        database_url=database_url,
    )


def market_dashboard(
    date: str,
    *,
    database_url: str | None = None,
) -> dict[str, Any]:
    """Fusion D 視角:大盤環境快照。

    讀 7 個 environment cores(taiex / us_market / exchange_rate / fear_greed /
    market_margin / business_indicator / commodity_macro)的最新一筆,抽出各核心
    headline metric + 歷史百分位(percentile_252)+ 短期變化。

    純資料快照 — 不打主觀標籤,由 LLM 自行判讀大盤環境。

    Returns:
        {as_of, component_count, components, missing}
        每個 component:{latest_date, value, change_pct, percentile_252, state, ...}
    """
    from fusion.market_dashboard import market_dashboard as _market_dashboard

    return _market_dashboard(_parse_date(date), database_url=database_url)


def key_levels(
    stock_id: str,
    date: str,
    *,
    database_url: str | None = None,
) -> dict[str, Any]:
    """Fusion B 視角:個股關鍵支撐 / 壓力價位。

    整合三來源並以 1% bucket cluster:support_resistance_core(SR 價位)、
    trendline_core(有效趨勢線)、neely_core flat_fib_zones(Fibonacci 區)。
    strength = 該價位被幾個來源確認(越多越強)。

    Returns:
        {stock_id, as_of, source_point_count, level_count,
         levels: [{price, low, high, sources, strength, member_count}, ...]}
        levels 依 price 升序。
    """
    from fusion.key_levels import key_levels as _key_levels

    return _key_levels(stock_id, _parse_date(date), database_url=database_url)


def stop_loss_calc(
    stock_id: str,
    entry_price: float,
    date: str,
    direction: str = "long",
    atr_mult: float = 2.0,
    reward_risk_ratio: float = 2.0,
    *,
    database_url: str | None = None,
) -> dict[str, Any]:
    """Fusion B 視角:止損 / 止盈計算。

    給定進場價,整合 ATR(atr_core)+ key_levels(SR / 趨勢線 / Neely Fib)算出
    止損、止盈候選。純計算 — 同時呈現 ATR-based 與 level-based 候選 + 距離百分比,
    不替你抉擇(由 LLM 判讀)。

    direction:long(預設)或 short。atr_mult 為止損 ATR 倍數;reward_risk_ratio
    為 ATR 止盈相對止損的報酬風險比。

    Returns:
        {stock_id, as_of, direction, entry_price, atr, stops, targets}
        stops/targets 各含 atr_based + nearest_level,每筆 {price, distance,
        distance_pct}。
    """
    from fusion.stop_loss import stop_loss as _stop_loss

    return _stop_loss(
        stock_id, entry_price, _parse_date(date),
        direction=direction, atr_mult=atr_mult,
        reward_risk_ratio=reward_risk_ratio, database_url=database_url,
    )


def pattern_scan(
    stock_id: str,
    date: str,
    *,
    database_url: str | None = None,
) -> dict[str, Any]:
    """Fusion B 視角:近期 K 線型態 + 支撐 / 壓力 context。

    撈 candlestick_pattern_core 近期偵測到的 K 線型態,為每個型態補上
    key_levels context(型態發生價是否貼近支撐 / 壓力 — 同型態在支撐附近
    與在中段意義不同)。型態本身由 core 偵測,本層只整合 key_levels。

    Returns:
        {stock_id, as_of, pattern_count,
         patterns: [{date, pattern, trend_context, strength, price,
                     level_context}, ...]}  依 date 降序。
    """
    from fusion.pattern_scan import pattern_scan as _pattern_scan

    return _pattern_scan(stock_id, _parse_date(date), database_url=database_url)


def _assemble(category: str, stock_id: str, date: str,
              indicators: list[str] | None, lookback_days: int,
              database_url: str | None) -> dict[str, Any]:
    """E 視角 4 個子類工具共用:依 category 過濾 indicators 後組裝。"""
    from fusion.indicator_assembly import assemble_indicators, category_indicators

    return assemble_indicators(
        stock_id, _parse_date(date),
        category_indicators(category, indicators),
        lookback_days=lookback_days, database_url=database_url,
    )


def indicator_momentum(
    stock_id: str, date: str, indicators: list[str] | None = None,
    lookback_days: int = 60, *, database_url: str | None = None,
) -> dict[str, Any]:
    """Fusion E 視角:動量 / 趨勢 / 強度類指標(series + events)。

    可選 indicators:macd / rsi / kd / adx / ma / ichimoku / williams_r /
    cci / coppock(可帶或不帶 `_core` 後綴);省略 = 全部。

    Returns:
        {stock_id, as_of, indicator_count, indicators, missing}
        indicators[<core>] = {value_date, series, events}。
    """
    return _assemble("momentum", stock_id, date, indicators, lookback_days, database_url)


def indicator_volatility(
    stock_id: str, date: str, indicators: list[str] | None = None,
    lookback_days: int = 60, *, database_url: str | None = None,
) -> dict[str, Any]:
    """Fusion E 視角:波動 / 通道類指標(series + events)。

    可選 indicators:bollinger / keltner / donchian / atr;省略 = 全部。
    """
    return _assemble("volatility", stock_id, date, indicators, lookback_days, database_url)


def indicator_volume(
    stock_id: str, date: str, indicators: list[str] | None = None,
    lookback_days: int = 60, *, database_url: str | None = None,
) -> dict[str, Any]:
    """Fusion E 視角:量能類指標(series + events)。

    可選 indicators:obv / vwap / mfi;省略 = 全部。
    """
    return _assemble("volume", stock_id, date, indicators, lookback_days, database_url)


def indicator_pattern(
    stock_id: str, date: str, indicators: list[str] | None = None,
    lookback_days: int = 60, *, database_url: str | None = None,
) -> dict[str, Any]:
    """Fusion E 視角:型態 / 價位類指標(series + events)。

    可選 indicators:candlestick_pattern / support_resistance / trendline;
    省略 = 全部。
    """
    return _assemble("pattern", stock_id, date, indicators, lookback_days, database_url)


def indicator_stack(
    stock_id: str, date: str, preset: str = "default",
    lookback_days: int = 60, *, database_url: str | None = None,
) -> dict[str, Any]:
    """Fusion E 視角:預設指標組合(series + events)。

    preset:default(MACD+RSI+KD+Bollinger+MA)/ day_trade(KD+RSI+VWAP+
    Bollinger)/ swing(MACD+MA+ADX+ATR)/ position(MA+Ichimoku+OBV+SR)。

    Returns:
        {stock_id, as_of, indicator_count, indicators, missing}
    """
    from fusion.indicator_assembly import INDICATOR_STACK_PRESETS, assemble_indicators

    cores = INDICATOR_STACK_PRESETS.get(preset, INDICATOR_STACK_PRESETS["default"])
    return assemble_indicators(
        stock_id, _parse_date(date), cores,
        lookback_days=lookback_days, database_url=database_url,
    )


# ────────────────────────────────────────────────────────────
# Fusion Layer · Consolidated 入口(v4.19 — 10 fusion tools → 3)
#
# 對齊 stock_snapshot 6→1 整併 pattern。每段獨立 graceful degradation
# (某子工具失敗 → 該段 = {"error": ...},不影響其他段)。被整併的 10 個
# fusion function 仍留本檔(dashboard / direct python 用),只是不再 MCP 註冊。
# ────────────────────────────────────────────────────────────


def market_overview(
    date: str,
    events_lookback_days: int = 30,
    severity_min: str = "notable",
    *,
    database_url: str | None = None,
) -> dict[str, Any]:
    """Fusion D 視角:大盤環境總覽(整併 market_dashboard + market_events)。

    - dashboard:7 個 environment cores 最新 headline metric + 歷史百分位
    - events:[date - events_lookback_days, date] 區間環境事件時間軸

    輸出大小:events 預設 severity_min="notable"(濾掉 info 噪音)+ 30 天窗。
    要更早 / 更全 → 調大 events_lookback_days 或 severity_min="info"。

    Returns:
        {as_of, dashboard, events} — 某段失敗 → 該段 = {"error": ..., "section": ...}
    """
    from datetime import timedelta

    as_of = _parse_date(date)
    out: dict[str, Any] = {"as_of": as_of.isoformat()}

    try:
        from fusion.market_dashboard import market_dashboard as _md
        out["dashboard"] = _md(as_of, database_url=database_url)
    except Exception as e:  # noqa: BLE001
        out["dashboard"] = {"error": f"{type(e).__name__}: {e}", "section": "dashboard"}

    try:
        from fusion.market_events import market_events as _me
        start = as_of - timedelta(days=max(1, events_lookback_days))
        out["events"] = _me(start, as_of, severity_min=severity_min,
                            database_url=database_url)
    except Exception as e:  # noqa: BLE001
        out["events"] = {"error": f"{type(e).__name__}: {e}", "section": "events"}

    return out


def stock_levels(
    stock_id: str,
    date: str,
    entry_price: float | None = None,
    direction: str = "long",
    atr_mult: float = 2.0,
    reward_risk_ratio: float = 2.0,
    *,
    database_url: str | None = None,
) -> dict[str, Any]:
    """Fusion B 視角:個股價位總覽(整併 key_levels + pattern_scan + stop_loss_calc)。

    - key_levels:支撐 / 壓力(SR + 趨勢線 + Neely Fib,1% cluster)
    - patterns:近期 K 線型態 + 支撐 / 壓力 context
    - stop_loss:止損 / 止盈計算 — **僅當給 entry_price 才算**,否則 None

    Returns:
        {stock_id, as_of, key_levels, patterns, stop_loss} — 某段失敗 → 該段
        = {"error": ..., "section": ...};未給 entry_price → stop_loss = None。
    """
    as_of = _parse_date(date)
    out: dict[str, Any] = {"stock_id": stock_id, "as_of": as_of.isoformat()}

    try:
        from fusion.key_levels import key_levels as _kl
        out["key_levels"] = _kl(stock_id, as_of, database_url=database_url)
    except Exception as e:  # noqa: BLE001
        out["key_levels"] = {"error": f"{type(e).__name__}: {e}", "section": "key_levels"}

    try:
        from fusion.pattern_scan import pattern_scan as _ps
        out["patterns"] = _ps(stock_id, as_of, database_url=database_url)
    except Exception as e:  # noqa: BLE001
        out["patterns"] = {"error": f"{type(e).__name__}: {e}", "section": "patterns"}

    if entry_price is None:
        out["stop_loss"] = None
    else:
        try:
            from fusion.stop_loss import stop_loss as _sl
            out["stop_loss"] = _sl(
                stock_id, entry_price, as_of,
                direction=direction, atr_mult=atr_mult,
                reward_risk_ratio=reward_risk_ratio, database_url=database_url,
            )
        except Exception as e:  # noqa: BLE001
            out["stop_loss"] = {"error": f"{type(e).__name__}: {e}", "section": "stop_loss"}

    return out


def indicators(
    stock_id: str,
    date: str,
    groups: list[str] | None = None,
    cores: list[str] | None = None,
    preset: str | None = None,
    lookback_days: int = 60,
    *,
    database_url: str | None = None,
) -> dict[str, Any]:
    """Fusion E 視角:技術指標 series + events(整併 5 個 indicator_* 工具)。

    整併 indicator_momentum / volatility / volume / pattern / stack。選擇優先序:
      1. cores  — 明確 core 清單(如 ["macd","rsi","atr"];可省略 `_core` 後綴)
      2. groups — 子類清單,取自 {momentum, volatility, volume, pattern}
      3. preset — {default, day_trade, swing, position}
      4. 皆未給 → preset="default"(MACD+RSI+KD+Bollinger+MA,5 cores)

    輸出大小:預設只回 5 cores。`groups` 多選會把整類 cores 攤開(momentum 一類
    就 9 cores),series 隨之放大 — 多 group 請求請自行斟酌。

    Returns:
        {stock_id, as_of, selection, indicator_count, indicators, missing}
    """
    from fusion.indicator_assembly import (
        INDICATOR_STACK_PRESETS, assemble_indicators, category_indicators,
    )

    as_of = _parse_date(date)

    if cores:
        selected = [
            c if str(c).strip().lower().endswith("_core")
            else f"{str(c).strip().lower()}_core"
            for c in cores
        ]
        selection: dict[str, Any] = {"mode": "cores", "value": selected}
    elif groups:
        selected = []
        seen: set[str] = set()
        for g in groups:
            for core in category_indicators(str(g).strip().lower(), None):
                if core not in seen:
                    seen.add(core)
                    selected.append(core)
        selection = {"mode": "groups", "value": [str(g) for g in groups]}
    else:
        key = preset if preset in INDICATOR_STACK_PRESETS else "default"
        selected = list(INDICATOR_STACK_PRESETS[key])
        selection = {"mode": "preset", "value": key}

    result = assemble_indicators(
        stock_id, as_of, selected,
        lookback_days=lookback_days, database_url=database_url,
    )
    result["selection"] = selection
    return result


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
