"""
silver/builders/us_market_index.py
==================================
market_index_us (legacy v2.0) → us_market_index_derived (Silver)。

注意:目前 Bronze 來源是 v2.0 `market_index_us`(SPY / ^VIX),v3.2 計畫 rename
為 `us_market_index_tw` 但尚未 rename(暫保留 v2.0 名)。動工時若已 rename,
將 BRONZE_TABLES 更新即可。

留 **PR #19c** 動工。
"""

from __future__ import annotations

from typing import Any


NAME          = "us_market_index"
SILVER_TABLE  = "us_market_index_derived"
BRONZE_TABLES = ["market_index_us"]   # v3.2 後可能 rename 為 us_market_index_tw


def run(
    db: Any,
    stock_ids: list[str] | None = None,
    full_rebuild: bool = False,
) -> dict[str, Any]:
    raise NotImplementedError(
        f"{NAME} builder 留 PR #19c 動工。"
    )
