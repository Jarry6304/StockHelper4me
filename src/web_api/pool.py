"""async psycopg connection pool(lifespan singleton)+ get_pool 依賴。

對齊 src/fusion/raw/_db.py 的 DATABASE_URL / .env 解析,但走 AsyncConnectionPool
(唯讀 API 全 async handler)。
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


async def open_pool(database_url: str | None = None) -> Any:
    """開 AsyncConnectionPool(lifespan startup)。"""
    global _pool
    from psycopg.rows import dict_row
    from psycopg_pool import AsyncConnectionPool

    url = _resolve_url(database_url)
    _pool = AsyncConnectionPool(
        url, min_size=1, max_size=10,
        kwargs={"row_factory": dict_row, "autocommit": True},
        open=False,
    )
    await _pool.open()
    return _pool


async def close_pool() -> None:
    """關 pool(lifespan shutdown)。"""
    global _pool
    if _pool is not None:
        await _pool.close()
        _pool = None


def get_pool() -> Any:
    """FastAPI 依賴:回現役 pool。測試以 dependency_overrides 注入 fake pool。"""
    if _pool is None:
        raise RuntimeError("connection pool not opened (lifespan not started)")
    return _pool
