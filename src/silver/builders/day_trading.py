"""
silver/builders/day_trading.py
==============================
day_trading_tw (Bronze) → day_trading_derived (Silver)。

7a 階段 raw 1:1 複製;7b 階段補 day_trading_ratio = day_trading_buy / price_daily.volume
(需先等 Phase 7c price_daily_fwd 算完才能算 ratio,故拆 7a/7b 兩段)。

留 **PR #19b** 補 7a 直拷邏輯;7b ratio 留 PR #19c 跟 orchestrator 一起動工。
"""

from __future__ import annotations

from typing import Any


NAME          = "day_trading"
SILVER_TABLE  = "day_trading_derived"
BRONZE_TABLES = ["day_trading_tw"]


def run(
    db: Any,
    stock_ids: list[str] | None = None,
    full_rebuild: bool = False,
) -> dict[str, Any]:
    raise NotImplementedError(
        f"{NAME} builder 留 PR #19b 動工。7a 階段 raw 直拷 day_trading_tw → "
        f"day_trading_derived;ratio 衍生欄留 PR #19c 7b 階段補。"
    )
