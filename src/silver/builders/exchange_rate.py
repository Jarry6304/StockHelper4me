"""
silver/builders/exchange_rate.py
================================
exchange_rate (legacy v2.0) → exchange_rate_derived (Silver)。

注意 PK 含 currency 維度(market, date, currency)— 不是 (market, stock_id, date)。
Bronze→Silver 1:1 直拷;v3.2 計畫 rename 為 exchange_rate_tw 但尚未 rename。

留 **PR #19c** 動工。
"""

from __future__ import annotations

from typing import Any


NAME          = "exchange_rate"
SILVER_TABLE  = "exchange_rate_derived"
BRONZE_TABLES = ["exchange_rate"]   # v3.2 後可能 rename 為 exchange_rate_tw


def run(
    db: Any,
    stock_ids: list[str] | None = None,
    full_rebuild: bool = False,
) -> dict[str, Any]:
    raise NotImplementedError(
        f"{NAME} builder 留 PR #19c 動工(注意 PK 含 currency 維度)。"
    )
