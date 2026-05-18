"""
cross_cores/low_volatility.py
=============================
Toolkit B B2:Low Volatility 252d cross-stock ranking。

對 252 trading days 計算 daily return std,bottom quintile 為 low-vol top stocks。
寫入 `low_volatility_ranked_derived`。

Refs:
  - Ang, A., Hodrick, R. J., Xing, Y., & Zhang, X. (2009). "High Idiosyncratic
    Volatility and Low Returns: International and Further U.S. Evidence."
    *Journal of Financial Economics* 91(1), 1-23.
  - Blitz, D., & van Vliet, P. (2007). "The Volatility Effect: Lower Risk
    without Lower Return." *Journal of Portfolio Management* 34(1), 102-113.
"""

from __future__ import annotations

import logging
import time
from typing import Any

from silver._common import upsert_silver

from cross_cores._shared import (
    assign_ranks,
    compute_returns_from_closes,
    compute_std,
    empty_row,
    fetch_close_series,
    fetch_latest_date,
    fetch_universe_filter,
)

logger = logging.getLogger("collector.cross_cores.low_volatility")

NAME            = "low_volatility"
OUTPUT_TABLE    = "low_volatility_ranked_derived"
UPSTREAM_TABLES = ["price_daily_fwd", "stock_info_ref"]

WINDOW_DAYS = 252
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
                                  extras={"std_252d": None, "vol_rank": None}))
            continue

        closes_rows = fetch_close_series(
            db, stock_id=sid, end_date=target_date, lookback_days=WINDOW_DAYS + 1,
        )
        closes = [r["close"] for r in reversed(closes_rows) if r.get("close") is not None]
        if len(closes) < 50:
            rows.append(empty_row(sid, target_date, excluded_reason="insufficient_history",
                                  extras={"std_252d": None, "vol_rank": None}))
            continue

        returns = compute_returns_from_closes(closes)
        std = compute_std(returns)
        rows.append({
            "market": "TW", "stock_id": sid, "date": target_date,
            "std_252d": std, "vol_rank": None,
            "universe_size": None, "is_top_n": False, "excluded_reason": None,
        })

    # rank:low vol 好(reverse=False → rank 1 = lowest std)
    assign_ranks(rows, rank_col="vol_rank", metric_col="std_252d",
                 reverse=False, top_n=TOP_N)

    written = upsert_silver(db, OUTPUT_TABLE, rows,
                            pk_cols=["market", "stock_id", "date"])
    elapsed_ms = int((time.monotonic() - start) * 1000)
    logger.info(f"[{NAME}] rows={len(rows)} written={written} ({elapsed_ms}ms)")
    return {"name": NAME, "rows_read": len(rows), "rows_written": written,
            "elapsed_ms": elapsed_ms}
