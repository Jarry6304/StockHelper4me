"""v3.26(2026-05-17):從 price_daily 撈最新收盤 + 漲跌幅(authoritative source)。

對齊 user 2026-05-17 bug 報告:`stock_health` / `kalman_trend` / `neely_forecast`
的 `current_price` 從 indicator_latest 撈導致 stale 或 0.0 fallback。這 helper
**直讀 Bronze price_daily**,保證即時準確,不依賴 indicator 是否最近跑過。

設計:
- 取 <= as_of 最新一筆 close;順便算 prev_close → change_pct
- 共用 `fusion.raw._db.get_connection()`(對齊 v3.5 R5 C12 single entry)
- 失敗 graceful 回 None,caller fallback 0.0(對齊既有行為)

呼叫端:`mcp_server/{_health,_kalman,_forecast}.py`。
"""

from __future__ import annotations

from datetime import date
from typing import Any


def fetch_latest_close_for_tool(
    stock_id: str,
    as_of: date,
    *,
    database_url: str | None = None,
) -> dict[str, Any] | None:
    """Open conn → 撈 fetch_latest_close → close conn(self-contained)。

    Args:
        stock_id:     股票代號
        as_of:        上界
        database_url: 可選 PG 連線字串

    Returns:
        dict {date, close, prev_close, change_pct} 或 None(無資料)。
        日期為 ISO 字串;close / prev_close 為 float;change_pct % 含 sign。
    """
    from fusion.raw._db import fetch_latest_close, get_connection

    conn = get_connection(database_url)
    try:
        return fetch_latest_close(conn, stock_id=stock_id, as_of=as_of)
    finally:
        conn.close()
