"""
bronze/_common.py
=================
Bronze 層共用 helper(v3.5 R1 C2 抽出)。
"""

import logging
from typing import Any

logger = logging.getLogger("collector.bronze._common")


def filter_to_trading_days(
    rows: list[dict[str, Any]],
    trading_dates: set[str],
    label: str,
) -> list[dict[str, Any]]:
    """過濾掉 date 不在 trading_dates 集合內的 rows,並記錄被丟掉的日期。

    用於對抗 FinMind 在週六回非交易日的鬼資料(institutional API 已知有此現象)。

    安全閥:trading_dates 為空(trading_calendar 表還沒灌資料)時不過濾,避免
    把整批資料都當鬼資料丟掉。
    """
    if not trading_dates:
        logger.warning(
            f"[{label}] trading_dates 為空(trading_calendar 表未填充?)"
            f",跳過非交易日過濾"
        )
        return rows

    kept: list[dict[str, Any]] = []
    dropped_dates: set[str] = set()
    for row in rows:
        d = row.get("date")
        if d is None or d in trading_dates:
            kept.append(row)
        else:
            dropped_dates.add(d)
    if dropped_dates:
        logger.warning(
            f"[{label}] FinMind 回了 {len(dropped_dates)} 個非交易日的資料,"
            f"已過濾:{sorted(dropped_dates)}"
        )
    return kept
