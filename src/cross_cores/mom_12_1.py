"""
cross_cores/mom_12_1.py
=======================
Toolkit C C3:12-1 momentum cross-stock ranking。

過去 12 月(~252 日)累積報酬,skip 最近 1 月(~21 日)避免短期反轉效應。
top decile / top N。

Refs:
  - Jegadeesh, N., & Titman, S. (1993). "Returns to Buying Winners and Selling
    Losers." *Journal of Finance* 48(1), 65-91.
"""

from __future__ import annotations

import logging
import time
from typing import Any

from silver._common import upsert_silver

from cross_cores._shared import (
    assign_ranks,
    empty_row,
    fetch_close_series,
    fetch_latest_date,
    fetch_universe_filter,
)

logger = logging.getLogger("collector.cross_cores.mom_12_1")

NAME            = "mom_12_1"
OUTPUT_TABLE    = "mom_12_1_ranked_derived"
UPSTREAM_TABLES = ["price_daily_fwd", "stock_info_ref"]

WINDOW_12M = 252
WINDOW_1M  = 21
TOP_N = 30


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
    rows: list[dict[str, Any]] = []
    for sid, excluded in universe.items():
        if stock_ids and sid not in stock_ids:
            continue
        if excluded is not None:
            rows.append(empty_row(sid, target_date, excluded_reason=excluded,
                                  extras={"return_12m_1m": None, "mom_rank": None}))
            continue

        # 取 WINDOW_12M + buffer 日,確認有足夠資料
        closes_rows = fetch_close_series(
            db, stock_id=sid, end_date=target_date, lookback_days=WINDOW_12M + 5,
        )
        # closes_rows 是 desc 排序,第 0 個 = 最新
        if len(closes_rows) < WINDOW_12M:
            rows.append(empty_row(sid, target_date, excluded_reason="insufficient_history",
                                  extras={"return_12m_1m": None, "mom_rank": None}))
            continue

        # close_skip1m = 1 月前的 close(skip 最近 1M)
        # close_12m_ago = 12 月前的 close
        try:
            close_skip1m = closes_rows[WINDOW_1M]["close"]
            close_12m_ago = closes_rows[WINDOW_12M - 1]["close"]
        except (IndexError, KeyError, TypeError):
            rows.append(empty_row(sid, target_date, excluded_reason="missing_close",
                                  extras={"return_12m_1m": None, "mom_rank": None}))
            continue

        if not close_12m_ago or close_12m_ago <= 0:
            rows.append(empty_row(sid, target_date, excluded_reason="invalid_close",
                                  extras={"return_12m_1m": None, "mom_rank": None}))
            continue

        ret_12_1 = (close_skip1m - close_12m_ago) / close_12m_ago
        rows.append({
            "market": "TW", "stock_id": sid, "date": target_date,
            "return_12m_1m": ret_12_1, "mom_rank": None,
            "universe_size": None, "is_top_n": False, "excluded_reason": None,
        })

    # rank:高報酬好(reverse=True)
    assign_ranks(rows, rank_col="mom_rank", metric_col="return_12m_1m",
                 reverse=True, top_n=TOP_N)
    written = upsert_silver(db, OUTPUT_TABLE, rows,
                            pk_cols=["market", "stock_id", "date"])
    elapsed_ms = int((time.monotonic() - start) * 1000)
    logger.info(f"[{NAME}] rows={len(rows)} written={written} ({elapsed_ms}ms)")
    return {"name": NAME, "rows_read": len(rows), "rows_written": written,
            "elapsed_ms": elapsed_ms}
