"""
silver/builders/__init__.py
============================
14 個 Silver `*_derived` builder 註冊入口。

PR #19a 落地 14 個 stub(本目錄下),全都 raise NotImplementedError。
PR #19b 補 5 個簡單 builder 邏輯(Bronze 已 PR #18 落地的 5 張)。
PR #19c 補剩 9 個 builder + orchestrator 真實邏輯。

註冊規則:每個 builder 模組 expose 一個 BUILDER 變數(SilverBuilder protocol),
orchestrator 透過 `BUILDERS[name]` 拿。

對映 spec §2.3 14 張 Silver 表(price_limit_merge_events 走 Rust 不在這裡):
"""

from __future__ import annotations

from . import (
    business_indicator,
    day_trading,
    exchange_rate,
    financial_statement,
    foreign_holding,
    holding_shares_per,
    institutional,
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
    # price_limit_merge_events 不在這裡;Rust 計算走 rust_bridge,Phase 7c
}
