"""
cross_cores/long_term_low_vol.py
================================
Toolkit C C1:36-month low volatility cross-stock ranking。

對 36M 交易日(約 756 trading days)計算 daily return std,low-vol top stocks。

Refs:
  - Blitz, D., & van Vliet, P. (2007). JPM 34(1), 102-113.
  - Ang et al. (2009). JFE 91(1), 1-23.
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

logger = logging.getLogger("collector.cross_cores.long_term_low_vol")

NAME            = "long_term_low_vol"
OUTPUT_TABLE    = "long_term_low_vol_ranked_derived"
UPSTREAM_TABLES = ["price_daily_fwd", "stock_info_ref"]

WINDOW_DAYS = 756   # 36 月 × 21 = 約 756 交易日
MIN_OBS = 252       # 至少 1 年資料才算
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
                                  extras={"std_36m": None, "vol_rank": None}))
            continue

        closes_rows = fetch_close_series(
            db, stock_id=sid, end_date=target_date, lookback_days=WINDOW_DAYS + 1,
        )
        closes = [r["close"] for r in reversed(closes_rows) if r.get("close") is not None]
        if len(closes) < MIN_OBS:
            rows.append(empty_row(sid, target_date, excluded_reason="insufficient_history",
                                  extras={"std_36m": None, "vol_rank": None}))
            continue

        returns = compute_returns_from_closes(closes)
        std = compute_std(returns)
        rows.append({
            "market": "TW", "stock_id": sid, "date": target_date,
            "std_36m": std, "vol_rank": None,
            "universe_size": None, "is_top_n": False, "excluded_reason": None,
        })

    assign_ranks(rows, rank_col="vol_rank", metric_col="std_36m",
                 reverse=False, top_n=TOP_N)
    written = upsert_silver(db, OUTPUT_TABLE, rows,
                            pk_cols=["market", "stock_id", "date"])
    elapsed_ms = int((time.monotonic() - start) * 1000)
    logger.info(f"[{NAME}] rows={len(rows)} written={written} ({elapsed_ms}ms)")
    return {"name": NAME, "rows_read": len(rows), "rows_written": written,
            "elapsed_ms": elapsed_ms}
