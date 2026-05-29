"""FastAPI app factory — lifespan pool + 壓縮中介層 + routers。

跑:`uvicorn web_api.app:app`(需 `pip install -e ".[web]"` + DATABASE_URL)。
全端點唯讀 / 切片,零 compute(對齊 m3Spec/read-api.md)。
"""

from __future__ import annotations

import asyncio
import sys
from contextlib import asynccontextmanager

from fastapi import FastAPI

from web_api import pool as _pool
from web_api.compression import add_compression
from web_api.routers import market, screens, series, stocks

# Windows:uvicorn 預設 ProactorEventLoop,psycopg async pool 不相容
# (raise "Psycopg cannot use the 'ProactorEventLoop' to run in async mode")。
# import 時切 SelectorEventLoop(uvicorn import 本 module 早於建 loop → lifespan 開 pool
# 時已是 selector loop)。非 Windows no-op。
if sys.platform == "win32":
    asyncio.set_event_loop_policy(asyncio.WindowsSelectorEventLoopPolicy())


@asynccontextmanager
async def lifespan(app: FastAPI):
    await _pool.open_pool()
    try:
        yield
    finally:
        await _pool.close_pool()


def create_app() -> FastAPI:
    app = FastAPI(
        title="StockHelper4me Golden L3 API",
        version="4.32.0",
        summary="唯讀 Golden 層:cores + fusion(levels/resonance/climate)passthrough + 切片",
        lifespan=lifespan,
    )
    add_compression(app)

    @app.get("/health", tags=["meta"])
    async def health():
        return {"status": "ok", "service": "golden-l3-api"}

    app.include_router(stocks.router)
    app.include_router(series.router)
    app.include_router(market.router)
    app.include_router(screens.router)
    return app


app = create_app()
