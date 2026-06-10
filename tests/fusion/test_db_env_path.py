"""Regression: DSN / .env 解析的 repo-root 路徑與單一真相源(v4.33 → P2-2 遷移)。

v4.33 原 bug:fusion.raw._db 自有副本上溯層數錯,.env 只解到 src/。
P2-2 後 _db 委派 src/dsn.py — 本檔改鎖兩個新不變量:
1. dsn.REPO_ROOT 解到 repo root(pyproject.toml marker,不依賴 .env 存在)
2. _db 不得長回自有 .env 解析副本(屬性 + 原始碼雙重掃描)

不打真 DB、不需 .env 存在。
"""

from __future__ import annotations

import inspect


def test_dsn_repo_root_points_to_repo_root():
    import dsn

    p = dsn.REPO_ROOT
    # repo root marker(不依賴 .env 是否存在)
    assert (p / "pyproject.toml").exists(), f"{p} 不是 repo root"
    # regression-lock:絕不可只解到 src/(v4.33 原 bug 形態)
    assert p.name != "src"


def test_fusion_db_has_no_local_env_copy():
    from fusion.raw import _db

    # P2-2 前的自有副本不得長回來
    assert not hasattr(_db, "_repo_root_env_path"), (
        "_db 長回自有 .env 路徑副本;DSN 解析單一真相源在 src/dsn.py"
    )
    src = inspect.getsource(_db)
    assert "load_dotenv" not in src, "_db 不得自行載 .env;委派 dsn.resolve_database_url"
