"""
silver/builders/foreign_holding.py
==================================
foreign_investor_share_tw (Bronze) → foreign_holding_derived (Silver)。

最簡單一張(無衍生計算,等同 Bronze→Silver 1:1 同 PK 同欄複製),只 reset dirty。

留 **PR #19b** 補實作(Bronze 已 PR #18 落地)。
"""

from __future__ import annotations

from typing import Any


NAME          = "foreign_holding"
SILVER_TABLE  = "foreign_holding_derived"
BRONZE_TABLES = ["foreign_investor_share_tw"]


def run(
    db: Any,
    stock_ids: list[str] | None = None,
    full_rebuild: bool = False,
) -> dict[str, Any]:
    raise NotImplementedError(
        f"{NAME} builder 留 PR #19b 動工。Bronze→Silver 1:1 直拷,reset dirty。"
    )
