"""Tool 7 內部演算法:`block_trade_summary` — 大宗交易摘要。

對齊 m3Spec/chip_cores.md §十一(v3.21 拍版)+ v3.22 MCP B-5。

設計:
- 直接 SELECT block_trade_derived(Silver,SUM by trade_type per stock,date)
- 30 天期間統計:total volume / value / 配對 share / 異常配對日
- payload ~ 1.5 KB / ~400 tokens

呼叫端:`mcp_server.tools.data.block_trade_summary()`。

Reference:
  - Cao, Field & Hanka (2009), "Block Trading and Stock Prices"
    *Journal of Empirical Finance* 16:1-25 — matched share > 0.80 視為異常。
"""

from __future__ import annotations

from datetime import date, timedelta
from typing import Any

from agg._db import get_connection


MATCHING_SPIKE_THRESHOLD = 0.80


def compute_block_trade_summary(
    stock_id: str,
    as_of: date,
    *,
    lookback_days: int = 30,
    database_url: str | None = None,
) -> dict[str, Any]:
    """大宗交易 30 天摘要 + 配對交易 spike 標記。

    Args:
        stock_id:      股票代號
        as_of:         查詢日上界
        lookback_days: 期間天數(預設 30)
        database_url:  可選 PG 連線字串

    Returns:
        dict ~1.5 KB:
          {
            "stock_id": "2330",
            "as_of": "2026-05-15",
            "period_days": 30,
            "active_days": 12,
            "total_volume": 152300,
            "total_trading_money": 162000000,
            "matching_share_avg": 0.65,
            "largest_single_trade_money": 80000000,
            "matching_spike_dates": ["2026-04-25"],
            "narrative": "..."
          }
    """
    conn = get_connection(database_url)
    period_start = as_of - timedelta(days=lookback_days)
    try:
        with conn.cursor() as cur:
            cur.execute(
                """
                SELECT date,
                       total_volume, total_trading_money,
                       matching_volume, matching_trading_money, matching_share,
                       largest_single_trade_money, trade_type_count
                  FROM block_trade_derived
                 WHERE stock_id = %s AND date <= %s AND date >= %s
                 ORDER BY date ASC
                """,
                [stock_id, as_of, period_start],
            )
            rows = cur.fetchall() or []
    finally:
        conn.close()

    if not rows:
        return _empty_result(stock_id, as_of, lookback_days)

    active_days = len(rows)
    total_volume = sum(int(r.get("total_volume") or 0) for r in rows)
    total_money = sum(int(r.get("total_trading_money") or 0) for r in rows)
    matching_vol = sum(int(r.get("matching_volume") or 0) for r in rows)
    matching_money = sum(int(r.get("matching_trading_money") or 0) for r in rows)
    matching_share_avg = (matching_vol / total_volume) if total_volume > 0 else None
    largest_single = max(
        (int(r.get("largest_single_trade_money") or 0) for r in rows), default=0,
    )

    # MatchingTradeSpike: 單日 matching_share >= 0.80
    spike_dates = [
        r["date"].isoformat()
        for r in rows
        if (r.get("matching_share") or 0.0) >= MATCHING_SPIKE_THRESHOLD
    ]

    return {
        "stock_id":                  stock_id,
        "as_of":                     as_of.isoformat(),
        "period_days":               lookback_days,
        "active_days":               active_days,
        "total_volume":              total_volume,
        "total_trading_money":       total_money,
        "matching_share_avg":        _round(matching_share_avg, 4),
        "largest_single_trade_money": largest_single,
        "matching_spike_dates":      spike_dates[:10],   # truncate budget
        "narrative": _compose_narrative(
            stock_id=stock_id, active_days=active_days, period_days=lookback_days,
            total_money=total_money, matching_share_avg=matching_share_avg,
            spike_dates=spike_dates,
        ),
    }


def _empty_result(stock_id: str, as_of: date, lookback_days: int) -> dict[str, Any]:
    return {
        "stock_id":                  stock_id,
        "as_of":                     as_of.isoformat(),
        "period_days":               lookback_days,
        "active_days":               0,
        "total_volume":              0,
        "total_trading_money":       0,
        "matching_share_avg":        None,
        "largest_single_trade_money": 0,
        "matching_spike_dates":      [],
        "narrative": (
            f"{stock_id} 過去 {lookback_days} 日無大宗交易資料"
            f"(<= {as_of.isoformat()})。可能尚未 backfill 或本期無大宗成交。"
        ),
    }


def _round(v: float | None, digits: int) -> float | None:
    if v is None:
        return None
    return round(float(v), digits)


def _compose_narrative(
    *, stock_id: str, active_days: int, period_days: int,
    total_money: int, matching_share_avg: float | None,
    spike_dates: list[str],
) -> str:
    if active_days == 0:
        return f"{stock_id} 過去 {period_days} 日無大宗交易。"

    share_phrase = ""
    if matching_share_avg is not None:
        share_phrase = f",配對交易主導 {matching_share_avg * 100:.0f}%"

    spike_phrase = ""
    if spike_dates:
        spike_phrase = f";{len(spike_dates)} 日配對佔比 ≥ 80% 異常("
        spike_phrase += ", ".join(spike_dates[:3])
        if len(spike_dates) > 3:
            spike_phrase += f" 等 {len(spike_dates)} 日"
        spike_phrase += ")"

    return (
        f"{stock_id} {period_days} 日內 {active_days} 日有大宗成交"
        f",總金額 {total_money / 1e8:.2f} 億{share_phrase}{spike_phrase}。"
    )
