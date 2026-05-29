"""sync psycopg ConnectionPool(lifespan singleton)+ get_pool 依賴。

v4.32 後改 **sync** pool(原 async 在 Windows + Python 3.14 撞 ProactorEventLoop
不相容 psycopg async)。sync psycopg 走 thread 阻塞 I/O,與 event loop 無關 →
跨 OS / Python 版本穩定;FastAPI 把 sync handler 丟 threadpool 跑,讀 API I/O 輕量足夠。

對齊 src/fusion/raw/_db.py 的 DATABASE_URL / .env 解析。
"""

from __future__ import annotations

import os
from pathlib import Path
from typing import Any

_pool: Any = None


def _resolve_url(database_url: str | None = None) -> str:
    if database_url:
        return database_url
    try:
        from dotenv import load_dotenv

        env_path = Path(__file__).resolve().parent.parent.parent / ".env"
        if env_path.exists():
            load_dotenv(env_path)
    except ImportError:
        pass
    url = os.getenv("DATABASE_URL")
    if not url:
        raise RuntimeError("DATABASE_URL 未設定(env 或 repo root .env)")
    return url


def open_pool(database_url: str | None = None) -> Any:
    """開 sync ConnectionPool(lifespan startup)。"""
    global _pool
    from psycopg.rows import dict_row
    from psycopg_pool import ConnectionPool

    url = _resolve_url(database_url)
    _pool = ConnectionPool(
        url, min_size=1, max_size=10,
        kwargs={"row_factory": dict_row, "autocommit": True},
        open=False,
    )
    _pool.open()
    return _pool


def close_pool() -> None:
    """關 pool(lifespan shutdown)。"""
    global _pool
    if _pool is not None:
        _pool.close()
        _pool = None


def get_pool() -> Any:
    """FastAPI 依賴:回現役 pool。測試以 dependency_overrides 注入 fake pool。"""
    if _pool is None:
        raise RuntimeError("connection pool not opened (lifespan not started)")
    return _pool
