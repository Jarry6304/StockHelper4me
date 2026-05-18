"""
cross_cores/monthly_trigger.py
==============================
Layer 5 Monthly Trigger Overlay(實驗性,提案 v1.1 §四)。

每月偵測 trigger events:
  Positive:revenue YoY > +30% AND 過去 20D 法人累積買超 > 0
  Negative:revenue YoY < -20% AND 法人賣超 > 流通市值 1%

不獨立配資,作為「conviction adjustment」hint(+20% / -50% position scale)。

寫入 `monthly_trigger_signals_derived`(PK 含 trigger_type,事件性質)。

Refs:
  - Hung, W., Lu, C. C., & Yang, J. J. (2025). RQFA 月營收揭露 alpha
  - Sias 2004 institutional herding(短期 momentum chain)
"""

from __future__ import annotations

import logging
import time
from typing import Any

from silver._common import upsert_silver

from cross_cores._shared import (
    fetch_latest_date,
    fetch_universe_filter,
)

logger = logging.getLogger("collector.cross_cores.monthly_trigger")

NAME            = "monthly_trigger"
OUTPUT_TABLE    = "monthly_trigger_signals_derived"
UPSTREAM_TABLES = [
    "monthly_revenue_derived",
    "institutional_daily_derived",
    "foreign_investor_share_tw",
    "stock_info_ref",
]

WINDOW_20D = 20
POSITIVE_YOY_THRESHOLD = 30.0    # revenue YoY > +30%
NEGATIVE_YOY_THRESHOLD = -20.0   # revenue YoY < -20%
NEGATIVE_SELL_PCT = 0.01         # 賣超 > 流通市值 1%


def _fetch_latest_revenue_yoy(
    db: Any, end_date: Any, *, market: str = "TW",
) -> dict[str, float]:
    """每股最新 monthly_revenue_derived.revenue_yoy。"""
    rows = db.query(
        """
        SELECT DISTINCT ON (stock_id) stock_id,
               revenue_yoy::float8 AS revenue_yoy
          FROM monthly_revenue_derived
         WHERE market = %s AND date <= %s
         ORDER BY stock_id, date DESC
        """,
        [market, end_date],
    )
    return {r["stock_id"]: r["revenue_yoy"] for r in rows if r.get("revenue_yoy") is not None}


def _fetch_inst_20d_net(
    db: Any, end_date: Any, *, market: str = "TW",
) -> dict[str, float]:
    """每股過去 20 個交易日 institutional sum(三大法人加總)。

    Schema 對齊 institutional_daily_derived(v1.5 5 類法人各拆 buy/sell):
      net = (foreign + foreign_dealer_self + investment_trust + dealer + dealer_hedging) buy - sell
    """
    rows = db.query(
        """
        WITH ranked AS (
          SELECT stock_id, date,
                 (COALESCE(foreign_buy, 0) + COALESCE(foreign_dealer_self_buy, 0)
                  + COALESCE(investment_trust_buy, 0)
                  + COALESCE(dealer_buy, 0) + COALESCE(dealer_hedging_buy, 0)
                  - COALESCE(foreign_sell, 0) - COALESCE(foreign_dealer_self_sell, 0)
                  - COALESCE(investment_trust_sell, 0)
                  - COALESCE(dealer_sell, 0) - COALESCE(dealer_hedging_sell, 0))::float8 AS net,
                 ROW_NUMBER() OVER (PARTITION BY stock_id ORDER BY date DESC) AS rn
            FROM institutional_daily_derived
           WHERE market = %s AND date <= %s
        )
        SELECT stock_id, SUM(net)::float8 AS sum_net
          FROM ranked
         WHERE rn <= 20
         GROUP BY stock_id
        """,
        [market, end_date],
    )
    return {r["stock_id"]: r["sum_net"] for r in rows}


def _fetch_shares_outstanding(
    db: Any, end_date: Any, *, market: str = "TW",
) -> dict[str, float]:
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
    revenue_yoy = _fetch_latest_revenue_yoy(db, target_date)
    inst_20d = _fetch_inst_20d_net(db, target_date)
    shares = _fetch_shares_outstanding(db, target_date)

    rows: list[dict[str, Any]] = []
    for sid, excluded in universe.items():
        if stock_ids and sid not in stock_ids:
            continue
        if excluded is not None:
            continue   # signal 表只記真實 trigger,排除股不入

        yoy = revenue_yoy.get(sid)
        inst_net = inst_20d.get(sid)
        sh = shares.get(sid)

        if yoy is None or inst_net is None or sh is None or sh <= 0:
            continue

        inst_pct = inst_net / sh

        # Positive trigger
        if yoy > POSITIVE_YOY_THRESHOLD and inst_net > 0:
            rows.append({
                "market": "TW", "stock_id": sid, "date": target_date,
                "trigger_type": "positive",
                "revenue_yoy_pct": yoy,
                "institutional_20d": inst_net,
                "shares_outstanding": sh,
                "institutional_pct": inst_pct,
                "action_hint": "increase_20pct",
                "detail": {
                    "rationale": (
                        f"revenue_yoy={yoy:.1f}% > {POSITIVE_YOY_THRESHOLD}% "
                        f"+ institutional_20d net buy"
                    ),
                },
            })

        # Negative trigger
        if yoy < NEGATIVE_YOY_THRESHOLD and inst_net < 0 \
                and abs(inst_pct) > NEGATIVE_SELL_PCT:
            rows.append({
                "market": "TW", "stock_id": sid, "date": target_date,
                "trigger_type": "negative",
                "revenue_yoy_pct": yoy,
                "institutional_20d": inst_net,
                "shares_outstanding": sh,
                "institutional_pct": inst_pct,
                "action_hint": "decrease_50pct",
                "detail": {
                    "rationale": (
                        f"revenue_yoy={yoy:.1f}% < {NEGATIVE_YOY_THRESHOLD}% "
                        f"+ institutional sell > {NEGATIVE_SELL_PCT * 100}% of shares"
                    ),
                },
            })

    written = upsert_silver(db, OUTPUT_TABLE, rows,
                            pk_cols=["market", "stock_id", "date", "trigger_type"])
    elapsed_ms = int((time.monotonic() - start) * 1000)
    logger.info(f"[{NAME}] triggers={len(rows)} written={written} ({elapsed_ms}ms)")
    return {"name": NAME, "rows_read": len(rows), "rows_written": written,
            "elapsed_ms": elapsed_ms}
