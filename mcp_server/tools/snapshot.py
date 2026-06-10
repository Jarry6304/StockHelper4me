"""個股快照域 tools — stock_snapshot 10-in-1 與其 v3.31 整併的元件函式 + 大盤判讀。

元件函式(stock_health / market_context / loan_collateral_snapshot /
block_trade_summary / risk_alert_status / commodity_macro_snapshot)預設不註冊
MCP,留給 dashboard / direct python;註冊見 server.py mcp.tool() 區塊。
"""

from __future__ import annotations

from typing import Any

from mcp_server.tools._shared import _parse_date, _read_materialized_snapshot


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
    as_of = _parse_date(date)
    # v4.32 Golden L3:預設 lookback 讀物化 climate_fusion;非預設 → compute fresh
    if lookback_days == 60:
        doc = _read_materialized_snapshot(
            "_market_", as_of, "climate_fusion", timeframe="_all_",
        )
        if doc is not None:
            return doc

    from mcp_server._climate import compute_market_context

    return compute_market_context(as_of, lookback_days=lookback_days)


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
