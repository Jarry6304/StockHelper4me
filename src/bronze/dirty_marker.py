"""
bronze/dirty_marker.py
======================
Bronze→Silver dirty marker。**PR #20 起 deprecated**:DB-side trigger
`trg_mark_silver_dirty`(alembic n3o4p5q6r7s8 / blueprint §5.5)接管,
本模組改 no-op + deprecation log,留 1~2 sprint 相容期供 emergency manual ops。

歷史:
  PR #19a:留 stub(API surface 定下,真實邏輯 PR #19b/#19c 接)
  PR #19b/#19c:從未實作,因 PR #20 trigger 上線同步把 DB-side path 接管
  PR #20:改 no-op + deprecation log;呼叫端應改用 trigger(自動)或直接
         在 Silver `*_derived` 表 UPDATE is_dirty / dirty_at

完整 Bronze ↔ Silver 對映表 + PK shape 變體見 alembic migration
`2026_05_04_n3o4p5q6r7s8_pr20_silver_dirty_triggers.py` header。
"""

from __future__ import annotations

import logging
import warnings
from typing import Any

logger = logging.getLogger("collector.bronze.dirty_marker")


# 保留對映表給呼叫端 introspection / verifier 用(只是參考資料,trigger 才是
# 真正的 source of truth)。對映語意對齊 PR #20 trigger;一 Bronze 對多 Silver
# 走 list(目前只有 price_adjustment_events 觸發 fwd 4 表)。
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
    """**DEPRECATED (PR #20)** — DB trigger 接管,本函式 no-op。

    PR #20 alembic n3o4p5q6r7s8 把 14 + 1 個 Bronze→Silver dirty trigger 接上,
    Bronze upsert 完成後 trigger 自動 mark Silver row dirty,Python 端不再需要
    顯式呼叫。本函式留 1~2 sprint 相容期(呼叫時 emit DeprecationWarning),
    PR #21 後完全移除。

    Args 維持 PR #19a 簽名以避免破壞既存呼叫端。
    """
    warnings.warn(
        "bronze.dirty_marker.mark_silver_dirty 已 deprecated(PR #20):"
        "DB trigger trg_mark_silver_dirty / trg_mark_fwd_silver_dirty 接管,"
        "本函式 no-op。PR #21 移除。",
        DeprecationWarning,
        stacklevel=2,
    )
    if bronze_table not in BRONZE_TO_SILVER:
        # 容錯:既存呼叫端傳未知 table name 時 log warn 不 raise(deprecated 階段不阻斷)
        logger.warning(
            f"[dirty_marker deprecated] 未知 bronze_table='{bronze_table}' "
            f"(已知:{sorted(BRONZE_TO_SILVER)})— 已忽略,DB trigger 是真實 source of truth"
        )
        return 0
    logger.debug(
        f"[dirty_marker deprecated no-op] bronze={bronze_table} rows={len(rows)} "
        f"full_history_fwd={full_history_fwd} — DB trigger 已處理"
    )
    return 0
