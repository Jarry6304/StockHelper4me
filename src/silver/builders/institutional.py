"""
silver/builders/institutional.py
================================
institutional_investors_tw (Bronze) → institutional_daily_derived (Silver)。

正向 pivot:per spec §2.6.2 + 對應 src/aggregators.py:aggregate_institutional 邏輯,
                每日 5 行 institutional_investors_tw → 1 行 institutional_daily_derived
                (10 個 buy/sell 欄 + gov_bank_net 1 欄)。
gov_bank_net:八大行庫淨買賣(blueprint §2.6.2),來源 GovernmentBankBuySell API
                — 留 PR #19b 動工時決定:走獨立 ref 表 join 或 Bronze 直存。

留 **PR #19b** 補實作(institutional Bronze 已 PR #18 落地有 26625 列真資料,
可用 verify_pr18_bronze 同樣的 round-trip 思路驗 builder 對 institutional_daily 等值)。
"""

from __future__ import annotations

from typing import Any


NAME          = "institutional"
SILVER_TABLE  = "institutional_daily_derived"
BRONZE_TABLES = ["institutional_investors_tw"]


def run(
    db: Any,
    stock_ids: list[str] | None = None,
    full_rebuild: bool = False,
) -> dict[str, Any]:
    raise NotImplementedError(
        f"{NAME} builder 留 PR #19b 動工。Bronze 已落地(PR #18 institutional_investors_tw),"
        f"可參考 src/aggregators.py:aggregate_institutional 正向 pivot 邏輯。"
    )
