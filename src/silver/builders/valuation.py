"""
silver/builders/valuation.py
============================
valuation_per_tw (Bronze) → valuation_daily_derived (Silver)。

Bronze 3 raw 欄(per/pbr/dividend_yield)+ 衍生 1 欄 market_value_weight
(spec §2.6.4 個股佔大盤市值比重,= close × NumberOfSharesIssued / 大盤總市值)。

留 **PR #19b** 補實作(Bronze 已 PR #18 落地;market_value_weight 需 join
price_daily 與 stock_info_ref 的發行股數,動工時決定是否走 view 即時算)。
"""

from __future__ import annotations

from typing import Any


NAME          = "valuation"
SILVER_TABLE  = "valuation_daily_derived"
BRONZE_TABLES = ["valuation_per_tw"]


def run(
    db: Any,
    stock_ids: list[str] | None = None,
    full_rebuild: bool = False,
) -> dict[str, Any]:
    raise NotImplementedError(
        f"{NAME} builder 留 PR #19b 動工。3 raw 欄 1:1 + market_value_weight 衍生。"
    )
