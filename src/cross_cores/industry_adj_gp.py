"""
cross_cores/industry_adj_gp.py
==============================
Toolkit B B3:Industry-Adjusted Gross Profitability (Novy-Marx 2013)。

GP = (Revenue - COGS) / Total Assets
Industry-Adj GP = GP − 同產業中位數

對齊提案 v1.1:用 industry-adjusted 防止 raw GP 被半導體 (TSMC) 等高 GP 產業
霸占 top N(Asness-Frazzini-Pedersen 2014 QMJ conditional sort 概念)。

Refs:
  - Novy-Marx, R. (2013). "The other side of value: The gross profitability premium."
    *Journal of Financial Economics* 108(1), 1-28.
  - Ng, A. C. C., & Shen, J. (2020). "Quality Investing in Asian Stock Markets."
    *Accounting & Finance*.
"""

from __future__ import annotations

import logging
import statistics
import time
from typing import Any

from silver._common import upsert_silver

from cross_cores._shared import (
    assign_ranks,
    empty_row,
    fetch_latest_date,
    fetch_universe_filter,
)

logger = logging.getLogger("collector.cross_cores.industry_adj_gp")

NAME            = "industry_adj_gp"
OUTPUT_TABLE    = "industry_adj_gp_ranked_derived"
UPSTREAM_TABLES = ["financial_statement_derived", "stock_info_ref"]

TOP_N = 30

KEY_REVENUE      = ("營業收入合計", "營業收入", "Revenue", "OperatingRevenue")
KEY_COGS         = ("營業成本合計", "營業成本", "CostOfGoodsSold", "COGS")
KEY_TOTAL_ASSETS = ("資產總額", "資產總計", "TotalAssets")


def _detail_get(detail: dict, keys: tuple[str, ...]) -> float | None:
    if not detail:
        return None
    for k in keys:
        v = detail.get(k)
        if v is None:
            continue
        try:
            return float(v)
        except (TypeError, ValueError):
            continue
    return None


def _fetch_industry_map(db: Any, *, market: str = "TW") -> dict[str, str]:
    """stock_id → industry_category。"""
    rows = db.query(
        "SELECT stock_id, industry_category FROM stock_info_ref WHERE market = %s",
        [market],
    )
    return {r["stock_id"]: (r.get("industry_category") or "其他") for r in rows}


def _fetch_latest_gp_inputs(
    db: Any, end_date: Any, *, market: str = "TW",
) -> dict[str, dict[str, float | None]]:
    """每股最近 1 季 income(revenue/cogs)+ 最近 1 季 balance(total_assets)。"""
    income_rows = db.query(
        """
        SELECT DISTINCT ON (stock_id) stock_id, date, detail
          FROM financial_statement_derived
         WHERE market = %s AND date <= %s AND type = 'income'
           AND detail IS NOT NULL
         ORDER BY stock_id, date DESC
        """,
        [market, end_date],
    )
    balance_rows = db.query(
        """
        SELECT DISTINCT ON (stock_id) stock_id, date, detail
          FROM financial_statement_derived
         WHERE market = %s AND date <= %s AND type = 'balance'
           AND detail IS NOT NULL
         ORDER BY stock_id, date DESC
        """,
        [market, end_date],
    )
    income_by_stock = {r["stock_id"]: r for r in income_rows}
    balance_by_stock = {r["stock_id"]: r for r in balance_rows}

    out: dict[str, dict[str, float | None]] = {}
    for sid, ir in income_by_stock.items():
        br = balance_by_stock.get(sid)
        if br is None:
            continue
        rev = _detail_get(ir["detail"], KEY_REVENUE)
        cogs = _detail_get(ir["detail"], KEY_COGS)
        ta = _detail_get(br["detail"], KEY_TOTAL_ASSETS)
        out[sid] = {"revenue": rev, "cogs": cogs, "total_assets": ta}
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
    industry_map = _fetch_industry_map(db)
    gp_inputs = _fetch_latest_gp_inputs(db, target_date)

    # Pass 1:算每股 raw GP
    rows: list[dict[str, Any]] = []
    industry_gp_groups: dict[str, list[float]] = {}

    for sid, excluded in universe.items():
        if stock_ids and sid not in stock_ids:
            continue
        industry = industry_map.get(sid, "其他")
        if excluded is not None:
            rows.append(empty_row(sid, target_date, excluded_reason=excluded,
                                  extras={"gross_profitability": None,
                                          "industry": industry,
                                          "industry_median_gp": None,
                                          "industry_adj_gp": None,
                                          "gp_rank": None}))
            continue

        fin = gp_inputs.get(sid)
        if fin is None or fin["revenue"] is None or fin["cogs"] is None \
                or fin["total_assets"] is None or fin["total_assets"] <= 0:
            rows.append(empty_row(sid, target_date, excluded_reason="no_gp_data",
                                  extras={"gross_profitability": None,
                                          "industry": industry,
                                          "industry_median_gp": None,
                                          "industry_adj_gp": None,
                                          "gp_rank": None}))
            continue

        gp = (fin["revenue"] - fin["cogs"]) / fin["total_assets"]
        industry_gp_groups.setdefault(industry, []).append(gp)
        rows.append({
            "market": "TW", "stock_id": sid, "date": target_date,
            "gross_profitability": gp,
            "industry": industry,
            "industry_median_gp": None,
            "industry_adj_gp": None,
            "gp_rank": None,
            "universe_size": None, "is_top_n": False, "excluded_reason": None,
        })

    # Pass 2:算每產業 median + adjusted gp
    industry_medians = {
        ind: (statistics.median(vals) if vals else 0.0)
        for ind, vals in industry_gp_groups.items()
    }
    for r in rows:
        if r.get("gross_profitability") is None:
            continue
        ind = r["industry"]
        median = industry_medians.get(ind, 0.0)
        r["industry_median_gp"] = median
        r["industry_adj_gp"] = r["gross_profitability"] - median

    # rank by industry_adj_gp(高的好)
    assign_ranks(rows, rank_col="gp_rank", metric_col="industry_adj_gp",
                 reverse=True, top_n=TOP_N)
    written = upsert_silver(db, OUTPUT_TABLE, rows,
                            pk_cols=["market", "stock_id", "date"])
    elapsed_ms = int((time.monotonic() - start) * 1000)
    logger.info(f"[{NAME}] industries={len(industry_medians)} rows={len(rows)} "
                f"written={written} ({elapsed_ms}ms)")
    return {"name": NAME, "rows_read": len(rows), "rows_written": written,
            "elapsed_ms": elapsed_ms}
