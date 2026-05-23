"""PIT views over Bronze fundamental tables.

Phase 1 placeholder: `report_date` is not yet a real column on Bronze
(monthly_revenue_tw / financial_statement_tw / business_indicator_tw); it lives in
facts.metadata or is computed via heuristic.  This module uses the same heuristic
as `src/fusion/raw/_lookahead.py`:

  - monthly_revenue:     row.date + 11 days  (publish lag ~10th of next month)
  - business_indicator:  row.date + 27 days  (publish lag ~27th of next month)
  - financial_statement: row.date + 45 days  (T+45 fallback for quarterlies)

Phase 2 will promote `report_date` to a real Bronze column; at that point this
module switches to read the column directly and the heuristic is removed.
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

    Filter: date <= asof_t AND date + 11d <= asof_t (publish lag).
    Returns rows ordered ascending by date.

    Note: This reads Bronze raw `monthly_revenue_tw`. Detail JSONB is preserved
    as-is.  Phase 2 will switch to reading explicit `report_date` column.
    """
    # The publish-lag filter is the binding constraint; date-only filter is
    # always weaker, so just enforce date + 11d <= asof_t.
    max_fact_date = asof_t - MONTHLY_REVENUE_LAG
    # Cheap lookback bound (28 days per month upper-bound is safe)
    earliest = asof_t - timedelta(days=lookback_months * 31 + 11)
    with conn.cursor() as cur:
        cur.execute(
            """SELECT date, revenue, revenue_year, revenue_month, detail
               FROM monthly_revenue_tw
               WHERE market = %s AND stock_id = %s
                 AND date >= %s AND date <= %s
               ORDER BY date""",
            (market, stock_id, earliest, max_fact_date),
        )
        return list(cur.fetchall())


def asof_financial(
    conn,
    stock_id: str,
    asof_t: date,
    lookback_quarters: int = 8,
    market: str = "TW",
) -> list[dict[str, Any]]:
    """As-of-T view of financial statement (income/balance/cashflow).

    Filter: date + 45d <= asof_t (T+45 publish lag fallback).
    Returns rows ordered ascending by (date, type, origin_name).
    """
    max_fact_date = asof_t - FINANCIAL_STATEMENT_LAG
    # 92 days per quarter upper-bound
    earliest = asof_t - timedelta(days=lookback_quarters * 92 + 45)
    with conn.cursor() as cur:
        cur.execute(
            """SELECT date, type, origin_name, value, detail
               FROM financial_statement_tw
               WHERE market = %s AND stock_id = %s
                 AND date >= %s AND date <= %s
               ORDER BY date, type, origin_name""",
            (market, stock_id, earliest, max_fact_date),
        )
        return list(cur.fetchall())
