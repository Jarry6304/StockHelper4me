"""StockHelper4me Golden L3 唯讀 Web API(v4.32)。

純讀 / 切片 structural_snapshots(cores + fusion)+ price / indicator / ranked。
neely forest 原樣 passthrough(snapshot::text)+ gzip/brotli 壓縮協商,渲染深度歸前端。
對齊 m3Spec/read-api.md。

跑:`uvicorn web_api.app:app`(需 `pip install -e ".[web]"`)。
"""

from web_api.app import app, create_app

__all__ = ["app", "create_app"]
