"""
silver/builders/business_indicator.py
=====================================
business_indicator_tw (Bronze) → business_indicator_derived (Silver)。

景氣指標(月頻 reference 級別,非 Core)。Bronze 已 PR #14 落地(B-6)。
spec §6.3:Silver 等同 Bronze 加 dirty,**不做衍生計算**(streak/3m_avg/changed
都由 Aggregation Layer 即時算)。

注意:Bronze 欄名 `leading_indicator` / `coincident_indicator` / `lagging_indicator`
       (避 PG 保留字)→ Silver 對應欄名 `leading` / `coincident` / `lagging`
       (per spec §6.3 DDL)。Builder 需做 rename。

留 **PR #19c** 動工。
"""

from __future__ import annotations

from typing import Any


NAME          = "business_indicator"
SILVER_TABLE  = "business_indicator_derived"
BRONZE_TABLES = ["business_indicator_tw"]


def run(
    db: Any,
    stock_ids: list[str] | None = None,
    full_rebuild: bool = False,
) -> dict[str, Any]:
    raise NotImplementedError(
        f"{NAME} builder 留 PR #19c 動工。Bronze leading_indicator → Silver leading rename。"
    )
