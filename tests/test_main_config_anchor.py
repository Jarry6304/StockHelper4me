"""Regression:main.py config 預設路徑從任意 cwd 都解析得到(v4.33 .env 同類)。

`--config` / `--stock-list` 預設值是 cwd 相對(config/collector.toml);從 repo 外
cwd(排程任務 / %TEMP%)啟動時 `main.py status` 炸「找不到設定檔」。
鎖定 `_anchor_to_repo_root`:cwd 找不到的相對路徑退回 repo root 解析;
顯式絕對路徑與 cwd 下存在的相對路徑原值不動。
"""

from __future__ import annotations

import os
from pathlib import Path

from main import _anchor_to_repo_root


def test_default_config_resolves_from_foreign_cwd(tmp_path, monkeypatch):
    monkeypatch.chdir(tmp_path)  # 模擬 %TEMP% 等 repo 外 cwd
    out = Path(_anchor_to_repo_root("config/collector.toml"))
    assert out.is_absolute()
    assert out.exists(), "預設 config 路徑須退回 repo root 解析"


def test_cwd_relative_hit_wins(tmp_path, monkeypatch):
    """cwd 下真的有該相對路徑 → 原值不動(本地覆寫 config 的工作流不受影響)。"""
    monkeypatch.chdir(tmp_path)
    (tmp_path / "config").mkdir()
    (tmp_path / "config" / "collector.toml").write_text("x", encoding="utf-8")
    assert _anchor_to_repo_root("config/collector.toml") == "config/collector.toml"


def test_absolute_path_untouched(tmp_path):
    p = str(tmp_path / "nonexistent.toml")
    assert os.path.isabs(p)
    assert _anchor_to_repo_root(p) == p
