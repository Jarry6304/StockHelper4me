"""
cross_cores/revenue_momentum.py
===============================
Toolkit A A2:Revenue Momentum 3-consec (Hung-Lu-Yang 2025)。

月營收 YoY top decile + 過去 3 月 YoY 連續正成長。對齊台股強制揭露機制
(每月 10 日前公告前月營收)。

Refs:
  - Hung, W., Lu, C. C., & Yang, J. J. (2025). "Market reaction to monthly
    revenue momentum." *Review of Quantitative Finance and Accounting*.
"""

from __future__ import annotations

import logging
import time
from typing import Any

from silver._common import upsert_silver

from cross_cores._shared import (
    assign_ranks,
    empty_row,
    fetch_latest_date,
    fetch_universe_filter,
)

logger = logging.getLogger("collector.cross_cores.revenue_momentum")

NAME            = "revenue_momentum"
OUTPUT_TABLE    = "revenue_momentum_ranked_derived"
UPSTREAM_TABLES = ["monthly_revenue_derived", "stock_info_ref"]

TOP_N = 30
CONSECUTIVE_MIN = 3   # 3 個月連續正


def _fetch_recent_revenue(
    db: Any, end_date: Any, *, market: str = "TW", n_months: int = 6,
) -> dict[str, list[dict[str, Any]]]:
    """每股最近 n_months 月的 revenue_yoy(desc:最新→舊)。"""
    rows = db.query(
        """
        SELECT stock_id, date, revenue_yoy::float8 AS revenue_yoy
          FROM monthly_revenue_derived
         WHERE market = %s AND date <= %s
         ORDER BY stock_id, date DESC
        """,
        [market, end_date],
    )
    out: dict[str, list[dict[str, Any]]] = {}
    for r in rows:
        out.setdefault(r["stock_id"], []).append(r)
    # truncate per stock to n_months
    for sid in out:
        out[sid] = out[sid][:n_months]
    return out


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
    revenue_by_stock = _fetch_recent_revenue(db, target_date, n_months=6)

    rows: list[dict[str, Any]] = []
    for sid, excluded in universe.items():
        if stock_ids and sid not in stock_ids:
            continue
        if excluded is not None:
            rows.append(empty_row(sid, target_date, excluded_reason=excluded,
                                  extras={"revenue_yoy_latest": None,
                                          "consecutive_positive": None,
                                          "revenue_rank": None}))
            continue

        history = revenue_by_stock.get(sid) or []
        if not history:
            rows.append(empty_row(sid, target_date, excluded_reason="no_revenue_data",
                                  extras={"revenue_yoy_latest": None,
                                          "consecutive_positive": None,
                                          "revenue_rank": None}))
            continue

        latest_yoy = history[0].get("revenue_yoy")
        consec = 0
        for r in history:
            yoy = r.get("revenue_yoy")
            if yoy is not None and yoy > 0:
                consec += 1
            else:
                break

        # 不夠 3 月連續 → 排除 ranking(但仍記入 detail)
        if consec < CONSECUTIVE_MIN:
            rows.append({
                "market": "TW", "stock_id": sid, "date": target_date,
                "revenue_yoy_latest": latest_yoy,
                "consecutive_positive": consec,
                "revenue_rank": None,
                "universe_size": None, "is_top_n": False,
                "excluded_reason": "consecutive_below_3",
            })
            continue

        rows.append({
            "market": "TW", "stock_id": sid, "date": target_date,
            "revenue_yoy_latest": latest_yoy,
            "consecutive_positive": consec,
            "revenue_rank": None,
            "universe_size": None, "is_top_n": False, "excluded_reason": None,
        })

    # rank by latest YoY(高的好)
    assign_ranks(rows, rank_col="revenue_rank", metric_col="revenue_yoy_latest",
                 reverse=True, top_n=TOP_N)
    written = upsert_silver(db, OUTPUT_TABLE, rows,
                            pk_cols=["market", "stock_id", "date"])
    elapsed_ms = int((time.monotonic() - start) * 1000)
    logger.info(f"[{NAME}] rows={len(rows)} written={written} ({elapsed_ms}ms)")
    return {"name": NAME, "rows_read": len(rows), "rows_written": written,
            "elapsed_ms": elapsed_ms}
