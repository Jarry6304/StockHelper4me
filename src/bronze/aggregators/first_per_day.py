"""
bronze/aggregators/first_per_day.py
====================================
v3.20(2026-05-17):intraday → daily 收斂 aggregator。

對 FinMind 5-分鐘 / 1-分鐘等 intraday dataset(初版只用在 `GoldPrice`),
依「每日只存第一筆」規則(user 拍版 2026-05-17)。

行為:
- 接 raw_rows(field_mapper 已 transform 過,含 market/source/commodity/date 欄)
- date 欄可能是 "YYYY-MM-DD HH:MM:SS" 字串(FinMind intraday)或 NaiveDate
- group by (commodity, date::date) → 取「最小 datetime」那筆
- 回去 date 改寫為純 DATE(去掉 time component),保留 price 欄
"""
from __future__ import annotations

from typing import Any


def aggregate_first_per_day(
    rows: list[dict[str, Any]],
    *,
    time_key: str = "date",
    inject_commodity: str = "GOLD",
) -> list[dict[str, Any]]:
    """每日多筆 → 每日 1 筆(取時間最早那筆)。

    Args:
        rows:             field_mapper 輸出的 row list
        time_key:         時間欄位名(FinMind intraday dataset 用 "date")
        inject_commodity: 注入 commodity 欄值(v3.20 hardcode "GOLD";
                          未來擴 silver / oil 時 generalize 走 collector.toml)

    Returns:
        每個 (commodity, date::date) 取最早一筆;date 改寫為 'YYYY-MM-DD'。

    Notes:
        - 容錯:date 為純 YYYY-MM-DD 視同無 time component → 取該值
        - empty rows → 回 []
        - rows 已有 commodity 欄則不覆蓋(向後相容)
    """
    if not rows:
        return []

    # group_key → (sort_value, row)
    # sort_value 用原始 time string 排序(ISO format lex sort = time sort)
    best: dict[tuple, tuple[str, dict[str, Any]]] = {}

    for row in rows:
        raw_dt = row.get(time_key)
        if raw_dt is None:
            continue
        dt_str = str(raw_dt)
        date_only = dt_str.split(" ")[0] if " " in dt_str else dt_str

        # 注入 commodity(rows 沒有此欄時)
        row_copy = dict(row)
        if "commodity" not in row_copy:
            row_copy["commodity"] = inject_commodity
        row_copy[time_key] = date_only

        gkey = (row_copy["commodity"], date_only)

        prev = best.get(gkey)
        if prev is None or dt_str < prev[0]:
            best[gkey] = (dt_str, row_copy)

    return [v[1] for v in best.values()]
