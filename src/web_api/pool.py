"""每請求 sync 連線依賴(無 pool / 無 async / 無 event loop)。

v4.32 hotfix:Windows + Python 3.14 下 uvicorn 在已建好的 ProactorEventLoop *內* 才
lazy import app,async psycopg(甚至 sync ConnectionPool 的開啟時機)反覆被 event-loop
議題絆住。改用 repo 既有、且 MCP serving 已 production-verified 的 sync
`fusion.raw._db.get_connection`,**每請求開 / 關一條連線**(讀 API I/O 輕量,可接受)。
完全不碰 asyncio → Windows/Linux × 任何 Python 版本都穩。檔名沿用 pool.py 避免 import 大改。
"""

from __future__ import annotations

from typing import Any, Iterator


def db_conn() -> Iterator[Any]:
    """FastAPI 依賴:yield 一條 sync psycopg 連線(dict_row, autocommit),請求結束關閉。

    走 fusion.raw._db.get_connection —— 與 MCP 工具(stock_levels / dual_track_resonance /
    market_context,production verify Step 4 全綠)同一條 sync 路徑。
    """
    from fusion.raw._db import get_connection

    conn = get_connection()
    try:
        yield conn
    finally:
        conn.close()
