"""StockHelper4me Golden L3 唯讀 Web API(v4.32)。

純讀 / 切片 structural_snapshots(cores + fusion)+ price / indicator / ranked。
neely forest 原樣 passthrough(snapshot::text)+ gzip/brotli 壓縮協商,渲染深度歸前端。
對齊 m3Spec/read-api.md。

跑:`uvicorn web_api.app:app`(需 `pip install -e ".[web]"`)。
"""

# 對齊 mcp_server/__init__.py / dashboards/aggregation.py 同一 sys.path 模式 —
# 確保 src/ 在 sys.path:editable install 的模組對映是裝機當下的快照,之後新增的
# 頂層 loose module(如 P2-2 的 src/dsn.py)不在表內,uvicorn 環境會
# ModuleNotFoundError(2026-06-11 /waves/summary 首個打 DB 的端點實踩)。
import sys as _sys
from pathlib import Path as _Path

_SRC_ROOT = _Path(__file__).resolve().parent.parent
if str(_SRC_ROOT) not in _sys.path:
    _sys.path.insert(0, str(_SRC_ROOT))

from web_api.app import app, create_app

__all__ = ["app", "create_app"]
