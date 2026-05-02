"""
silver/builders/financial_statement.py
======================================
financial_statement_tw (Bronze) → financial_statement_derived (Silver)。

7b 階段 builder:需 monthly_revenue_derived 對齊(財報季度 vs 月營收日期映射),
故跑在 Phase 7a 之後。對應 src/aggregators.py:aggregate_financial 的正向 pack
(每筆 1 個會計科目 → 1 row per (date, stock_id, type) + detail JSONB)。

留 **PR #19c** 動工(Bronze 來源依賴 PR #18.5 重抓 — 中→英 origin_name 對應丟失,
無法從 v2.0 detail JSONB 反推)。
"""

from __future__ import annotations

from typing import Any


NAME          = "financial_statement"
SILVER_TABLE  = "financial_statement_derived"
BRONZE_TABLES = ["financial_statement_tw"]


def run(
    db: Any,
    stock_ids: list[str] | None = None,
    full_rebuild: bool = False,
) -> dict[str, Any]:
    raise NotImplementedError(
        f"{NAME} builder 留 PR #19c 動工(7b 階段)。Bronze 依賴 PR #18.5 重抓。"
    )
