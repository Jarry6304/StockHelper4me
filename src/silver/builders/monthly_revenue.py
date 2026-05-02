"""
silver/builders/monthly_revenue.py
==================================
monthly_revenue_tw (Bronze) → monthly_revenue_derived (Silver)。

Bronze→Silver 1:1 直拷 + 7b 階段 financial_statement 對齊用。

留 **PR #19c** 動工(Bronze 來源依賴 PR #18.5 重抓 — FinMind 月營收只回 1 row/股/月,
無法 reverse-pivot)。
"""

from __future__ import annotations

from typing import Any


NAME          = "monthly_revenue"
SILVER_TABLE  = "monthly_revenue_derived"
BRONZE_TABLES = ["monthly_revenue_tw"]


def run(
    db: Any,
    stock_ids: list[str] | None = None,
    full_rebuild: bool = False,
) -> dict[str, Any]:
    raise NotImplementedError(
        f"{NAME} builder 留 PR #19c 動工。Bronze 來源依賴 PR #18.5 重抓。"
    )
