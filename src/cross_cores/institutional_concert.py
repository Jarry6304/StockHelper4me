"""
cross_cores/institutional_concert.py
====================================
Toolkit A A3:Institutional Concert(Sias 2004 + 周賓凰-池祥麟 2014)。

過去 20D 三大法人(外資 / 投信 / 自營)同向買賣天數,加 foreign 累積 / 流通股數
top decile。

Refs:
  - Sias, R. W. (2004). "Institutional Herding." *Review of Financial Studies* 17(1), 165-206.
  - 周賓凰、池祥麟 (2014). "三大法人於台灣股市的買賣超與市場報酬之關聯性分析." TES.
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

logger = logging.getLogger("collector.cross_cores.institutional_concert")

NAME            = "institutional_concert"
OUTPUT_TABLE    = "institutional_concert_ranked_derived"
UPSTREAM_TABLES = [
    "institutional_daily_derived",
    "foreign_holding_derived",
    "foreign_investor_share_tw",
    "stock_info_ref",
]

WINDOW_20D = 20
TOP_N = 30


def _fetch_institutional_20d(
    db: Any, end_date: Any, *, market: str = "TW",
) -> dict[str, list[dict[str, Any]]]:
    """每股最近 20 個交易日 institutional net。

    Schema 對齊 institutional_daily_derived(v1.5 5 類法人各拆 buy/sell):
      - 外資   = foreign + foreign_dealer_self
      - 投信   = investment_trust
      - 自營   = dealer + dealer_hedging
    """
    rows = db.query(
        """
        SELECT stock_id, date,
               (COALESCE(foreign_buy, 0) + COALESCE(foreign_dealer_self_buy, 0)
                - COALESCE(foreign_sell, 0) - COALESCE(foreign_dealer_self_sell, 0))::float8 AS foreign_net,
               (COALESCE(investment_trust_buy, 0)
                - COALESCE(investment_trust_sell, 0))::float8 AS investment_trust_net,
               (COALESCE(dealer_buy, 0) + COALESCE(dealer_hedging_buy, 0)
                - COALESCE(dealer_sell, 0) - COALESCE(dealer_hedging_sell, 0))::float8 AS dealer_net
          FROM institutional_daily_derived
         WHERE market = %s AND date <= %s
         ORDER BY stock_id, date DESC
        """,
        [market, end_date],
    )
    out: dict[str, list[dict[str, Any]]] = {}
    for r in rows:
        out.setdefault(r["stock_id"], []).append(r)
    for sid in out:
        out[sid] = out[sid][:WINDOW_20D]
    return out


def _fetch_shares_outstanding(
    db: Any, end_date: Any, *, market: str = "TW",
) -> dict[str, float]:
    """每股 latest 60-day shares_outstanding(對齊 magic_formula._fetch_market_caps_for_date)。"""
    rows = db.query(
        """
        SELECT DISTINCT ON (stock_id) stock_id,
               total_issued::float8 AS total_issued
          FROM foreign_investor_share_tw
         WHERE market = %s AND date <= %s
           AND date >= (%s::date - INTERVAL '60 days')
           AND total_issued IS NOT NULL
         ORDER BY stock_id, date DESC
        """,
        [market, end_date, end_date],
    )
    return {r["stock_id"]: r["total_issued"] for r in rows}


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
    inst_history = _fetch_institutional_20d(db, target_date)
    shares_out = _fetch_shares_outstanding(db, target_date)

    rows: list[dict[str, Any]] = []
    for sid, excluded in universe.items():
        if stock_ids and sid not in stock_ids:
            continue
        if excluded is not None:
            rows.append(empty_row(sid, target_date, excluded_reason=excluded,
                                  extras={"concert_days": None,
                                          "foreign_cumulative_20d": None,
                                          "shares_outstanding": None,
                                          "cumulative_pct": None,
                                          "concert_rank": None}))
            continue

        history = inst_history.get(sid) or []
        if len(history) < 5:    # 至少 5 天才有意義
            rows.append(empty_row(sid, target_date, excluded_reason="insufficient_inst_history",
                                  extras={"concert_days": None,
                                          "foreign_cumulative_20d": None,
                                          "shares_outstanding": None,
                                          "cumulative_pct": None,
                                          "concert_rank": None}))
            continue

        # 同向天數 = 三家法人同 sign(都 > 0 或都 < 0)
        concert_days = 0
        foreign_cum = 0.0
        for r in history:
            fn = r["foreign_net"]
            it = r["investment_trust_net"]
            dn = r["dealer_net"]
            foreign_cum += fn
            if (fn > 0 and it > 0 and dn > 0) or (fn < 0 and it < 0 and dn < 0):
                concert_days += 1

        shares = shares_out.get(sid)
        cumulative_pct = (foreign_cum / shares) if shares and shares > 0 else None

        rows.append({
            "market": "TW", "stock_id": sid, "date": target_date,
            "concert_days": concert_days,
            "foreign_cumulative_20d": foreign_cum,
            "shares_outstanding": shares,
            "cumulative_pct": cumulative_pct,
            "concert_rank": None,
            "universe_size": None, "is_top_n": False, "excluded_reason": None,
        })

    # rank by cumulative_pct(法人累積 / 流通股數 — 高的好)
    assign_ranks(rows, rank_col="concert_rank", metric_col="cumulative_pct",
                 reverse=True, top_n=TOP_N)
    written = upsert_silver(db, OUTPUT_TABLE, rows,
                            pk_cols=["market", "stock_id", "date"])
    elapsed_ms = int((time.monotonic() - start) * 1000)
    logger.info(f"[{NAME}] rows={len(rows)} written={written} ({elapsed_ms}ms)")
    return {"name": NAME, "rows_read": len(rows), "rows_written": written,
            "elapsed_ms": elapsed_ms}
