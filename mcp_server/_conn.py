"""共用 PG connection 取得 — reuse agg.get_connection。

MCP server 跑 stdio 是長壽 process,每個 tool 呼叫各自開 connection 後關掉
(對齊 src/agg/query.py owns_conn 模式),避免長壽 connection 跨工具 leak。

未來可換成 connection pool — 介面不變。
"""

from __future__ import annotations

import sys
from pathlib import Path

# 對齊 dashboards/aggregation.py:確保從 repo root 跑 python -m mcp_server 時
# src/(放 agg/ silver/ bronze/)+ repo root(放 dashboards/)都在 sys.path。
_REPO_ROOT = Path(__file__).resolve().parent.parent
_SRC_ROOT = _REPO_ROOT / "src"
if str(_SRC_ROOT) not in sys.path:
    sys.path.insert(0, str(_SRC_ROOT))
if str(_REPO_ROOT) not in sys.path:
    sys.path.insert(0, str(_REPO_ROOT))


def get_connection(database_url: str | None = None):
    """Thin wrapper around agg._db.get_connection。

    Args:
        database_url: 可選的 PG 連線字串(優先序高於 env / .env)。

    Returns:
        psycopg.Connection(autocommit=True, row_factory=dict_row)。
    """
    from agg._db import get_connection as _agg_get_connection

    return _agg_get_connection(database_url)
