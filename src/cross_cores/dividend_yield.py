"""
cross_cores/dividend_yield.py
=============================
Toolkit C C2:Cash Dividend Yield + yield trap filter (Boudoukh 2007 + 提案 v1.1)。

殖利率 ≥ 4% + 12M 報酬 > -20%(falling knife filter)+ 5y 至少 3y 配息
(可持續性)。

對齊提案 v1.1 設計:Hard filter(三條件全過)+ Soft rank(殖利率 z-score)。

Refs:
  - Boudoukh, J., Michaely, R., Richardson, M., & Roberts, M. (2007).
    "On the Importance of Measuring Payout Yield." *Journal of Finance* 62(2), 877-915.
  - Engsted, T., & Pedersen, T. Q. (2018). "Dividend Persistence and Return
    Predictability."
"""

from __future__ import annotations

import logging
import time
from datetime import timedelta
from typing import Any

from silver._common import upsert_silver

from cross_cores._shared import (
    assign_ranks,
    empty_row,
    fetch_close_series,
    fetch_latest_date,
    fetch_universe_filter,
)

logger = logging.getLogger("collector.cross_cores.dividend_yield")

NAME            = "dividend_yield"
OUTPUT_TABLE    = "dividend_yield_ranked_derived"
UPSTREAM_TABLES = [
    "valuation_daily_derived",     # primary:已算過的 dividend_yield 欄
    "price_adjustment_events",     # fallback:cash_dividend SUM
    "price_daily_fwd",
    "stock_info_ref",
]

TOP_N = 30
MIN_YIELD_PCT = 4.0           # 殖利率 ≥ 4%
MAX_12M_DROP = -20.0          # 12M 報酬 > -20%
MIN_PAYOUT_YEARS_5Y = 3       # 5 年至少 3 年配息


def _fetch_latest_yield(
    db: Any, end_date: Any, *, market: str = "TW",
) -> dict[str, float]:
    """每股 latest valuation_daily_derived.dividend_yield(對齊 v3.31 確認 column 已有)。"""
    rows = db.query(
        """
        SELECT DISTINCT ON (stock_id) stock_id,
               dividend_yield::float8 AS dividend_yield
          FROM valuation_daily_derived
         WHERE market = %s AND date <= %s AND dividend_yield IS NOT NULL
         ORDER BY stock_id, date DESC
        """,
        [market, end_date],
    )
    return {r["stock_id"]: r["dividend_yield"] for r in rows}


def _fetch_5y_payout_history(
    db: Any, end_date: Any, *, market: str = "TW",
) -> dict[str, int]:
    """每股 5 年內配息年數(對齊 price_adjustment_events.cash_dividend > 0)。"""
    five_years_ago = end_date - timedelta(days=365 * 5)
    rows = db.query(
        """
        SELECT stock_id,
               COUNT(DISTINCT EXTRACT(YEAR FROM date))::int AS payout_years
          FROM price_adjustment_events
         WHERE market = %s AND date <= %s AND date >= %s
           AND event_type = 'cash_dividend'
           AND cash_dividend IS NOT NULL AND cash_dividend > 0
         GROUP BY stock_id
        """,
        [market, end_date, five_years_ago],
    )
    return {r["stock_id"]: r["payout_years"] for r in rows}


def _compute_12m_return(closes_rows: list[dict]) -> float | None:
    """closes_rows desc;算 12M 報酬 (latest vs 252d ago)。"""
    if len(closes_rows) < 252:
        return None
    cur = closes_rows[0].get("close")
    past = closes_rows[251].get("close")
    if not cur or not past or past <= 0:
        return None
    return (cur - past) / past * 100.0   # pct


def run(
    db: Any,
    stock_ids: list[str] | None = None,
    full_rebuild: bool = False,
    lookback_days: int | None = None,
) -> dict[str, Any]:
    start = time.monotonic()
    target_date = fetch_latest_date(db, "price_daily_fwd")
    if target_date is None:
        return {"name": NAME, "rows_read": 0, "rows_written": 0,
                "elapsed_ms": int((time.monotonic() - start) * 1000)}

    universe = fetch_universe_filter(db)
    yields = _fetch_latest_yield(db, target_date)
    payout_5y = _fetch_5y_payout_history(db, target_date)

    rows: list[dict[str, Any]] = []
    for sid, excluded in universe.items():
        if stock_ids and sid not in stock_ids:
            continue
        if excluded is not None:
            rows.append(empty_row(sid, target_date, excluded_reason=excluded,
                                  extras={"dividend_yield_pct": None,
                                          "return_12m_pct": None,
                                          "payout_years_5y": None,
                                          "yield_rank": None}))
            continue

        y = yields.get(sid)
        if y is None:
            rows.append(empty_row(sid, target_date, excluded_reason="no_yield_data",
                                  extras={"dividend_yield_pct": None,
                                          "return_12m_pct": None,
                                          "payout_years_5y": None,
                                          "yield_rank": None}))
            continue

        closes_rows = fetch_close_series(db, stock_id=sid, end_date=target_date,
                                         lookback_days=260)
        ret_12m = _compute_12m_return(closes_rows)
        years = payout_5y.get(sid, 0)

        # Hard filter
        excluded_reason: str | None = None
        if y < MIN_YIELD_PCT:
            excluded_reason = "yield_below_4pct"
        elif ret_12m is None or ret_12m < MAX_12M_DROP:
            excluded_reason = "yield_trap_falling_knife"
        elif years < MIN_PAYOUT_YEARS_5Y:
            excluded_reason = "payout_history_insufficient"

        rows.append({
            "market": "TW", "stock_id": sid, "date": target_date,
            "dividend_yield_pct": y,
            "return_12m_pct": ret_12m,
            "payout_years_5y": years,
            "yield_rank": None,
            "universe_size": None, "is_top_n": False,
            "excluded_reason": excluded_reason,
        })

    # rank by dividend_yield_pct(高的好),只 rank 未排除者
    eligible_rows = [r for r in rows if r.get("excluded_reason") is None]
    eligible_rows.sort(key=lambda r: r["dividend_yield_pct"], reverse=True)
    n = len(eligible_rows)
    for i, r in enumerate(eligible_rows, 1):
        r["yield_rank"] = i
        r["universe_size"] = n
    for r in eligible_rows[:TOP_N]:
        r["is_top_n"] = True

    written = upsert_silver(db, OUTPUT_TABLE, rows,
                            pk_cols=["market", "stock_id", "date"])
    elapsed_ms = int((time.monotonic() - start) * 1000)
    logger.info(f"[{NAME}] eligible={n} rows={len(rows)} written={written} ({elapsed_ms}ms)")
    return {"name": NAME, "rows_read": len(rows), "rows_written": written,
            "elapsed_ms": elapsed_ms}
