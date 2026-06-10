"""DSN / .env 解析單一真相源(P2-2)。

repo 內所有「載 repo-root .env + 解 DATABASE_URL」邏輯收斂到本檔
(v4.33 fusion 路徑 bug 的根因即多份副本間層數漂移)。
連線「策略」不在本檔職責 — 寫 pool(db.py)/ 唯讀(fusion.raw)/
per-request(web_api)各自維持。

例外:alembic/env.py 保留等義 8 行鏡像(零安裝自足;改本檔語意必同步該處)。
"""

from __future__ import annotations

import os
from pathlib import Path

# 不變量:本檔在 src/ 第一層;搬位必同步 parent 層數。
REPO_ROOT = Path(__file__).resolve().parent.parent


def load_repo_env() -> None:
    """載入 repo-root .env(若存在;不覆蓋既有環境變數)。

    純副作用:供只需 .env 變數(如 FINMIND_TOKEN)而不需 DATABASE_URL 的
    進入點呼叫。python-dotenv 未安裝則靜默略過。
    """
    try:
        from dotenv import load_dotenv
    except ImportError:
        return
    env_path = REPO_ROOT / ".env"
    if env_path.exists():
        load_dotenv(env_path)


def resolve_database_url(explicit: str | None = None) -> str:
    """解出 Postgres 連線字串:explicit > 環境變數;皆無 → RuntimeError。

    **無條件先 load_repo_env()** — 載 .env 的副作用是既有 feature
    (入口靠它帶入 FINMIND_TOKEN 等其他變數),explicit 有值也不得短路跳過。

    Args:
        explicit: 顯式連線字串;None 時走環境變數 / .env。

    Raises:
        RuntimeError: 兩種來源皆無 DATABASE_URL。
    """
    load_repo_env()
    url = explicit or os.getenv("DATABASE_URL")
    if not url:
        raise RuntimeError(
            "DATABASE_URL 未設定。請執行以下任一:\n"
            "  1. export DATABASE_URL=postgresql://twstock:twstock@localhost:5432/twstock\n"
            "  2. 在 .env 檔設定 DATABASE_URL(配合 python-dotenv)"
        )
    return url
