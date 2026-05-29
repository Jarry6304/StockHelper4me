"""壓縮中介層 — brotli + gzip content negotiation(Accept-Encoding)。

brotli-asgi 的 BrotliMiddleware 自帶 gzip_fallback(對不支援 brotli 的 client 退 gzip)。
structural_snapshots(neely forest)壓縮率高 → 顯著省頻寬。缺 brotli-asgi 時退純 gzip。
"""

from __future__ import annotations

from typing import Any


def add_compression(app: Any) -> None:
    try:
        from brotli_asgi import BrotliMiddleware

        # quality 4-5 對 JSON 壓縮 / CPU 折衷佳;minimum_size 過濾小 payload
        app.add_middleware(
            BrotliMiddleware, quality=4, minimum_size=512, gzip_fallback=True,
        )
    except ImportError:
        from starlette.middleware.gzip import GZipMiddleware

        app.add_middleware(GZipMiddleware, minimum_size=512)
