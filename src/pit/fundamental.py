"""PIT views over Bronze fundamental tables.

v0.3 phase 2(alembic b8c9d0e1f2g3,2026-05-23):3 張 Bronze 表加 `report_date`
欄。本模組改讀新欄,NULL 時 fallback fact_date + heuristic lag,維持向下相容。

Heuristic lag(對齊 _lookahead.py 既有約定):
  - monthly_revenue:   row.date + 11 days  (publish lag ~10th of next month)
  - business_indicator: row.date + 27 days  (國發會公布日 ~27 of next month)
  - financial_statement: row.date + 45 days  (T+45 fallback)

SQL filter 用 COALESCE(report_date, date + heuristic_lag) ≤ asof_t,讓有真實
publish-date 的 row 用真實值,沒有的走 heuristic — 同一條 SQL 處理。
"""

from __future__ import annotations

from datetime import date, timedelta
from typing import Any

MONTHLY_REVENUE_LAG = timedelta(days=11)
BUSINESS_INDICATOR_LAG = timedelta(days=27)
FINANCIAL_STATEMENT_LAG = timedelta(days=45)


def asof_revenue(
    conn,
    stock_id: str,
    asof_t: date,
    lookback_months: int = 24,
    market: str = "TW",
) -> list[dict[str, Any]]:
    """As-of-T view of monthly revenue.

    Filter: COALESCE(report_date, date + 11d) ≤ asof_t
    報酬 row 含 report_date 欄(真實值或 NULL,後者代表 heuristic fallback)。
    """
    # 寬鬆下界足以包住 publish lag(report_date 可能延後最多 ~45 天)
    earliest = asof_t - timedelta(days=lookback_months * 31 + 60)
    with conn.cursor() as cur:
        cur.execute(
            """SELECT date, revenue, revenue_year, revenue_month,
                      country, create_time, report_date, detail
               FROM monthly_revenue
               WHERE market = %s AND stock_id = %s
                 AND date >= %s
                 AND COALESCE(report_date, date + INTERVAL '11 days') <= %s
               ORDER BY date""",
            (market, stock_id, earliest, asof_t),
        )
        return list(cur.fetchall())


def asof_financial(
    conn,
    stock_id: str,
    asof_t: date,
    lookback_quarters: int = 8,
    market: str = "TW",
) -> list[dict[str, Any]]:
    """As-of-T view of financial statement(income/balance/cashflow).

    Filter: COALESCE(report_date, date + 45d) ≤ asof_t
    """
    earliest = asof_t - timedelta(days=lookback_quarters * 92 + 60)
    with conn.cursor() as cur:
        cur.execute(
            """SELECT date, event_type, type, origin_name, value, report_date
               FROM financial_statement
               WHERE market = %s AND stock_id = %s
                 AND date >= %s
                 AND COALESCE(report_date, date + INTERVAL '45 days') <= %s
               ORDER BY date, event_type, type, origin_name""",
            (market, stock_id, earliest, asof_t),
        )
        return list(cur.fetchall())


def asof_business_indicator(
    conn,
    asof_t: date,
    lookback_months: int = 24,
    market: str = "tw",
) -> list[dict[str, Any]]:
    """As-of-T view of business indicator(月頻,market-level)。

    Filter: COALESCE(report_date, date + 27d) ≤ asof_t
    """
    earliest = asof_t - timedelta(days=lookback_months * 31 + 60)
    with conn.cursor() as cur:
        cur.execute(
            """SELECT date, leading_indicator, coincident_indicator,
                      lagging_indicator, monitoring, monitoring_color,
                      report_date, detail
               FROM business_indicator_tw
               WHERE market = %s
                 AND date >= %s
                 AND COALESCE(report_date, date + INTERVAL '27 days') <= %s
               ORDER BY date""",
            (market, earliest, asof_t),
        )
        return list(cur.fetchall())
