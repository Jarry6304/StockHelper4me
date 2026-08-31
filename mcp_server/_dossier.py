"""Tool 1 內部:`neely_forecast` — 引擎證據 dossier(m3Spec/wave_judgment_loop.md §4)。

v4.39 起取代舊 `_forecast.py` 的 primary/prob 路徑:讀者面零 primary,
`primary_scenario` / `scenario_count` / `scenario_staleness` 三鍵**刪除**(非 rename);
回應 = dossier(三 timeframe live-edge 候選 + anchor_key + 證據三區 + active judgment)。
builder 實作在 `fusion.judgment.dossier`(web_api 共用);本檔只做 MCP 側轉接
(current_price 取價 + conn 生命週期)。
"""

from __future__ import annotations

from datetime import date
from typing import Any


def compute_neely_dossier(
    stock_id: str,
    as_of: date,
    *,
    database_url: str | None = None,
) -> dict[str, Any]:
    from fusion.judgment import build_dossier
    from fusion.raw import get_connection
    from mcp_server._price import fetch_latest_close_for_tool

    price_info = fetch_latest_close_for_tool(stock_id, as_of, database_url=database_url)
    current_price = price_info["close"] if price_info else None

    conn = get_connection(database_url)
    try:
        return build_dossier(
            conn, stock_id=stock_id, as_of=as_of, current_price=current_price,
        )
    finally:
        conn.close()
