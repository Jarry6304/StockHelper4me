"""
silver/builders/margin.py
=========================
margin_purchase_short_sale_tw + securities_lending_tw (Bronze) →
                                margin_daily_derived (Silver)。

整合 SBL 6 欄(per spec §2.6.1):融券關鍵 3 欄(short_sales / short_covering /
current_day_balance)+ 借券關鍵 3 欄(short_sales / returns / current_day_balance)。

留 **PR #19b** 補實作(margin Bronze 已 PR #18 落地;securities_lending_tw 是 v3.2
B-5 新表,需確認 user 本機是否已抓資料 — 若無,builder 該段 fallback 到 NULL)。
"""

from __future__ import annotations

from typing import Any


NAME          = "margin"
SILVER_TABLE  = "margin_daily_derived"
BRONZE_TABLES = ["margin_purchase_short_sale_tw", "securities_lending_tw"]


def run(
    db: Any,
    stock_ids: list[str] | None = None,
    full_rebuild: bool = False,
) -> dict[str, Any]:
    raise NotImplementedError(
        f"{NAME} builder 留 PR #19b 動工。整合 margin_purchase_short_sale_tw + "
        f"securities_lending_tw 到 margin_daily_derived(SBL 6 欄 per §2.6.1)。"
    )
