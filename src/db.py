"""
db.py
------
DB 寫入抽象與實作層。

設計原則(v3.3,PG-only):

1. 抽象層採 typing.Protocol(structural typing)
   - 不強制繼承,任何符合 method signature 的 class 都是合格 DBWriter
   - 比 ABC 更彈性,允許將來多實作的內部行為差異

2. PostgresWriter(唯一實作,production)
   - psycopg3 + psycopg_pool.ConnectionPool(v3.3 升 connection pool)
   - 每個 method 走 `with self.pool.connection() as conn:` 取連線
   - INSERT ... ON CONFLICT DO UPDATE
   - JSONB / DATE / TIMESTAMPTZ 由 psycopg 自動處理
   - 寫入時依 _column_types 自動 cast,業務 code 不用煩惱型別
   - autocommit=True + 每個 method 用 `with conn.transaction()` 顯式邊界

3. v3.3 移除 SqliteWriter
   - 過去 v3.0 起已 deprecated(寫入 raise NotImplementedError,讀取留)
   - 全部 collector + Silver + Cores production path 走 PG;CI 無 SQLite fallback
   - 緊急情況可從 git history 還原(git show <pre-v3.3-commit>:src/db.py)

4. Schema 初始化
   - 走 Alembic migration(`alembic upgrade head`),不再內嵌 DDL 字串
   - init_schema() 比對 schema_metadata.schema_version 後觸發 alembic 遷移

5. _table_columns / _table_column_types 快取
   - _table_columns(table) -> set[str]:外部 API,給 field_mapper / upsert 用
   - _table_column_types(table) -> dict[str, str]:內部用,upsert 自動 cast 依據

連線字串(DATABASE_URL):
    postgresql://twstock:twstock@localhost:5432/twstock

Pool 大小:
    `DB_POOL_SIZE` env var(預設 max=8 / min=2)。並發 backfill(規格 4
    asyncio.gather + Semaphore(12))下,實測 8 conn 已足夠 — Bronze 寫入是
    short-lived transaction,conn release 快;Phase 7 Silver / Cores 各自串列
    走自己的 conn。
"""

from __future__ import annotations

import json
import logging
import os
from datetime import date, datetime
from decimal import Decimal
from pathlib import Path
from typing import Any, Protocol, runtime_checkable

logger = logging.getLogger("collector.db")

# Schema 版本,與 schema_metadata 表中的值對齊(由 alembic c2d3e4f5g6h7 bump 到 3.2)
# init_schema() 比對此常數判斷是否要跑 schema_pg.sql;rust_bridge EXPECTED_SCHEMA_VERSION
# 也鎖在同一值,schema 升版時三處(此處 / rust_bridge.py:31 / alembic migration)一起改
SCHEMA_VERSION = "3.2"


# =============================================================================
# 抽象介面(Protocol)
# =============================================================================

@runtime_checkable
class DBWriter(Protocol):
    """
    DB 寫入抽象。

    Structural typing — 任何具備這些 method 的物件都符合,不需 inherit。
    v3.3 起唯一實作:PostgresWriter。

    這層的職責邊界:
    - ✅ 提供「跟 DB 講話的低階 API」
    - ✅ 處理連線、transaction、型別 cast
    - ❌ 不抽象 schema(不知道 stock_info 有哪些欄位)
    - ❌ 不抽象業務邏輯(由業務 code 寫 SQL 字串)
    """

    def upsert(
        self,
        table: str,
        rows: list[dict[str, Any]],
        primary_keys: list[str],
    ) -> int:
        """批次 UPSERT。略過 schema 中不存在的欄位(API 欄位漂移容錯)。"""
        ...

    def insert(self, table: str, row: dict[str, Any]) -> None:
        """單筆 UPSERT(便捷 wrapper)。"""
        ...

    def query(self, sql: str, params: list[Any] | None = None) -> list[dict[str, Any]]:
        """執行 SELECT,回傳 dict list。SQL 用 %s 參數佔位符。"""
        ...

    def query_one(self, sql: str, params: list[Any] | None = None) -> dict[str, Any] | None:
        """執行 SELECT,回傳第一筆 dict 或 None。"""
        ...

    def update(self, sql: str, params: list[Any] | None = None) -> int:
        """執行 UPDATE / DELETE,回傳影響列數。"""
        ...

    def init_schema(self) -> None:
        """初始化 schema(觸發 Alembic migration 或對等機制)。"""
        ...

    def _table_columns(self, table: str) -> set[str]:
        """回傳指定資料表的有效欄位名稱集合(快取)。"""
        ...

    def _table_pks(self, table: str) -> list[str]:
        """回傳指定資料表的 PRIMARY KEY 欄位名稱列表(依 ordinal_position 排序)。"""
        ...

    def close(self) -> None:
        """關閉連線池。"""
        ...

    def __enter__(self) -> "DBWriter": ...
    def __exit__(self, exc_type, exc_val, exc_tb) -> bool: ...


# =============================================================================
# PostgresWriter — psycopg3 + ConnectionPool(v3.3)
# =============================================================================

class PostgresWriter:
    """
    psycopg3 同步 wrapper,基於 psycopg_pool.ConnectionPool。

    v3.3 升 ConnectionPool(從 v3.2 single-conn):
    - Spec 4 phase_executor `asyncio.gather` 並發跑 12 個 task,各自需要 conn
    - Spec 2 Silver builder 平行化要 multi-conn 才有意義
    - Spec 7 post_process batch SQL 改 single statement,但 N+1 砍掉後並發仍有需要

    Pool 設計:
    - min=2 / max=8(預設;env DB_POOL_SIZE 覆蓋 max)
    - autocommit=True + dict_row factory
    - 每個 method 在 with self.pool.connection() 內取 conn,結束自動 release
    - 每個寫入用 with conn.transaction() 明示 BEGIN/COMMIT 邊界
    """

    def __init__(self, connection_url: str, pool_max: int | None = None):
        try:
            import psycopg
            from psycopg.rows import dict_row
            from psycopg_pool import ConnectionPool
        except ImportError as e:
            raise RuntimeError(
                "psycopg / psycopg-pool not installed. "
                "Run: pip install 'psycopg[binary]>=3.2' 'psycopg-pool>=3.2'"
            ) from e

        self._psycopg = psycopg
        self._dict_row = dict_row
        self.url = connection_url

        max_size = pool_max or int(os.getenv("DB_POOL_SIZE", "8"))
        min_size = min(2, max_size)
        # autocommit=True:讓 with conn.transaction() block 能自己 BEGIN ... COMMIT。
        # 若 autocommit=False,第一個 query 隱式開 outer transaction,後續
        # with conn.transaction() 只開 SAVEPOINT,process 結束 close 整段 rollback。
        # ref: https://www.psycopg.org/psycopg3/docs/basic/transactions.html
        self.pool: ConnectionPool = ConnectionPool(
            conninfo=connection_url,
            min_size=min_size,
            max_size=max_size,
            kwargs={"autocommit": True, "row_factory": dict_row},
            open=True,
        )

        # 快取:table -> {col_name: col_type};instance level 共享,thread-safe
        # (asyncio 單執行緒 + GIL,multi-conn 不會踩 dict mutation race)
        self._col_type_cache: dict[str, dict[str, str]] = {}
        self._pk_cache: dict[str, list[str]] = {}
        self._warned_drops: set[tuple[str, frozenset[str]]] = set()

        logger.info(
            f"PostgresWriter connected to {self._mask_url(connection_url)} "
            f"(pool min={min_size} max={max_size})"
        )

    @staticmethod
    def _mask_url(url: str) -> str:
        """將連線字串中的密碼遮蔽,給 log 用。"""
        if "@" not in url:
            return url
        scheme_user, host_part = url.split("@", 1)
        if ":" in scheme_user:
            scheme_user = scheme_user.rsplit(":", 1)[0] + ":***"
        return f"{scheme_user}@{host_part}"

    # -------------------------------------------------------------------------
    # 寫入
    # -------------------------------------------------------------------------

    def upsert(
        self,
        table: str,
        rows: list[dict[str, Any]],
        primary_keys: list[str],
    ) -> int:
        """
        Postgres INSERT ... ON CONFLICT DO UPDATE 批次 UPSERT。

        - rows 中不存在於 schema 的欄位會被略過,並 log warning 一次
        - dict / list 自動轉 JSONB
        - datetime / date 自動處理
        - Decimal 對應 NUMERIC
        """
        if not rows:
            return 0
        if not primary_keys:
            raise ValueError(f"upsert() requires primary_keys for ON CONFLICT clause (table={table})")

        col_types = self._table_column_types(table)
        valid_cols = set(col_types.keys())

        first_row_keys = list(rows[0].keys())
        columns = [c for c in first_row_keys if c in valid_cols]
        if not columns:
            logger.warning(f"upsert -> {table}: 所有欄位都不在 schema 中,略過")
            return 0

        dropped = set(first_row_keys) - set(columns)
        if dropped:
            self._warn_dropped_once(table, dropped)

        col_str = ", ".join(f'"{c}"' for c in columns)
        placeholders = ", ".join(["%s"] * len(columns))
        pk_str = ", ".join(f'"{k}"' for k in primary_keys)
        update_cols = [c for c in columns if c not in primary_keys]

        if update_cols:
            # updated_at 特殊處理:
            #   1. row dict 有 updated_at → UPDATE 改 NOW()(不照 EXCLUDED 寫)
            #   2. row dict 沒 updated_at 但 schema 有 → 補 updated_at = NOW()
            update_pairs = [
                f'"{c}" = NOW()' if c == "updated_at" else f'"{c}" = EXCLUDED."{c}"'
                for c in update_cols
            ]
            if "updated_at" in valid_cols and "updated_at" not in columns:
                update_pairs.append('"updated_at" = NOW()')
            update_clause = ", ".join(update_pairs)
            sql = (
                f'INSERT INTO "{table}" ({col_str}) VALUES ({placeholders}) '
                f"ON CONFLICT ({pk_str}) DO UPDATE SET {update_clause}"
            )
        else:
            sql = (
                f'INSERT INTO "{table}" ({col_str}) VALUES ({placeholders}) '
                f"ON CONFLICT ({pk_str}) DO NOTHING"
            )

        values = [
            tuple(self._cast_for_pg(row.get(c), col_types[c]) for c in columns)
            for row in rows
        ]

        try:
            with self.pool.connection() as conn:
                with conn.transaction():
                    with conn.cursor() as cur:
                        cur.executemany(sql, values)
                        rowcount = cur.rowcount
        except Exception as e:
            logger.error(f"upsert -> {table} failed: {e}")
            raise

        logger.debug(f"upsert -> {table}: {rowcount} rows (pk={primary_keys})")
        return rowcount

    def insert(self, table: str, row: dict[str, Any]) -> None:
        """單筆 UPSERT 便捷 wrapper(INSERT ... ON CONFLICT DO NOTHING)。"""
        if not row:
            return
        col_types = self._table_column_types(table)
        valid_cols = set(col_types.keys())

        columns = [c for c in row.keys() if c in valid_cols]
        if not columns:
            logger.warning(f"insert -> {table}: 所有欄位都不在 schema 中,略過")
            return

        col_str = ", ".join(f'"{c}"' for c in columns)
        placeholders = ", ".join(["%s"] * len(columns))
        sql = (
            f'INSERT INTO "{table}" ({col_str}) VALUES ({placeholders}) '
            f"ON CONFLICT DO NOTHING"
        )
        values = tuple(self._cast_for_pg(row.get(c), col_types[c]) for c in columns)

        with self.pool.connection() as conn:
            with conn.transaction():
                with conn.cursor() as cur:
                    cur.execute(sql, values)

    def update(
        self,
        sql: str,
        params: list[Any] | None = None,
    ) -> int:
        """
        執行任意 UPDATE / DELETE 語句。

        SQL 需用 %s 參數佔位符(psycopg 慣例)。
        """
        with self.pool.connection() as conn:
            with conn.transaction():
                with conn.cursor() as cur:
                    cur.execute(sql, params or [])
                    return cur.rowcount

    # -------------------------------------------------------------------------
    # 讀取
    # -------------------------------------------------------------------------

    def query(
        self,
        sql: str,
        params: list[Any] | None = None,
    ) -> list[dict[str, Any]]:
        """SELECT 全表。Row factory 是 dict_row。"""
        with self.pool.connection() as conn:
            with conn.cursor() as cur:
                cur.execute(sql, params or [])
                return cur.fetchall()

    def query_one(
        self,
        sql: str,
        params: list[Any] | None = None,
    ) -> dict[str, Any] | None:
        with self.pool.connection() as conn:
            with conn.cursor() as cur:
                cur.execute(sql, params or [])
                return cur.fetchone()

    # -------------------------------------------------------------------------
    # Schema
    # -------------------------------------------------------------------------

    def init_schema(self) -> None:
        """
        v2.0:不再內嵌 DDL,改走 Alembic。

        執行邏輯:
        1. 檢查 schema_metadata 是否存在 + version 是否吻合
        2. 不吻合或不存在 → 觸發 `alembic upgrade head`(子進程)
        3. 吻合 → no-op,僅 log
        """
        UndefinedTable = self._psycopg.errors.UndefinedTable
        try:
            row = self.query_one(
                "SELECT value FROM schema_metadata WHERE key = %s",
                ["schema_version"],
            )
            if row and row.get("value") == SCHEMA_VERSION:
                logger.info(f"Schema 已是最新版本 (version={SCHEMA_VERSION}),略過初始化")
                return
        except UndefinedTable:
            logger.info("schema_metadata 不存在,執行初始化")

        if self._try_alembic_upgrade():
            return

        self._fallback_schema_pg_sql()

    def _try_alembic_upgrade(self) -> bool:
        """嘗試呼叫 alembic upgrade head,成功回傳 True。"""
        try:
            import subprocess
            project_root = Path(__file__).resolve().parent.parent
            alembic_ini = project_root / "alembic.ini"
            if not alembic_ini.exists():
                logger.debug("alembic.ini 不存在,跳過 Alembic 路徑")
                return False

            logger.info("執行 alembic upgrade head ...")
            result = subprocess.run(
                ["alembic", "upgrade", "head"],
                cwd=project_root,
                capture_output=True,
                text=True,
                timeout=60,
            )
            if result.returncode != 0:
                logger.error(
                    f"alembic upgrade head 失敗 (returncode={result.returncode}): "
                    f"{result.stderr}"
                )
                return False
            logger.info(f"alembic upgrade head 完成: {result.stdout.strip()}")
            return True
        except FileNotFoundError:
            logger.warning("alembic 命令不存在,fallback 到 schema_pg.sql")
            return False
        except Exception as e:
            logger.error(f"alembic 執行錯誤: {e}")
            return False

    def _fallback_schema_pg_sql(self) -> None:
        """讀取 src/schema_pg.sql 一次性執行(僅作 fallback)。"""
        schema_path = Path(__file__).resolve().parent / "schema_pg.sql"
        if not schema_path.exists():
            raise RuntimeError(
                f"schema_pg.sql 不存在於 {schema_path},無法初始化 schema"
            )
        sql = schema_path.read_text(encoding="utf-8")
        with self.pool.connection() as conn:
            with conn.transaction():
                with conn.cursor() as cur:
                    cur.execute(sql)
        logger.info(f"Schema 透過 schema_pg.sql 初始化完成 (version={SCHEMA_VERSION})")
        self._col_type_cache.clear()

    # -------------------------------------------------------------------------
    # Table 欄位 introspection
    # -------------------------------------------------------------------------

    def _table_column_types(self, table: str) -> dict[str, str]:
        """回傳 {col_name: col_type_lower}。information_schema.columns 查表,快取。"""
        if table not in self._col_type_cache:
            rows = self.query(
                """
                SELECT column_name, data_type
                  FROM information_schema.columns
                 WHERE table_schema = 'public'
                   AND table_name = %s
                 ORDER BY ordinal_position
                """,
                [table],
            )
            self._col_type_cache[table] = {
                r["column_name"]: r["data_type"].lower() for r in rows
            }
            if not self._col_type_cache[table]:
                logger.warning(f"_table_column_types: 表 {table} 不存在或無欄位")
        return self._col_type_cache[table]

    def _table_columns(self, table: str) -> set[str]:
        """外部 API:回傳欄位名稱 set。給 field_mapper / upsert 過濾用。"""
        return set(self._table_column_types(table).keys())

    def _table_pks(self, table: str) -> list[str]:
        """回傳 PRIMARY KEY 欄位名稱列表(依 ordinal_position 排序)。"""
        if table not in self._pk_cache:
            rows = self.query(
                """
                SELECT a.attname AS column_name
                  FROM pg_index i
                  JOIN pg_attribute a
                    ON a.attrelid = i.indrelid AND a.attnum = ANY(i.indkey)
                 WHERE i.indrelid = %s::regclass AND i.indisprimary
                 ORDER BY array_position(i.indkey, a.attnum)
                """,
                [table],
            )
            pks = [r["column_name"] for r in rows]
            if not pks:
                raise RuntimeError(
                    f"_table_pks: 表 {table} 找不到 PRIMARY KEY,"
                    f"可能是表不存在或 schema 未初始化"
                )
            self._pk_cache[table] = pks
        return self._pk_cache[table]

    def _invalidate_cache(self, table: str | None = None) -> None:
        """schema migration 後清快取。table=None 全清。"""
        if table is None:
            self._col_type_cache.clear()
            self._pk_cache.clear()
        else:
            self._col_type_cache.pop(table, None)
            self._pk_cache.pop(table, None)

    # -------------------------------------------------------------------------
    # 型別 cast
    # -------------------------------------------------------------------------

    @staticmethod
    def _cast_for_pg(value: Any, pg_type: str) -> Any:
        """把 Python value 轉成 psycopg 能正確處理的型態。"""
        if value is None:
            return None

        if pg_type == "jsonb":
            if isinstance(value, (dict, list)):
                from psycopg.types.json import Jsonb
                return Jsonb(value)
            if isinstance(value, str):
                try:
                    parsed = json.loads(value)
                    from psycopg.types.json import Jsonb
                    return Jsonb(parsed)
                except (ValueError, TypeError):
                    return value
            return value

        if pg_type == "date":
            if isinstance(value, str):
                if not value:
                    return None
                return value.split(" ")[0].split("T")[0]
            return value

        if pg_type in ("integer", "bigint", "smallint", "numeric", "real", "double precision"):
            if isinstance(value, str) and value.strip() == "":
                return None

        return value

    # -------------------------------------------------------------------------
    # Schema 變動容錯
    # -------------------------------------------------------------------------

    def _warn_dropped_once(self, table: str, dropped: set[str]) -> None:
        """同一 (table, dropped_keys) 只 warn 一次。"""
        key = (table, frozenset(dropped))
        if key in self._warned_drops:
            return
        self._warned_drops.add(key)
        logger.warning(
            f"upsert -> {table}: 略過不存在的欄位 {dropped}(API 欄位與 DB schema 不符)"
        )

    # -------------------------------------------------------------------------
    # 連線管理
    # -------------------------------------------------------------------------

    def close(self) -> None:
        """關閉 connection pool。"""
        if self.pool and not self.pool.closed:
            self.pool.close()
            logger.debug("PostgresWriter pool 關閉")

    def __enter__(self):
        return self

    def __exit__(self, exc_type, exc_val, exc_tb):
        self.close()
        return False


# =============================================================================
# Factory
# =============================================================================

def create_writer(connection_url: str | None = None) -> DBWriter:
    """
    DBWriter Factory(v3.3 PG-only)。

    優先序:
    1. 顯式參數 connection_url → PostgresWriter
    2. 環境變數 DATABASE_URL → PostgresWriter
    3. 都沒有 → 拋錯

    v3.3 移除 SQLite fallback(`TWSTOCK_USE_SQLITE=1` 不再有效);
    緊急情況可從 git history 還原。

    Args:
        connection_url: Postgres 連線字串。None 時走環境變數。

    Returns:
        PostgresWriter 實例

    Raises:
        RuntimeError: 環境變數缺失且未提供 connection_url
    """
    # 鏡像 alembic/env.py:載 .env 檔(若存在),讓 verify_*.py / main.py 等
    # 入口不必各自手動 load_dotenv。load_dotenv 預設不覆蓋已存在的環境變數。
    try:
        from dotenv import load_dotenv

        env_path = Path(__file__).resolve().parent.parent / ".env"
        if env_path.exists():
            load_dotenv(env_path)
    except ImportError:
        pass

    url = connection_url or os.getenv("DATABASE_URL")
    if not url:
        raise RuntimeError(
            "DATABASE_URL 未設定。請執行以下任一:\n"
            "  1. export DATABASE_URL=postgresql://twstock:twstock@localhost:5432/twstock\n"
            "  2. 在 .env 檔設定 DATABASE_URL(配合 python-dotenv)"
        )
    return PostgresWriter(url)
