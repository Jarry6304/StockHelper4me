"""
silver/builders/institutional.py
================================
institutional_investors_tw (Bronze) → institutional_daily_derived (Silver)。

Pivot 邏輯:對 (market, stock_id, date) 把 Bronze 多 row(每 investor_type 一筆)
合成 1 寬 row,10 個 buy/sell 欄位 + gov_bank_net 1 欄。

對齊 src/aggregators.py:aggregate_institutional 的正向 pivot(同 INSTITUTIONAL_NAME_MAP
但 Bronze 已經用英文 key 了,investor_type ∈ {Foreign_Investor, Foreign_Dealer_Self,
Investment_Trust, Dealer, Dealer_Hedging})。

gov_bank_net(per spec §2.6.2)八大行庫淨買賣:
- 來源 GovernmentBankBuySell API(目前 collector.toml 沒接,Bronze 不存)
- PR #19b stub 階段:寫 NULL(欄位存在但無資料)
- 留 PR #19c 接 government_bank_tw Bronze 後 join 補

驗證(用戶本機):
    python scripts/verify_pr19b_silver.py
    對 institutional 跑 round-trip 比對 institutional_daily(v2.0 legacy)
    10 個 buy/sell 欄應 100% 等值;gov_bank_net 兩邊都 NULL(暫不比)。
"""

from __future__ import annotations

import logging
import time
from typing import Any

from .._common import filter_to_trading_days, get_trading_dates, fetch_bronze, upsert_silver


logger = logging.getLogger("collector.silver.builders.institutional")


NAME          = "institutional"
SILVER_TABLE  = "institutional_daily_derived"
BRONZE_TABLES = ["institutional_investors_tw"]


# investor_type → (buy 欄, sell 欄)— Bronze investor_type 已是英文 key
INVESTOR_TYPE_MAP: dict[str, tuple[str, str]] = {
    "Foreign_Investor":     ("foreign_buy",              "foreign_sell"),
    "Foreign_Dealer_Self":  ("foreign_dealer_self_buy",  "foreign_dealer_self_sell"),
    "Investment_Trust":     ("investment_trust_buy",     "investment_trust_sell"),
    "Dealer":               ("dealer_buy",               "dealer_sell"),
    "Dealer_Hedging":       ("dealer_hedging_buy",       "dealer_hedging_sell"),
}


def _pivot(
    bronze_rows: list[dict[str, Any]],
    trading_dates: set[str],
) -> list[dict[str, Any]]:
    """Bronze 多 row → Silver 1 寬 row。"""
    if trading_dates:
        bronze_rows = filter_to_trading_days(bronze_rows, trading_dates, label=NAME)

    grouped: dict[tuple, dict[str, Any]] = {}
    for row in bronze_rows:
        key = (row.get("market"), row.get("stock_id"), row.get("date"))
        if key not in grouped:
            agg = {
                "market":   row.get("market"),
                "stock_id": row.get("stock_id"),
                "date":     row.get("date"),
            }
            for buy_col, sell_col in INVESTOR_TYPE_MAP.values():
                agg[buy_col]  = None
                agg[sell_col] = None
            agg["gov_bank_net"] = None  # PR #19c 才接
            grouped[key] = agg

        inv_type = row.get("investor_type", "")
        cols = INVESTOR_TYPE_MAP.get(inv_type)
        if cols:
            buy_col, sell_col = cols
            grouped[key][buy_col]  = row.get("buy")
            grouped[key][sell_col] = row.get("sell")
        else:
            logger.warning(
                f"未知 investor_type='{inv_type}' "
                f"(market={key[0]}, stock_id={key[1]}, date={key[2]}),已略過"
            )

    return list(grouped.values())


def run(
    db: Any,
    stock_ids: list[str] | None = None,
    full_rebuild: bool = False,
) -> dict[str, Any]:
    """跑 institutional builder。

    Args:
        db:           DBWriter
        stock_ids:    None = 全市場
        full_rebuild: True = 全部 Bronze 重算(目前唯一支援的模式;dirty queue
                      pull 留 PR #19c orchestrator 動工)

    Returns:
        {name, rows_read, rows_written, elapsed_ms}
    """
    start = time.monotonic()

    trading_dates = get_trading_dates(db)
    bronze = fetch_bronze(db, "institutional_investors_tw", stock_ids=stock_ids)
    silver = _pivot(bronze, trading_dates)

    written = upsert_silver(
        db, SILVER_TABLE, silver,
        pk_cols=["market", "stock_id", "date"],
    )

    elapsed_ms = int((time.monotonic() - start) * 1000)
    logger.info(
        f"[{NAME}] read={len(bronze)} bronze rows → "
        f"wrote={written} silver rows(elapsed={elapsed_ms}ms)"
    )
    return {
        "name":         NAME,
        "rows_read":    len(bronze),
        "rows_written": written,
        "elapsed_ms":   elapsed_ms,
    }
