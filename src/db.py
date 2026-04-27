"""
db.py
------
SQLite 連線管理與 UPSERT 工具模組。

設計重點：
- 啟用 WAL 模式：寫入不阻塞讀取，長時間回補更穩定
- busy_timeout=5000：等待鎖最多 5 秒，避免立即拋出 OperationalError
- foreign_keys=ON：確保資料完整性
- 提供通用 upsert()（INSERT OR REPLACE）供各模組使用
- Schema 初始化：首次執行時建立所有資料表
"""

import logging
import sqlite3
from pathlib import Path
from typing import Any

logger = logging.getLogger("collector.db")

# Schema 版本，與 Rust binary 的 schema_version 對應
SCHEMA_VERSION = "1.1"


class DBWriter:
    """
    SQLite 連線管理與寫入工具。

    使用方式：
        db = DBWriter("data/tw_stock.db")
        db.init_schema()
        db.upsert("stock_info", rows, ["market", "stock_id"])
        db.close()
    """

    def __init__(self, db_path: str):
        """
        初始化 SQLite 連線並套用必要 PRAGMA。

        Args:
            db_path: SQLite 資料庫檔案路徑
        """
        path = Path(db_path)
        path.parent.mkdir(parents=True, exist_ok=True)

        self.conn = sqlite3.connect(str(path))
        self.conn.row_factory = sqlite3.Row  # 查詢結果可用欄位名存取

        # WAL 模式：讀寫不互相阻塞，適合長時間回補情境
        self.conn.execute("PRAGMA journal_mode=WAL;")
        # 等待鎖最多 5 秒，避免立即噴 OperationalError: database is locked
        self.conn.execute("PRAGMA busy_timeout=5000;")
        # 啟用外鍵約束
        self.conn.execute("PRAGMA foreign_keys=ON;")

        logger.debug(f"DB 連線建立：{db_path}")

    # =========================================================================
    # 寫入工具
    # =========================================================================

    def upsert(
        self,
        table: str,
        rows: list[dict[str, Any]],
        primary_keys: list[str] | None = None,
    ) -> int:
        """
        以 INSERT OR REPLACE 執行批次 UPSERT。

        寫入前會比對資料表實際欄位（PRAGMA table_info），
        自動略過不存在的欄位，防止 API 新增欄位時炸掉。
        欄位清單有快取，同一 table 只查一次。

        Args:
            table:        目標資料表名稱
            rows:         要寫入的資料列（dict list）
            primary_keys: 衝突檢測用的 PK 欄位（僅供日誌記錄，SQLite 依表定義處理）

        Returns:
            實際影響的列數
        """
        if not rows:
            return 0

        # 取得資料表有效欄位（有快取，同表只查一次 PRAGMA）
        valid_cols = self._table_columns(table)

        # 過濾 rows，只保留 DB 中實際存在的欄位
        # 被略過的欄位已在 field_mapper._validate_schema 記錄 WARNING
        columns = [c for c in rows[0].keys() if c in valid_cols]
        if not columns:
            logger.warning(f"upsert → {table}: 所有欄位都不在 schema 中，略過")
            return 0

        dropped = set(rows[0].keys()) - set(columns)
        if dropped:
            logger.warning(
                f"upsert → {table}: 略過不存在的欄位 {dropped}（API 欄位與 DB schema 不符）"
            )

        placeholders = ", ".join(["?"] * len(columns))
        col_str      = ", ".join(columns)

        sql    = f"INSERT OR REPLACE INTO {table} ({col_str}) VALUES ({placeholders})"
        values = [tuple(row.get(c) for c in columns) for row in rows]

        with self.conn:
            cursor = self.conn.executemany(sql, values)

        logger.debug(f"upsert → {table}: {cursor.rowcount} rows (pk={primary_keys})")
        return cursor.rowcount

    def _table_columns(self, table: str) -> set[str]:
        """
        回傳指定資料表的有效欄位名稱集合（結果有快取）。

        Args:
            table: 資料表名稱

        Returns:
            欄位名稱 set
        """
        if not hasattr(self, "_col_cache"):
            self._col_cache: dict[str, set[str]] = {}

        if table not in self._col_cache:
            rows = self.query(f"PRAGMA table_info({table})")
            self._col_cache[table] = {row["name"] for row in rows}

        return self._col_cache[table]

    def insert(self, table: str, row: dict[str, Any]) -> None:
        """
        插入單筆資料（INSERT OR REPLACE）。

        Args:
            table: 目標資料表
            row:   單筆資料 dict
        """
        self.upsert(table, [row])

    def update(self, sql: str, params: list[Any] | None = None) -> int:
        """
        執行任意 UPDATE 語句。

        Args:
            sql:    UPDATE SQL 字串（使用 ? 作為參數佔位符）
            params: 對應的參數列表

        Returns:
            影響的列數
        """
        with self.conn:
            cursor = self.conn.execute(sql, params or [])
        return cursor.rowcount

    def query(self, sql: str, params: list[Any] | None = None) -> list[sqlite3.Row]:
        """
        執行 SELECT 查詢並回傳所有結果。

        Args:
            sql:    SELECT SQL 字串
            params: 對應的參數列表

        Returns:
            sqlite3.Row 物件列表（可用欄位名索引）
        """
        cursor = self.conn.execute(sql, params or [])
        return cursor.fetchall()

    def query_one(self, sql: str, params: list[Any] | None = None) -> sqlite3.Row | None:
        """查詢並回傳第一筆結果，無結果時回傳 None"""
        cursor = self.conn.execute(sql, params or [])
        return cursor.fetchone()

    # =========================================================================
    # Schema 初始化
    # =========================================================================

    def init_schema(self) -> None:
        """
        建立所有資料表（若不存在）。
        Schema 定義來自 tw_stock_architecture_review_v1.1。
        """
        ddl_statements = _get_schema_ddl()
        with self.conn:
            for ddl in ddl_statements:
                self.conn.execute(ddl)
        logger.info(f"Schema 初始化完成（version={SCHEMA_VERSION}）")

    # =========================================================================
    # 連線管理
    # =========================================================================

    def close(self) -> None:
        """關閉資料庫連線"""
        if self.conn:
            self.conn.close()
            logger.debug("DB 連線關閉")

    def __enter__(self):
        return self

    def __exit__(self, exc_type, exc_val, exc_tb):
        self.close()
        return False


# =============================================================================
# Schema DDL
# =============================================================================

def _get_schema_ddl() -> list[str]:
    """
    回傳所有 CREATE TABLE 語句。
    使用 IF NOT EXISTS，可安全重複執行。
    """
    return [
        # 股票基本資料
        """
        CREATE TABLE IF NOT EXISTS stock_info (
            market          TEXT NOT NULL,
            stock_id        TEXT NOT NULL,
            stock_name      TEXT,
            market_type     TEXT,          -- twse | otc | emerging
            industry        TEXT,
            listing_date    TEXT,
            delist_date     TEXT,
            par_value       REAL,
            detail          TEXT,          -- JSON，儲存額外欄位
            source          TEXT DEFAULT 'finmind',
            updated_at      TEXT DEFAULT (datetime('now')),
            PRIMARY KEY (market, stock_id)
        )
        """,

        # 交易日曆
        """
        CREATE TABLE IF NOT EXISTS trading_calendar (
            market  TEXT NOT NULL,
            date    TEXT NOT NULL,
            source  TEXT DEFAULT 'finmind',
            PRIMARY KEY (market, date)
        )
        """,

        # 台股加權報酬指數
        """
        CREATE TABLE IF NOT EXISTS market_index_tw (
            market    TEXT NOT NULL,
            stock_id  TEXT NOT NULL,   -- 指數代碼：TAIEX（加權報酬指數）/ TPEx（櫃買報酬指數）
            date      TEXT NOT NULL,
            price     REAL,
            source    TEXT DEFAULT 'finmind',
            PRIMARY KEY (market, stock_id, date)
        )
        """,

        # 價格調整事件（除權息、減資、分割、面額變更、現增）
        """
        CREATE TABLE IF NOT EXISTS price_adjustment_events (
            market             TEXT    NOT NULL,
            stock_id           TEXT    NOT NULL,
            date               TEXT    NOT NULL,
            event_type         TEXT    NOT NULL,  -- dividend | capital_reduction | split | par_value_change | capital_increase
            before_price       REAL,
            after_price        REAL,
            reference_price    REAL,
            adjustment_factor  REAL    DEFAULT 1.0,
            volume_factor      REAL    DEFAULT 1.0,
            cash_dividend      REAL,
            stock_dividend     REAL,
            detail             TEXT,              -- JSON，儲存額外欄位
            source             TEXT    DEFAULT 'finmind',
            PRIMARY KEY (market, stock_id, date, event_type)
        )
        """,

        # 股利政策暫存表（Phase 2 → post_process 後不保留）
        """
        CREATE TABLE IF NOT EXISTS _dividend_policy_staging (
            market      TEXT NOT NULL,
            stock_id    TEXT NOT NULL,
            date        TEXT NOT NULL,
            detail      TEXT,
            PRIMARY KEY (market, stock_id, date)
        )
        """,

        # 日K 原始價格
        """
        CREATE TABLE IF NOT EXISTS price_daily (
            market      TEXT    NOT NULL,
            stock_id    TEXT    NOT NULL,
            date        TEXT    NOT NULL,
            open        REAL,
            high        REAL,
            low         REAL,
            close       REAL,
            volume      INTEGER,
            turnover    REAL,
            source      TEXT    DEFAULT 'finmind',
            PRIMARY KEY (market, stock_id, date)
        )
        """,

        # 漲跌停價格
        """
        CREATE TABLE IF NOT EXISTS price_limit (
            market      TEXT    NOT NULL,
            stock_id    TEXT    NOT NULL,
            date        TEXT    NOT NULL,
            limit_up    REAL,
            limit_down  REAL,
            source      TEXT    DEFAULT 'finmind',
            PRIMARY KEY (market, stock_id, date)
        )
        """,

        # 後復權日K（Rust Phase 4 計算產出）
        """
        CREATE TABLE IF NOT EXISTS price_daily_fwd (
            market      TEXT    NOT NULL,
            stock_id    TEXT    NOT NULL,
            date        TEXT    NOT NULL,
            open        REAL,
            high        REAL,
            low         REAL,
            close       REAL,
            volume      INTEGER,
            PRIMARY KEY (market, stock_id, date)
        )
        """,

        # 後復權週K（Rust Phase 4 聚合）
        """
        CREATE TABLE IF NOT EXISTS price_weekly_fwd (
            market      TEXT    NOT NULL,
            stock_id    TEXT    NOT NULL,
            year        INTEGER NOT NULL,
            week        INTEGER NOT NULL,
            open        REAL,
            high        REAL,
            low         REAL,
            close       REAL,
            volume      INTEGER,
            PRIMARY KEY (market, stock_id, year, week)
        )
        """,

        # 後復權月K（Rust Phase 4 聚合）
        """
        CREATE TABLE IF NOT EXISTS price_monthly_fwd (
            market      TEXT    NOT NULL,
            stock_id    TEXT    NOT NULL,
            year        INTEGER NOT NULL,
            month       INTEGER NOT NULL,
            open        REAL,
            high        REAL,
            low         REAL,
            close       REAL,
            volume      INTEGER,
            PRIMARY KEY (market, stock_id, year, month)
        )
        """,

        # 三大法人買賣超（5 類：外資、外資自營商、投信、自營商自行、自營商避險）
        """
        CREATE TABLE IF NOT EXISTS institutional_daily (
            market                    TEXT    NOT NULL,
            stock_id                  TEXT    NOT NULL,
            date                      TEXT    NOT NULL,
            foreign_buy               INTEGER,
            foreign_sell              INTEGER,
            foreign_dealer_self_buy   INTEGER,
            foreign_dealer_self_sell  INTEGER,
            investment_trust_buy      INTEGER,
            investment_trust_sell     INTEGER,
            dealer_buy                INTEGER,
            dealer_sell               INTEGER,
            dealer_hedging_buy        INTEGER,
            dealer_hedging_sell       INTEGER,
            source                    TEXT    DEFAULT 'finmind',
            PRIMARY KEY (market, stock_id, date)
        )
        """,

        # 融資融券
        """
        CREATE TABLE IF NOT EXISTS margin_daily (
            market              TEXT    NOT NULL,
            stock_id            TEXT    NOT NULL,
            date                TEXT    NOT NULL,
            margin_purchase     INTEGER,
            margin_sell         INTEGER,
            margin_balance      INTEGER,
            short_sale          INTEGER,
            short_cover         INTEGER,
            short_balance       INTEGER,
            source              TEXT    DEFAULT 'finmind',
            PRIMARY KEY (market, stock_id, date)
        )
        """,

        # 外資持股
        """
        CREATE TABLE IF NOT EXISTS foreign_holding (
            market              TEXT    NOT NULL,
            stock_id            TEXT    NOT NULL,
            date                TEXT    NOT NULL,
            foreign_holding_shares  INTEGER,
            foreign_holding_ratio   REAL,
            source              TEXT    DEFAULT 'finmind',
            PRIMARY KEY (market, stock_id, date)
        )
        """,

        # 股權分散表
        """
        CREATE TABLE IF NOT EXISTS holding_shares_per (
            market      TEXT    NOT NULL,
            stock_id    TEXT    NOT NULL,
            date        TEXT    NOT NULL,
            detail      TEXT,              -- JSON，儲存各級距持股人數與張數
            source      TEXT    DEFAULT 'finmind',
            PRIMARY KEY (market, stock_id, date)
        )
        """,

        # 本益比 / 殖利率 / 淨值比
        """
        CREATE TABLE IF NOT EXISTS valuation_daily (
            market      TEXT    NOT NULL,
            stock_id    TEXT    NOT NULL,
            date        TEXT    NOT NULL,
            per         REAL,
            dividend_yield REAL,
            pbr         REAL,
            source      TEXT    DEFAULT 'finmind',
            PRIMARY KEY (market, stock_id, date)
        )
        """,

        # 當沖資訊
        """
        CREATE TABLE IF NOT EXISTS day_trading (
            market          TEXT    NOT NULL,
            stock_id        TEXT    NOT NULL,
            date            TEXT    NOT NULL,
            day_trading_buy  INTEGER,
            day_trading_sell INTEGER,
            source          TEXT    DEFAULT 'finmind',
            PRIMARY KEY (market, stock_id, date)
        )
        """,

        # 指數成分權重
        """
        CREATE TABLE IF NOT EXISTS index_weight_daily (
            market      TEXT    NOT NULL,
            stock_id    TEXT    NOT NULL,
            date        TEXT    NOT NULL,
            weight      REAL,
            source      TEXT    DEFAULT 'finmind',
            PRIMARY KEY (market, stock_id, date)
        )
        """,

        # 月營收
        """
        CREATE TABLE IF NOT EXISTS monthly_revenue (
            market      TEXT    NOT NULL,
            stock_id    TEXT    NOT NULL,
            date        TEXT    NOT NULL,
            revenue     REAL,
            revenue_mom REAL,
            revenue_yoy REAL,
            source      TEXT    DEFAULT 'finmind',
            PRIMARY KEY (market, stock_id, date)
        )
        """,

        # 財務報表（損益表、資產負債表、現金流量表共用一張表）
        """
        CREATE TABLE IF NOT EXISTS financial_statement (
            market      TEXT    NOT NULL,
            stock_id    TEXT    NOT NULL,
            date        TEXT    NOT NULL,
            type        TEXT    NOT NULL,  -- 報表類型（income | balance | cashflow）
            detail      TEXT,              -- JSON，儲存各科目值
            source      TEXT    DEFAULT 'finmind',
            PRIMARY KEY (market, stock_id, date, type)
        )
        """,

        # 美股指數（SPY, VIX）
        """
        CREATE TABLE IF NOT EXISTS market_index_us (
            market      TEXT    NOT NULL,
            stock_id    TEXT    NOT NULL,
            date        TEXT    NOT NULL,
            open        REAL,
            high        REAL,
            low         REAL,
            close       REAL,
            volume      INTEGER,
            source      TEXT    DEFAULT 'finmind',
            PRIMARY KEY (market, stock_id, date)
        )
        """,

        # 匯率（每日多幣別，PK 含 currency）
        """
        CREATE TABLE IF NOT EXISTS exchange_rate (
            market      TEXT    NOT NULL,
            date        TEXT    NOT NULL,
            currency    TEXT    NOT NULL,  -- 幣別代碼，如 USD、EUR
            rate        REAL,              -- spot_buy（即期買匯）
            detail      TEXT,              -- JSON，儲存 cash_buy / cash_sell / spot_sell
            source      TEXT    DEFAULT 'finmind',
            PRIMARY KEY (market, date, currency)
        )
        """,

        # 全市場三大法人
        """
        CREATE TABLE IF NOT EXISTS institutional_market_daily (
            market                    TEXT    NOT NULL,
            date                      TEXT    NOT NULL,
            foreign_buy               INTEGER,
            foreign_sell              INTEGER,
            foreign_dealer_self_buy   INTEGER,
            foreign_dealer_self_sell  INTEGER,
            investment_trust_buy      INTEGER,
            investment_trust_sell     INTEGER,
            dealer_buy                INTEGER,
            dealer_sell               INTEGER,
            dealer_hedging_buy        INTEGER,
            dealer_hedging_sell       INTEGER,
            source                    TEXT    DEFAULT 'finmind',
            PRIMARY KEY (market, date)
        )
        """,

        # 整體市場融資維持率
        """
        CREATE TABLE IF NOT EXISTS market_margin_maintenance (
            market      TEXT    NOT NULL,
            date        TEXT    NOT NULL,
            ratio       REAL,
            source      TEXT    DEFAULT 'finmind',
            PRIMARY KEY (market, date)
        )
        """,

        # CNN 恐懼貪婪指數
        """
        CREATE TABLE IF NOT EXISTS fear_greed_index (
            market      TEXT    NOT NULL,
            date        TEXT    NOT NULL,
            score       REAL,
            label       TEXT,
            detail      TEXT,              -- JSON，儲存任何額外欄位（欄位視 API 版本而定）
            source      TEXT    DEFAULT 'finmind',
            PRIMARY KEY (market, date)
        )
        """,

        # 同步狀態追蹤（per-stock 級別）
        """
        CREATE TABLE IF NOT EXISTS stock_sync_status (
            market              TEXT    NOT NULL,
            stock_id            TEXT    NOT NULL,
            last_full_sync      TEXT,              -- 最後一次完整同步日期
            last_incr_sync      TEXT,              -- 最後一次增量同步日期
            fwd_adj_valid       INTEGER DEFAULT 0, -- Rust 後復權是否有效（0/1）
            PRIMARY KEY (market, stock_id)
        )
        """,

        # API 層級斷點續傳進度表
        """
        CREATE TABLE IF NOT EXISTS api_sync_progress (
            api_name        TEXT    NOT NULL,
            stock_id        TEXT    NOT NULL,
            segment_start   TEXT    NOT NULL,
            segment_end     TEXT    NOT NULL,
            status          TEXT    NOT NULL DEFAULT 'pending',
            record_count    INTEGER DEFAULT 0,
            error_message   TEXT,
            updated_at      TEXT    NOT NULL DEFAULT (datetime('now')),
            PRIMARY KEY (api_name, stock_id, segment_start)
        )
        """,
    ]
