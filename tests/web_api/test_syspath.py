"""Regression:import web_api 必須把 src/ 插進 sys.path(editable 快照免疫)。

editable install 的模組對映是裝機當下快照 — 之後新增的頂層 loose module
(P2-2 src/dsn.py)不在表內;uvicorn 只 import web_api,首個打 DB 的端點
(2026-06-11 /waves/summary)在 get_connection 內 `from dsn import …` 即
ModuleNotFoundError。鎖定 web_api/__init__ 的 sys.path 插入(對齊 mcp_server /
dashboards 既有慣例),讓 uvicorn 環境不依賴 editable 對映新鮮度。
"""

from __future__ import annotations

import importlib
import sys
from pathlib import Path


def test_import_web_api_inserts_src_on_syspath(monkeypatch):
    import web_api

    src_root = Path(web_api.__file__).resolve().parent.parent
    # 模擬「src 不在 sys.path」的 uvicorn 環境(editable 對映缺新模組的情境 proxy)
    stripped = [p for p in sys.path if Path(p or ".").resolve() != src_root]
    monkeypatch.setattr(sys, "path", stripped)

    importlib.reload(web_api)

    assert str(src_root) in sys.path, "web_api/__init__ 須把 src/ 插回 sys.path"
    # dsn(P2-2 頂層 loose module)必須在此 path 下可解析 — 即實踩的失敗點
    assert importlib.util.find_spec("dsn") is not None
