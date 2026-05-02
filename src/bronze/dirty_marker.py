"""
bronze/dirty_marker.py
======================
Bronze 寫入後在 Silver dirty queue 標記(per blueprint §三 + §5.5)。

兩條路徑(blueprint §5.7):

  **短期(PR #19a~PR #20 之間)**
    Bronze upsert 完成 → Python code 顯式呼叫 mark_silver_dirty(...)
    → INSERT/UPDATE Silver 對應表 is_dirty=TRUE / dirty_at=NOW()
    本檔 PR #19a 留 stub(API surface 定下,真實邏輯 PR #19b/#19c 接);
    PR #19c 整合進 src/phase_executor.py 的 Bronze 寫入路徑。

  **長期(PR #20 trigger 上線後)**
    DB-side trigger trg_mark_silver_dirty(blueprint §5.5)接管;
    Python 端 mark_silver_dirty 改為 no-op 或 deprecated。

對映表(Bronze → Silver)per spec §5.5 + §2.3:

  institutional_investors_tw     → institutional_daily_derived
  margin_purchase_short_sale_tw  → margin_daily_derived
  securities_lending_tw          → margin_daily_derived(整合 SBL)
  foreign_investor_share_tw      → foreign_holding_derived
  holding_shares_per_tw          → holding_shares_per_derived
  day_trading_tw                 → day_trading_derived
  valuation_per_tw               → valuation_daily_derived
  monthly_revenue_tw             → monthly_revenue_derived
  financial_statement_tw         → financial_statement_derived
  market_ohlcv_tw                → taiex_index_derived
  market_index_us                → us_market_index_derived  (TBD rename)
  exchange_rate                  → exchange_rate_derived    (TBD rename)
  market_margin_maintenance      → market_margin_maintenance_derived  (TBD rename)
  business_indicator_tw          → business_indicator_derived
  price_adjustment_events        → price_*_fwd + price_limit_merge_events
                                    (Phase 4 Rust 後復權專屬;blueprint §5.5 fwd trigger)
"""

from __future__ import annotations

import logging
from typing import Any

logger = logging.getLogger("collector.bronze.dirty_marker")


# Bronze → 對應 Silver 表(同 PK / 1:1 映射;一 Bronze 對多 Silver 走多 entry list)
BRONZE_TO_SILVER: dict[str, list[str]] = {
    "institutional_investors_tw":     ["institutional_daily_derived"],
    "margin_purchase_short_sale_tw":  ["margin_daily_derived"],
    "securities_lending_tw":          ["margin_daily_derived"],
    "foreign_investor_share_tw":      ["foreign_holding_derived"],
    "holding_shares_per_tw":          ["holding_shares_per_derived"],
    "day_trading_tw":                 ["day_trading_derived"],
    "valuation_per_tw":               ["valuation_daily_derived"],
    "monthly_revenue_tw":             ["monthly_revenue_derived"],
    "financial_statement_tw":         ["financial_statement_derived"],
    "market_ohlcv_tw":                ["taiex_index_derived"],
    "market_index_us":                ["us_market_index_derived"],
    "exchange_rate":                  ["exchange_rate_derived"],
    "market_margin_maintenance":      ["market_margin_maintenance_derived"],
    "business_indicator_tw":          ["business_indicator_derived"],
    # 後復權路徑:price_adjustment_events 寫入 → 該 stock_id 全段歷史 fwd 全 dirty
    # (因 multiplier 倒推,新除權息會回頭改全段歷史值)— blueprint §5.5
    "price_adjustment_events":        ["price_daily_fwd", "price_weekly_fwd",
                                        "price_monthly_fwd", "price_limit_merge_events"],
}


def mark_silver_dirty(
    db: Any,
    bronze_table: str,
    rows: list[dict[str, Any]],
    *,
    full_history_fwd: bool = False,
) -> int:
    """Bronze 寫入後標 Silver dirty。

    Args:
        db:               DBWriter
        bronze_table:     Bronze 表名,需在 BRONZE_TO_SILVER 內
        rows:             剛寫入 Bronze 的 row dict list(取 PK 用)
        full_history_fwd: True 時,對 price_*_fwd 走「該 stock_id 全段歷史 dirty」
                          (僅 price_adjustment_events 寫入時 = True;對應 blueprint
                          §5.5 trg_mark_fwd_silver_dirty)

    Returns:
        標 dirty 的列數(0 表示 PR #19a stub 階段未實作)

    Raises:
        ValueError: bronze_table 不在 BRONZE_TO_SILVER 對映表內

    Note:
        PR #19a 階段:此函式僅做參數驗證 + log,不真實寫 DB。
        PR #19b/#19c 才把實際 INSERT/UPDATE 邏輯接上;
        PR #20 trigger 上線後改成 no-op + deprecation log。
    """
    if bronze_table not in BRONZE_TO_SILVER:
        raise ValueError(
            f"bronze_table='{bronze_table}' 不在 BRONZE_TO_SILVER 對映表。"
            f"已知:{sorted(BRONZE_TO_SILVER)}"
        )

    silver_targets = BRONZE_TO_SILVER[bronze_table]
    logger.debug(
        f"[dirty_marker stub] bronze={bronze_table} rows={len(rows)} "
        f"silver_targets={silver_targets} full_history_fwd={full_history_fwd}"
    )

    # PR #19a stub:no-op。實作留 PR #19b 起。
    return 0
