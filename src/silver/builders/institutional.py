"""
silver/builders/institutional.py
================================
institutional_investors_tw + government_bank_buy_sell_tw (Bronze) →
                              institutional_daily_derived (Silver)。

Pivot 邏輯:對 (market, stock_id, date) 把 Bronze 多 row(每 investor_type 一筆)
合成 1 寬 row,10 個 buy/sell 欄位 + gov_bank_net 1 欄。

對齊 src/aggregators.py:aggregate_institutional 的正向 pivot(同 INSTITUTIONAL_NAME_MAP
但 Bronze 已經用英文 key 了,investor_type ∈ {Foreign_Investor, Foreign_Dealer_Self,
Investment_Trust, Dealer, Dealer_Hedging})。

gov_bank_net(per spec §2.6.2)八大行庫淨買賣:
- 來源 government_bank_buy_sell_tw Bronze(v3.14 alembic a6b7c8d9e0f1 後加
  bank_name 維度:8 行庫每股每日各 1 row + buy/sell/buy_amount/sell_amount)
- gov_bank_net = SUM(buy) - SUM(sell) GROUP BY (market, stock_id, date)
  (跨 8 行庫 net 股數;NULL 視同 0,對齊 SQL SUM 行為)
- LEFT JOIN 模式:institutional Bronze 主表;gov_bank Bronze 缺對應 (stock,date)
  時 gov_bank_net = NULL,不影響其他 stocks/dates 的 pivot

驗證(用戶本機):
    python scripts/verify_pr19b_silver.py
    對 institutional 跑 round-trip 比對 institutional_daily(v2.0 legacy)
    10 個 buy/sell 欄應 100% 等值;gov_bank_net 兩邊 v2.0 legacy 沒有,
    skip 在 verify spec 內(以 Bronze 是否填寫驗 gov_bank_net 是否 NOT NULL)。
"""

from __future__ import annotations

import logging
import time
from typing import Any

from .._common import filter_to_trading_days, get_trading_dates, fetch_bronze, upsert_silver


logger = logging.getLogger("collector.silver.builders.institutional")


NAME          = "institutional"
SILVER_TABLE  = "institutional_daily_derived"
BRONZE_TABLES = ["institutional_investors_tw", "government_bank_buy_sell_tw"]


# investor_type → (buy 欄, sell 欄)— Bronze investor_type 已是英文 key
INVESTOR_TYPE_MAP: dict[str, tuple[str, str]] = {
    "Foreign_Investor":     ("foreign_buy",              "foreign_sell"),
    "Foreign_Dealer_Self":  ("foreign_dealer_self_buy",  "foreign_dealer_self_sell"),
    "Investment_Trust":     ("investment_trust_buy",     "investment_trust_sell"),
    "Dealer":               ("dealer_buy",               "dealer_sell"),
    "Dealer_Hedging":       ("dealer_hedging_buy",       "dealer_hedging_sell"),
}


def _build_gov_bank_lookup(
    bronze_rows: list[dict[str, Any]],
) -> dict[tuple, int]:
    """v3.14:Bronze 多 row(8 行庫各 1)→ SUM net by (market, stock_id, date)。
    NULL 視同 0(對齊 SQL SUM 行為)。回傳 `{(market, stock_id, date): net_shares}`。
    """
    sums: dict[tuple, dict[str, int]] = {}
    for row in bronze_rows:
        key = (row.get("market"), row.get("stock_id"), row.get("date"))
        if key not in sums:
            sums[key] = {"buy": 0, "sell": 0}
        buy  = row.get("buy")
        sell = row.get("sell")
        if buy is not None:
            sums[key]["buy"] += int(buy)
        if sell is not None:
            sums[key]["sell"] += int(sell)
    return {k: v["buy"] - v["sell"] for k, v in sums.items()}


def _pivot(
    bronze_rows: list[dict[str, Any]],
    trading_dates: set[str],
    gov_bank_lookup: dict[tuple, int],
) -> list[dict[str, Any]]:
    """Bronze 多 row → Silver 1 寬 row。

    v3.14.1:對齊 v1.26 B fix(margin/market_margin 同 pattern):**先 seed
    所有 gov_bank-only keys 成 empty agg**,再用 institutional Bronze 覆蓋。
    這樣 gov_bank Bronze 涵蓋的 (stock,date) 即使 institutional 沒料(FinMind
    institutional 比 gov_bank 慢幾天 / 退市 / 假日),Silver 仍生 row 且
    gov_bank_net 從 lookup 填。
    """
    if trading_dates:
        bronze_rows = filter_to_trading_days(bronze_rows, trading_dates, label=NAME)

    grouped: dict[tuple, dict[str, Any]] = {}

    def _empty_agg(market: Any, stock_id: Any, dt: Any, key: tuple) -> dict[str, Any]:
        agg = {"market": market, "stock_id": stock_id, "date": dt}
        for buy_col, sell_col in INVESTOR_TYPE_MAP.values():
            agg[buy_col]  = None
            agg[sell_col] = None
        agg["gov_bank_net"] = gov_bank_lookup.get(key)
        return agg

    # Seed:gov_bank-only keys(institutional Bronze 沒料的日子)先生 empty agg
    # safety:trading_dates 為空時 bypass filter,否則只 seed 真實交易日
    for key in gov_bank_lookup:
        market, stock_id, dt = key
        if trading_dates and dt not in trading_dates:
            continue
        if key not in grouped:
            grouped[key] = _empty_agg(market, stock_id, dt, key)

    # institutional Bronze 覆蓋:對應 (stock, date) 有料 → 填法人 buy/sell 欄
    for row in bronze_rows:
        key = (row.get("market"), row.get("stock_id"), row.get("date"))
        if key not in grouped:
            grouped[key] = _empty_agg(
                row.get("market"), row.get("stock_id"), row.get("date"), key,
            )

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
    gov_bank_bronze = fetch_bronze(db, "government_bank_buy_sell_tw", stock_ids=stock_ids)
    gov_bank_lookup = _build_gov_bank_lookup(gov_bank_bronze)

    silver = _pivot(bronze, trading_dates, gov_bank_lookup)

    written = upsert_silver(
        db, SILVER_TABLE, silver,
        pk_cols=["market", "stock_id", "date"],
    )

    elapsed_ms = int((time.monotonic() - start) * 1000)
    logger.info(
        f"[{NAME}] read={len(bronze)} bronze rows + {len(gov_bank_bronze)} gov_bank rows → "
        f"wrote={written} silver rows(elapsed={elapsed_ms}ms)"
    )
    return {
        "name":         NAME,
        "rows_read":    len(bronze),
        "rows_written": written,
        "elapsed_ms":   elapsed_ms,
    }
