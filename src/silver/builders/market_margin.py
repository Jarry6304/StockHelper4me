"""
silver/builders/market_margin.py
================================
market_margin_maintenance (legacy v2.0) → market_margin_maintenance_derived (Silver)。

PK = (market, date)市場級表。+2 衍生欄位 per spec §2.6.3:
  - total_margin_purchase_balance(整體市場融資餘額)
  - total_short_sale_balance(整體市場融券餘額)
這 2 欄需從 TotalMarginPurchaseShortSale API 抓(目前可能未在 collector.toml,
動工前要先 confirm 是否需新增 Bronze 表)。

留 **PR #19c** 動工。
"""

from __future__ import annotations

from typing import Any


NAME          = "market_margin"
SILVER_TABLE  = "market_margin_maintenance_derived"
BRONZE_TABLES = ["market_margin_maintenance"]   # v3.2 可能 rename + 新增 total_* 來源表


def run(
    db: Any,
    stock_ids: list[str] | None = None,
    full_rebuild: bool = False,
) -> dict[str, Any]:
    raise NotImplementedError(
        f"{NAME} builder 留 PR #19c 動工。+2 衍生欄需確認 Bronze 來源。"
    )
