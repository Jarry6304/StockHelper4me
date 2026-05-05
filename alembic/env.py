"""
Alembic env.py — tw-stock-collector 客製版

設計要點:
1. 連線字串從環境變數 DATABASE_URL 讀(支援 .env 檔)
2. 不用 SQLAlchemy ORM,target_metadata = None
3. Migration 用純 SQL(op.execute() 或 op.create_table 等 schema-level API)
4. 兼容 Postgres + SQLite(SQLite 在 fallback 模式下也可以跑 migration)
"""
import os
import sys
from logging.config import fileConfig
from pathlib import Path

from sqlalchemy import engine_from_config, pool

from alembic import context

# 載入 .env(若存在)
try:
    from dotenv import load_dotenv

    project_root = Path(__file__).resolve().parent.parent
    env_path = project_root / ".env"
    if env_path.exists():
        load_dotenv(env_path)
except ImportError:
    pass  # python-dotenv 未安裝也無妨,直接讀 os.environ

# Alembic Config 物件
config = context.config

# 從環境變數覆蓋 sqlalchemy.url(忽略 alembic.ini 中的設定)
db_url = os.getenv("DATABASE_URL")
if db_url:
    # SQLAlchemy 不認 "postgresql://...",要轉成 "postgresql+psycopg://..."
    # 因為我們用 psycopg3 driver
    if db_url.startswith("postgresql://"):
        db_url = db_url.replace("postgresql://", "postgresql+psycopg://", 1)
    config.set_main_option("sqlalchemy.url", db_url)
elif not config.get_main_option("sqlalchemy.url"):
    raise RuntimeError(
        "DATABASE_URL 未設定。請執行:\n"
        "  export DATABASE_URL=postgresql://twstock:twstock@localhost:5432/twstock\n"
        "或在 .env 檔中設定。"
    )

# Logging 設定(從 alembic.ini)
if config.config_file_name is not None:
    fileConfig(config.config_file_name)

# 不使用 SQLAlchemy ORM model,純 SQL migration
target_metadata = None


def run_migrations_offline() -> None:
    """Offline 模式:不連 DB,只產生 SQL 字串。
    用法:alembic upgrade head --sql > migration.sql
    """
    url = config.get_main_option("sqlalchemy.url")
    context.configure(
        url=url,
        target_metadata=target_metadata,
        literal_binds=True,
        dialect_opts={"paramstyle": "named"},
    )

    with context.begin_transaction():
        context.run_migrations()


def run_migrations_online() -> None:
    """Online 模式:連 DB 執行 migration。"""
    connectable = engine_from_config(
        config.get_section(config.config_ini_section, {}),
        prefix="sqlalchemy.",
        poolclass=pool.NullPool,  # migration 不需要 pool
    )

    with connectable.connect() as connection:
        context.configure(
            connection=connection,
            target_metadata=target_metadata,
            # 純 SQL migration 不需要這些
            compare_type=False,
            compare_server_default=False,
        )

        with context.begin_transaction():
            context.run_migrations()


if context.is_offline_mode():
    run_migrations_offline()
else:
    run_migrations_online()
