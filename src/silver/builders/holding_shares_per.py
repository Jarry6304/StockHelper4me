"""
silver/builders/holding_shares_per.py
=====================================
holding_shares_per_tw (Bronze) → holding_shares_per_derived (Silver)。

Bronze 是長 row(每筆 1 個 HoldingSharesLevel),Silver 才 pack 成 detail JSONB
(對應 src/aggregators.py:aggregate_holding_shares 的正向 pack 邏輯)。

留 **PR #19c** 動工。Bronze 來源 holding_shares_per_tw 走 PR #18.5 Option A
全量重抓(detail JSONB unpack 不可逆),需等用戶本機跑完 30-40h FinMind 重抓
才有真資料可驗。動工前需先 confirm PR #18.5 已 merge。
"""

from __future__ import annotations

from typing import Any


NAME          = "holding_shares_per"
SILVER_TABLE  = "holding_shares_per_derived"
BRONZE_TABLES = ["holding_shares_per_tw"]


def run(
    db: Any,
    stock_ids: list[str] | None = None,
    full_rebuild: bool = False,
) -> dict[str, Any]:
    raise NotImplementedError(
        f"{NAME} builder 留 PR #19c 動工。Bronze 來源依賴 PR #18.5 重抓完成。"
    )
