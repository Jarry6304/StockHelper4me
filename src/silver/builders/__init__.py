"""
silver/builders/__init__.py
============================
Silver `*_derived` per-stock builder 註冊入口(v3.5 R2 收緊 per-stock 邊界)。

v3.5 R2:`SilverBuilder` Protocol 加 per-stock 契約明文(see silver/_common.py)。
v3.5 R3:magic_formula_ranked 搬出到 `src/cross_cores/`(違反 per-stock 契約,
        屬於 cross-stock 排名;CrossStockBuilder ABC 接管)。

註冊規則:每個 builder 模組 expose 模組級 NAME / SILVER_TABLE / BRONZE_TABLES /
run() 接口(對齊 SilverBuilder protocol),orchestrator 透過 `BUILDERS[name]` 拿。

對映 spec §2.3 14 張 Silver 表(price_limit_merge_events 走 Rust 不在這裡):
"""

from __future__ import annotations

from . import (
    block_trade,
    business_indicator,
    commodity_macro,
    day_trading,
    exchange_rate,
    financial_statement,
    foreign_holding,
    holding_shares_per,
    institutional,
    loan_collateral,
    margin,
    market_margin,
    monthly_revenue,
    taiex_index,
    us_market_index,
    valuation,
)


# Builder 註冊表:name → module(orchestrator 用 module.BUILDER 取 instance)
BUILDERS: dict[str, object] = {
    "institutional":         institutional,
    "margin":                margin,
    "foreign_holding":       foreign_holding,
    "holding_shares_per":    holding_shares_per,
    "valuation":             valuation,
    "day_trading":           day_trading,
    "monthly_revenue":       monthly_revenue,
    "financial_statement":   financial_statement,
    "taiex_index":           taiex_index,
    "us_market_index":       us_market_index,
    "exchange_rate":         exchange_rate,
    "market_margin":         market_margin,
    "business_indicator":    business_indicator,
    # v3.21 新加 3 個 builders
    "loan_collateral":       loan_collateral,    # loan_collateral_balance_derived
    "block_trade":           block_trade,        # block_trade_derived
    "commodity_macro":       commodity_macro,    # commodity_price_daily_derived
    # price_limit_merge_events 不在這裡;Rust 計算走 rust_bridge,Phase 7c
    # magic_formula_ranked:v3.5 R3 搬到 cross_cores/(per-stock 契約違規)
    # risk_alert 不需 Silver derived(直讀 Bronze,§十二 對齊 fear_greed 例外)
}
