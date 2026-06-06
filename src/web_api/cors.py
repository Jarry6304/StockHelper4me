"""CORS 中介層 — 讓 Svelte dev(:5173)等跨來源前端可打 API。

預設僅放 Vite dev origin(`http://localhost:5173`)。
prod 走環境變數 `WEB_API_CORS_ORIGINS`(逗號分隔)覆寫。
"""

from __future__ import annotations

import os
from typing import Any

DEFAULT_DEV_ORIGIN = "http://localhost:5173"


def _parse_origins() -> list[str]:
    raw = os.environ.get("WEB_API_CORS_ORIGINS", "").strip()
    if not raw:
        return [DEFAULT_DEV_ORIGIN]
    return [o.strip() for o in raw.split(",") if o.strip()]


def add_cors(app: Any) -> None:
    from fastapi.middleware.cors import CORSMiddleware

    app.add_middleware(
        CORSMiddleware,
        allow_origins=_parse_origins(),
        allow_methods=["GET", "OPTIONS"],
        allow_headers=["*"],
        max_age=600,
    )
