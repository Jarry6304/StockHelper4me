"""
db.py
------
DB 寫入抽象與實作層。

設計原則(v2.0,Postgres 為主、SQLite 為過渡 fallback):

1. 抽象層採 typing.Protocol(structural typing)
   - 不強制繼承,任何符合 method signature 的 class 都是合格 DBWriter
   - 比 ABC 更彈性,允許兩個實作的內部行為差異(例如 advisory lock)

2. PostgresWriter(預設、production)
   - psycopg3 + connection pool + dict_row factory
   - INSERT ... ON CONFLICT DO UPDATE(取代 SQLite 的 INSERT OR REPLACE)
   - JSONB 欄位由 psycopg 自動處理(dict <-> jsonb)
   - DATE / TIMESTAMPTZ 欄位由 psycopg 自動處理(date / datetime)
   - 寫入時依 _column_types 自動 cast,業務 code 不用煩惱型別

3. SqliteWriter(過渡、debug only)
   - 啟用條件:環境變數 TWSTOCK_USE_SQLITE=1
   - 維護紀律:只實作 Protocol 必要 method,不追新功能
   - v2.1 開發完成後評估是否完全廢棄

4. Schema 初始化
   - 走 Alembic migration(`alembic upgrade head`),不再內嵌 DDL 字串
   - init_schema() 改為觸發 alembic 遷移(或檢查 schema_metadata)

5. _table_columns / _table_column_types 快取
   - _table_columns(table) -> set[str]:外部 API,給 field_mapper / upsert 用
   - _table_column_types(table) -> dict[str, str]:內部用,upsert 自動 cast 依據

連線字串(DATABASE_URL):
    postgresql://twstock:twstock@localhost:5432/twstock

隱藏 SQLite 模式:
    TWSTOCK_USE_SQLITE=1 SQLITE_PATH=data/tw_stock.db python src/main.py ...
"""

from __future__ import annotations

import json
import logging
import os
import sqlite3
from datetime import date, datetime
from decimal import Decimal
from pathlib import Path
from typing import Any, Protocol, runtime_checkable

logger = logging.getLogger("collector.db")

# Schema 版本,與 schema_metadata 表中的值對齊
# Rust binary 啟動時 assert 此值
SCHEMA_VERSION = "2.0"


# =============================================================================
# 抽象介面(Protocol)
# =============================================================================

@runtime_checkable
class DBWriter(Protocol):
    """
    DB 寫入抽象。

    Structural typing — 任何具備這些 method 的物件都符合,不需 inherit。
    主要實作:PostgresWriter(production)、SqliteWriter(debug fallback)

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
        """
        批次 UPSERT。略過 schema 中不存在的欄位(API 欄位漂移容錯)。
        primary_keys 用於 ON CONFLICT 語句。
        回傳影響的列數。
        """
        ...

    def insert(self, table: str, row: dict[str, Any]) -> None:
        """單筆 UPSERT(便捷 wrapper)。"""
        ...

    def query(
        self,
        sql: str,
        params: list[Any] | None = None,
    ) -> list[dict[str, Any]]:
        """執行 SELECT,回傳 dict list。SQL 用 %s 參數佔位符。"""
        ...

    def query_one(
        self,
        sql: str,
        params: list[Any] | None = None,
    ) -> dict[str, Any] | None:
        """執行 SELECT,回傳第一筆 dict 或 None。"""
        ...

    def update(
        self,
        sql: str,
        params: list[Any] | None = None,
    ) -> int:
        """執行 UPDATE / DELETE,回傳影響列數。"""
        ...

    def init_schema(self) -> None:
        """初始化 schema(觸發 Alembic migration 或對等機制)。"""
        ...

    def _table_columns(self, table: str) -> set[str]:
        """
        回傳指定資料表的有效欄位名稱集合(快取)。
        給 upsert 過濾與 field_mapper schema validation 用。
        """
        ...

    def _table_pks(self, table: str) -> list[str]:
        """
        回傳指定資料表的 PRIMARY KEY 欄位名稱列表(依 ordinal_position 排序)。
        給 upsert 的 ON CONFLICT 子句用,避免上層硬編碼 PK 對照表。
        """
        ...

    def close(self) -> None:
        """關閉連線。"""
        ...

    def __enter__(self) -> "DBWriter": ...
    def __exit__(self, exc_type, exc_val, exc_tb) -> bool: ...


# =============================================================================
# PostgresWriter — 預設實作,psycopg3
# =============================================================================

class PostgresWriter:
    """
    psycopg3 同步 wrapper(以 dict_row 為預設 row factory)。

    Phase 排程是 serial 的(Phase 1 → 2 → ... → 6),Collector 階段不需要 async。
    若將來 Aggregation Layer / on-demand 補算需要 async,另外建 PostgresAsyncWriter
    或讓業務 code 直接用 psycopg.AsyncConnection,本 class 不混合 sync/async。

    Connection 模式:
    - 持有單一 Connection(autocommit=True)
    - 每個寫入 method 用 `with conn.transaction()` 建立顯式 BEGIN ... COMMIT 邊界
    - 讀取 method 走 implicit single-statement transaction(autocommit 預設行為)
    - close() 時釋放
    為何 autocommit=True:psycopg3 在 autocommit=False 下,第一個 query 會
    隱式開 outer transaction,後續 with conn.transaction() 只開 SAVEPOINT,
    process 結束 close 時整段 outer tx rollback → 寫入丟失。
    ref: https://www.psycopg.org/psycopg3/docs/basic/transactions.html
    """

    def __init__(self, connection_url: str):
        try:
            import psycopg
            from psycopg.rows import dict_row
        except ImportError as e:
            raise RuntimeError(
                "psycopg not installed. Run: pip install 'psycopg[binary,pool]>=3.2'"
            ) from e

        self._psycopg = psycopg
        self._dict_row = dict_row
        self.url = connection_url
        # autocommit=True，讓 transaction() block 能自己 BEGIN ... COMMIT。
        # 若設 False，第一個 query 會隱式開一個 outer transaction，
        # 之後 with conn.transaction() 只會在它裡面開 SAVEPOINT，
        # 帶 RELEASE 但不 COMMIT → close() 就隨 connection 一起 rollback。
        # 之前的「寫入看似成功但 process 結束後資料不見」就是踩這個坑。
        # ref: https://www.psycopg.org/psycopg3/docs/basic/transactions.html
        self.conn: psycopg.Connection = psycopg.connect(
            connection_url,
            autocommit=True,
            row_factory=dict_row,
        )
        # 快取:table -> {col_name: col_type}
        self._col_type_cache: dict[str, dict[str, str]] = {}
        logger.debug(f"PostgresWriter connected to {self._mask_url(connection_url)}")

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

        # 過濾欄位
        first_row_keys = list(rows[0].keys())
        columns = [c for c in first_row_keys if c in valid_cols]
        if not columns:
            logger.warning(f"upsert -> {table}: 所有欄位都不在 schema 中,略過")
            return 0

        dropped = set(first_row_keys) - set(columns)
        if dropped:
            # 只 warn 一次(同一 table + 同一組 dropped keys)
            self._warn_dropped_once(table, dropped)

        # 組 SQL
        # INSERT INTO t (col1, col2, ...) VALUES (%s, %s, ...)
        # ON CONFLICT (pk1, pk2) DO UPDATE SET col1 = EXCLUDED.col1, ...
        col_str = ", ".join(f'"{c}"' for c in columns)
        placeholders = ", ".join(["%s"] * len(columns))
        pk_str = ", ".join(f'"{k}"' for k in primary_keys)
        update_cols = [c for c in columns if c not in primary_keys]

        if update_cols:
            # updated_at 特殊處理(兩條路都對齊):
            #   1. row dict 有 updated_at → UPDATE 不照 EXCLUDED 寫,改 NOW()
            #      (api_sync_progress 走這條,row 帶 datetime.now() 跟 NOW() 等價)
            #   2. row dict 沒 updated_at 但 schema 有(stock_info 移到 detail 後)
            #      → 補一條 updated_at = NOW(),跟 _merge_delist_date 的
            #      SET updated_at = NOW() 語意一致,也跟 schema DEFAULT NOW() 對 INSERT
            #      的行為一致。否則 UPDATE 路徑會留下舊的「ISO date 轉 timestamp」殘值。
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
            # 全欄位都是 PK(罕見,例如 trading_calendar)
            sql = (
                f'INSERT INTO "{table}" ({col_str}) VALUES ({placeholders}) '
                f"ON CONFLICT ({pk_str}) DO NOTHING"
            )

        # 準備 values,依 col_type 做必要 cast
        values = [
            tuple(self._cast_for_pg(row.get(c), col_types[c]) for c in columns)
            for row in rows
        ]

        try:
            with self.conn.transaction():
                with self.conn.cursor() as cur:
                    cur.executemany(sql, values)
                    rowcount = cur.rowcount
        except Exception as e:
            logger.error(f"upsert -> {table} failed: {e}")
            raise

        logger.debug(f"upsert -> {table}: {rowcount} rows (pk={primary_keys})")
        return rowcount

    def insert(self, table: str, row: dict[str, Any]) -> None:
        """單筆 UPSERT 便捷 wrapper。需手動傳 primary_keys 才能 conflict 處理 →
        為了相容 SQLite 版的 insert(table, row) signature,這裡退回 INSERT ... ON CONFLICT DO NOTHING。"""
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

        with self.conn.transaction():
            with self.conn.cursor() as cur:
                cur.execute(sql, values)

    def update(
        self,
        sql: str,
        params: list[Any] | None = None,
    ) -> int:
        """
        執行任意 UPDATE / DELETE 語句。

        SQL 需用 %s 參數佔位符(psycopg 慣例)。
        相容性提示:從 SQLite 版遷移時,把 ? 改 %s。
        """
        with self.conn.transaction():
            with self.conn.cursor() as cur:
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
        """SELECT 全表。Row factory 是 dict_row,結果可用 dict[key] 存取。"""
        with self.conn.cursor() as cur:
            cur.execute(sql, params or [])
            return cur.fetchall()

    def query_one(
        self,
        sql: str,
        params: list[Any] | None = None,
    ) -> dict[str, Any] | None:
        with self.conn.cursor() as cur:
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

        若 alembic 不可用,fallback 到讀取 src/schema_pg.sql 一次性執行
        (供 CI 快速啟動或無 alembic 環境使用)。
        """
        # Step 1: 檢查 schema_metadata 是否存在 + 版本是否吻合
        # autocommit=True 下,query_one 失敗 connection 不會卡 abort 狀態,
        # 不需要顯式 rollback。只縮窄 except 到 UndefinedTable,避免把
        # 連線錯誤 / 權限錯誤誤判為「表不存在」往下走 alembic 路徑。
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

        # Step 2: 嘗試走 Alembic
        if self._try_alembic_upgrade():
            return

        # Step 3: Fallback 走 schema_pg.sql
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
        with self.conn.transaction():
            with self.conn.cursor() as cur:
                cur.execute(sql)
        logger.info(f"Schema 透過 schema_pg.sql 初始化完成 (version={SCHEMA_VERSION})")
        # 清快取(因為剛建表)
        self._col_type_cache.clear()

    # -------------------------------------------------------------------------
    # Table 欄位 introspection
    # -------------------------------------------------------------------------

    def _table_column_types(self, table: str) -> dict[str, str]:
        """
        回傳 {col_name: col_type_lower}。col_type 來自 information_schema.columns
        的 data_type 欄位(jsonb / text / integer / bigint / numeric / date / timestamp with time zone ...)。

        快取:同表只查一次。schema migration 後如需更新,呼叫 _invalidate_cache()。
        """
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
        """
        回傳指定資料表的 PRIMARY KEY 欄位名稱列表(依 ordinal_position 排序)。

        schema 是 single source of truth — 上層 phase_executor / sync_tracker
        不再硬編碼 PK 對照表,改呼叫此 API 動態查 PG。

        快取:同表只查一次。schema migration 改 PK 時呼叫 _invalidate_cache()。

        Raises:
            RuntimeError: 表不存在或沒有 PRIMARY KEY 定義。
        """
        if not hasattr(self, "_pk_cache"):
            self._pk_cache: dict[str, list[str]] = {}
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
            if hasattr(self, "_pk_cache"):
                self._pk_cache.clear()
        else:
            self._col_type_cache.pop(table, None)
            if hasattr(self, "_pk_cache"):
                self._pk_cache.pop(table, None)

    # -------------------------------------------------------------------------
    # 型別 cast(寫入時依欄位 type 自動處理)
    # -------------------------------------------------------------------------

    @staticmethod
    def _cast_for_pg(value: Any, pg_type: str) -> Any:
        """
        把 Python value 轉成 psycopg 能正確處理的型態。

        關鍵 cast:
        - dict / list → JSON string(psycopg3 用 Jsonb adapter,但這層 explicit cast 比較直觀)
        - "YYYY-MM-DD HH:MM:SS" → 截到 "YYYY-MM-DD"(若 pg_type 是 date)
        - 空字串 → None(date / numeric / int 欄位)
        - 其他保持原樣,讓 psycopg 自動處理
        """
        if value is None:
            return None

        # JSONB:dict/list → JSON 字串包裝為 Jsonb adapter
        if pg_type == "jsonb":
            if isinstance(value, (dict, list)):
                # psycopg3 has Jsonb wrapper; import lazily
                from psycopg.types.json import Jsonb
                return Jsonb(value)
            if isinstance(value, str):
                # 已經是 JSON 字串 → 嘗試 parse 後包 Jsonb(避免雙重 escape)
                try:
                    parsed = json.loads(value)
                    from psycopg.types.json import Jsonb
                    return Jsonb(parsed)
                except (ValueError, TypeError):
                    # 無法 parse,當純字串塞進去,Postgres 會視情況報錯(讓開發者看見)
                    return value
            return value

        # DATE:截字串 / 處理空字串
        if pg_type == "date":
            if isinstance(value, str):
                if not value:
                    return None
                # "YYYY-MM-DD" 或 "YYYY-MM-DD HH:MM:SS"
                return value.split(" ")[0].split("T")[0]
            return value

        # 數字欄位:空字串 → None
        if pg_type in ("integer", "bigint", "smallint", "numeric", "real", "double precision"):
            if isinstance(value, str) and value.strip() == "":
                return None

        return value

    # -------------------------------------------------------------------------
    # Schema 變動容錯 — 略過欄位的 warning 去重
    # -------------------------------------------------------------------------

    def _warn_dropped_once(self, table: str, dropped: set[str]) -> None:
        """同一 (table, dropped_keys) 只 warn 一次,避免每次 upsert 洗版。"""
        if not hasattr(self, "_warned_drops"):
            self._warned_drops: set[tuple[str, frozenset[str]]] = set()
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
        if self.conn and not self.conn.closed:
            self.conn.close()
            logger.debug("PostgresWriter 連線關閉")

    def __enter__(self):
        return self

    def __exit__(self, exc_type, exc_val, exc_tb):
        self.close()
        return False


# =============================================================================
# SqliteWriter — 過渡 / debug fallback,僅 TWSTOCK_USE_SQLITE=1 啟用
# =============================================================================

class SqliteWriter:
    """
    SQLite fallback — v2.0 起實質停用,只剩讀取舊 dump 的能力。

    停用原因:
    - init_schema() 依賴的 db_legacy_sqlite_ddl 模組已移除
    - update() 用 ? 佔位符,但 phase_executor / post_process / stock_resolver
      已全面改 PG 的 %s 佔位符,SQLite 模式跑這些 SQL 會語法錯
    - schema 演進(detail TEXT、5 類法人擴充等)不再回填 SQLite 端

    保留唯一目的:CI 環境無 Postgres 時用 query / query_one 讀舊 .db dump,
    純檢視用。寫入(upsert/insert/update)、init_schema 都會 raise。

    v2.1 規劃完全砍除整個 class。
    """

    def __init__(self, db_path: str):
        Path(db_path).parent.mkdir(parents=True, exist_ok=True)
        self.path = db_path
        self.conn = sqlite3.connect(db_path, isolation_level=None)
        self.conn.row_factory = sqlite3.Row
        self.conn.execute("PRAGMA journal_mode=WAL")
        self.conn.execute("PRAGMA busy_timeout=5000")
        self.conn.execute("PRAGMA foreign_keys=ON")
        self._col_cache: dict[str, set[str]] = {}
        logger.warning(
            f"SqliteWriter 啟用 (path={db_path}). "
            "v2.0 起此模式僅支援 query / query_one(讀取舊 dump);"
            "init_schema / upsert / insert / update 已停用,production 應走 PostgresWriter."
        )

    def upsert(
        self,
        table: str,
        rows: list[dict[str, Any]],
        primary_keys: list[str],
    ) -> int:
        raise NotImplementedError(
            "v2.0 起 SqliteWriter 寫入功能已停用(schema 不再同步維護)。"
            "請改用 PostgresWriter:export DATABASE_URL=postgresql://..."
        )

    def insert(self, table: str, row: dict[str, Any]) -> None:
        raise NotImplementedError(
            "v2.0 起 SqliteWriter 寫入功能已停用。請改用 PostgresWriter。"
        )

    def update(self, sql: str, params: list[Any] | None = None) -> int:
        raise NotImplementedError(
            "v2.0 起 SqliteWriter.update 已停用:呼叫端的 SQL 用 PG 的 %s 佔位符,"
            "與 SQLite 的 ? 不相容。請改用 PostgresWriter。"
        )

    def query(
        self,
        sql: str,
        params: list[Any] | None = None,
    ) -> list[dict[str, Any]]:
        cursor = self.conn.execute(sql, params or [])
        return [dict(r) for r in cursor.fetchall()]

    def query_one(
        self,
        sql: str,
        params: list[Any] | None = None,
    ) -> dict[str, Any] | None:
        cursor = self.conn.execute(sql, params or [])
        row = cursor.fetchone()
        return dict(row) if row else None

    def init_schema(self) -> None:
        """v2.0 起停用:db_legacy_sqlite_ddl 模組已不存在,且 schema 不再維護 SQLite 版。"""
        raise NotImplementedError(
            "v2.0 起 SqliteWriter.init_schema 已停用(legacy DDL 模組已移除)。"
            "請改用 PostgresWriter:export DATABASE_URL=postgresql://... && alembic upgrade head"
        )

    def _table_columns(self, table: str) -> set[str]:
        if table not in self._col_cache:
            rows = self.query(f"PRAGMA table_info({table})")
            self._col_cache[table] = {r["name"] for r in rows}
        return self._col_cache[table]

    def _table_pks(self, table: str) -> list[str]:
        raise NotImplementedError(
            "v2.0 起 SqliteWriter._table_pks 已停用(寫入功能也停用,讀取無此需求)。"
            "請改用 PostgresWriter。"
        )

    def close(self) -> None:
        if self.conn:
            self.conn.close()

    def __enter__(self):
        return self

    def __exit__(self, exc_type, exc_val, exc_tb):
        self.close()
        return False


# =============================================================================
# Factory — 依環境選擇實作
# =============================================================================

def create_writer(connection_url: str | None = None) -> DBWriter:
    """
    DBWriter Factory。

    優先序:
    1. 隱藏 flag TWSTOCK_USE_SQLITE=1 → SqliteWriter(僅供 debug / CI)
    2. 顯式參數 connection_url → PostgresWriter
    3. 環境變數 DATABASE_URL → PostgresWriter
    4. 都沒有 → 拋錯,提示如何設定

    Args:
        connection_url: Postgres 連線字串。None 時走環境變數。

    Returns:
        DBWriter 實作(PostgresWriter 或 SqliteWriter)

    Raises:
        RuntimeError: 環境變數缺失且未提供 connection_url
    """
    if os.getenv("TWSTOCK_USE_SQLITE") == "1":
        sqlite_path = os.getenv("SQLITE_PATH", "data/tw_stock.db")
        return SqliteWriter(sqlite_path)

    url = connection_url or os.getenv("DATABASE_URL")
    if not url:
        raise RuntimeError(
            "DATABASE_URL 未設定。請執行以下任一:\n"
            "  1. export DATABASE_URL=postgresql://twstock:twstock@localhost:5432/twstock\n"
            "  2. 在 .env 檔設定 DATABASE_URL(配合 python-dotenv)\n"
            "  3. 啟動 SQLite debug 模式:export TWSTOCK_USE_SQLITE=1"
        )
    return PostgresWriter(url)


# =============================================================================
# 向後相容 alias(讓現有 import DBWriter from db 仍 work)
# =============================================================================

# 舊版 main.py / phase_executor.py 用 `from db import DBWriter` 並 instantiate
# 為避免一次改太多檔,提供 alias:呼叫 DBWriter(path) 等同 create_writer
# 但此 alias 行為將在 v2.1 移除,請改用 create_writer()
def _legacy_dbwriter_constructor(*args, **kwargs):
    """
    DEPRECATED: 舊用法 DBWriter(db_path) 已改為 create_writer(connection_url)。
    此 shim 只接 db_path 字串(若副檔名 .db → SQLite,否則視為 Postgres URL)。
    """
    import warnings
    warnings.warn(
        "DBWriter(path) 構造方式已 deprecated。請改用 create_writer() + DATABASE_URL 環境變數。",
        DeprecationWarning,
        stacklevel=2,
    )
    if args and isinstance(args[0], str):
        path = args[0]
        if path.endswith(".db") or path.endswith(".sqlite"):
            os.environ["TWSTOCK_USE_SQLITE"] = "1"
            os.environ["SQLITE_PATH"] = path
            return create_writer()
        return create_writer(connection_url=path)
    return create_writer()
