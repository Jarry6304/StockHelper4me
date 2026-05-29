"""FastAPI app factory — 壓縮中介層 + routers(每請求 sync conn,無 pool / 無 lifespan)。

跑:`uvicorn web_api.app:app`(需 `pip install -e ".[web]"` + DATABASE_URL)。
全端點唯讀 / 切片,零 compute(對齊 m3Spec/read-api.md)。handler 為 sync(FastAPI
threadpool)+ 每請求 sync psycopg conn(fusion.raw._db.get_connection)→ 完全不碰 asyncio
event loop(Windows ProactorEventLoop / Python 3.14 安全)。
"""

from __future__ import annotations

from fastapi import FastAPI

from web_api.compression import add_compression
from web_api.routers import market, screens, series, stocks


def create_app() -> FastAPI:
    app = FastAPI(
        title="StockHelper4me Golden L3 API",
        version="4.32.0",
        summary="唯讀 Golden 層:cores + fusion(levels/resonance/climate)passthrough + 切片",
    )
    add_compression(app)

    @app.get("/health", tags=["meta"])
    def health():
        return {"status": "ok", "service": "golden-l3-api"}

    app.include_router(stocks.router)
    app.include_router(series.router)
    app.include_router(market.router)
    app.include_router(screens.router)
    return app


app = create_app()
