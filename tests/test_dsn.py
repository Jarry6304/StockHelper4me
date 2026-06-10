"""src/dsn.py — DSN / .env 解析單一真相源(P2-2)語意表整表回歸。

不打真 DB;以 tmp_path 假 repo root + save-restore 隔離 env 污染
(load_dotenv 會持久寫 os.environ,monkeypatch.delenv 對原本不存在的
key 不留還原紀錄,故用顯式 save-restore fixture)。
"""

from __future__ import annotations

import os

import pytest

import dsn


@pytest.fixture()
def clean_env(monkeypatch, tmp_path):
    """隔離 DATABASE_URL / FINMIND_TOKEN + 假 repo root(tmp_path)。"""
    saved = {k: os.environ.get(k) for k in ("DATABASE_URL", "FINMIND_TOKEN")}
    for k in saved:
        os.environ.pop(k, None)
    monkeypatch.setattr(dsn, "REPO_ROOT", tmp_path)
    yield tmp_path
    for k, v in saved.items():
        if v is None:
            os.environ.pop(k, None)
        else:
            os.environ[k] = v


class TestResolveDatabaseUrl:
    def test_explicit_does_not_short_circuit_env_side_effect(self, clean_env):
        """explicit 有值仍先載 .env — FINMIND_TOKEN 副作用回歸(規格語意表列 1)。"""
        (clean_env / ".env").write_text(
            "DATABASE_URL=postgresql://from-dotenv/db\nFINMIND_TOKEN=tok-from-env-file\n",
            encoding="utf-8",
        )
        url = dsn.resolve_database_url("postgresql://explicit/db")
        assert url == "postgresql://explicit/db"
        assert os.environ.get("FINMIND_TOKEN") == "tok-from-env-file"

    def test_env_var_wins_over_dotenv(self, clean_env):
        """環境變數已設、.env 值不同 → 環境變數為準(載 .env 不覆蓋)。"""
        (clean_env / ".env").write_text(
            "DATABASE_URL=postgresql://from-dotenv/db\n", encoding="utf-8",
        )
        os.environ["DATABASE_URL"] = "postgresql://from-envvar/db"
        assert dsn.resolve_database_url() == "postgresql://from-envvar/db"

    def test_dotenv_only(self, clean_env):
        """僅 .env 有值 → 載入後從環境變數取得。"""
        (clean_env / ".env").write_text(
            "DATABASE_URL=postgresql://from-dotenv/db\n", encoding="utf-8",
        )
        assert dsn.resolve_database_url() == "postgresql://from-dotenv/db"

    def test_missing_everywhere_raises_with_dual_hint(self, clean_env):
        """全無 → RuntimeError,訊息含兩種設定方式。"""
        with pytest.raises(RuntimeError) as exc:
            dsn.resolve_database_url()
        msg = str(exc.value)
        assert "export DATABASE_URL" in msg
        assert ".env" in msg

    def test_load_repo_env_no_file_is_noop(self, clean_env):
        """無 .env 檔 → load_repo_env 無聲 no-op,環境不變。"""
        dsn.load_repo_env()
        assert "DATABASE_URL" not in os.environ
        assert "FINMIND_TOKEN" not in os.environ
