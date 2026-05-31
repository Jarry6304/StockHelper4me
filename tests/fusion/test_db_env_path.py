"""Regression: fusion.raw._db 的 .env 路徑須解到 repo root,不是 src/。

對齊 v4.32 後 streamlit 乾淨啟動撞到 DATABASE_URL 未設定的 bug —— get_connection
舊碼 parent.parent.parent 只上溯到 src/,找不到 repo root 的 .env。本 test 不打真
DB、不需 .env 存在,純驗路徑層數正確。
"""

from __future__ import annotations


def test_repo_root_env_path_points_to_repo_root():
    from fusion.raw._db import _repo_root_env_path

    p = _repo_root_env_path()
    assert p.name == ".env"
    # repo root marker(不依賴 .env 是否存在)
    assert (p.parent / "pyproject.toml").exists(), f"{p.parent} 不是 repo root"
    # regression-lock:絕不可只解到 src/
    assert p.parent.name != "src"
