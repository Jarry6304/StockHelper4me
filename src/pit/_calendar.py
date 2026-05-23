"""Trading-calendar helpers for PIT layer.

Reads `trading_date_ref` (renamed from `trading_calendar` per v3.2 R-1).
"""

from __future__ import annotations

from datetime import date


def trading_days_between(
    conn,
    start: date,
    end: date,
    market: str = "TW",
) -> list[date]:
    """Return ascending list of trading days in [start, end] inclusive."""
    with conn.cursor() as cur:
        cur.execute(
            """SELECT date FROM trading_date_ref
               WHERE market = %s AND date BETWEEN %s AND %s
               ORDER BY date""",
            (market, start, end),
        )
        rows = cur.fetchall()
    # rows are dict_row; key is "date"
    return [r["date"] for r in rows]
