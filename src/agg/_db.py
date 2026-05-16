"""Aggregation Layer PG connection helper.

對齊 src/db.py:create_writer() 的 .env 載入 + DATABASE_URL 處理。
本 layer 純讀,故直接 psycopg connection(不走 PostgresWriter wrapper 的 write methods)。
"""

from __future__ import annotations

import os
from pathlib import Path
from typing import Any


def get_connection(database_url: str | None = None):
    """回傳 psycopg.Connection(read-only,autocommit=True,row_factory=dict_row)。

    優先序:
    1. 顯式 database_url 參數
    2. 環境變數 DATABASE_URL
    3. 從 repo root .env 載入 DATABASE_URL

    Args:
        database_url: 可選的連線字串。

    Raises:
        RuntimeError: 無法解出 connection string
        ImportError: psycopg 未安裝
    """
    try:
        import psycopg
        from psycopg.rows import dict_row
    except ImportError as e:
        raise ImportError(
            "psycopg not installed. Run: pip install 'psycopg[binary]>=3.2'"
        ) from e

    # 對齊 src/db.py 載入 .env 行為
    try:
        from dotenv import load_dotenv

        env_path = Path(__file__).resolve().parent.parent.parent / ".env"
        if env_path.exists():
            load_dotenv(env_path)
    except ImportError:
        pass

    url = database_url or os.getenv("DATABASE_URL")
    if not url:
        raise RuntimeError(
            "DATABASE_URL 未設定。請執行以下任一:\n"
            "  1. export DATABASE_URL=postgresql://twstock:twstock@localhost:5432/twstock\n"
            "  2. 在 .env 檔設定 DATABASE_URL"
        )
    return psycopg.connect(url, row_factory=dict_row, autocommit=True)


def fetch_facts(
    conn,
    *,
    stock_ids: list[str],
    as_of,
    lookback_days: int,
    cores: list[str] | None = None,
) -> list[dict[str, Any]]:
    """從 facts 表撈 rows(不做 look-ahead 過濾;留 _lookahead.filter_visible)。

    SQL 用 stock_id IN(...) + fact_date BETWEEN(as_of - lookback) AND as_of。
    `as_of` 上界用 `fact_date <= as_of`(不過 look-ahead 防衛 — 由 lookahead 層處理)。
    """
    if not stock_ids:
        return []
    from datetime import timedelta

    start = as_of - timedelta(days=lookback_days)

    placeholders = ",".join(["%s"] * len(stock_ids))
    sql = f"""
        SELECT stock_id, fact_date, timeframe, source_core, source_version,
               statement, metadata, params_hash
        FROM facts
        WHERE stock_id IN ({placeholders})
          AND fact_date BETWEEN %s AND %s
    """
    params = list(stock_ids) + [start, as_of]
    if cores:
        core_placeholders = ",".join(["%s"] * len(cores))
        sql += f" AND source_core IN ({core_placeholders})"
        params.extend(cores)
    sql += " ORDER BY fact_date DESC, source_core ASC"

    with conn.cursor() as cur:
        cur.execute(sql, params)
        return cur.fetchall()


def fetch_indicator_latest(
    conn,
    *,
    stock_id: str,
    as_of,
    cores: list[str] | None = None,
    timeframes: list[str] | None = None,
) -> list[dict[str, Any]]:
    """每個 (source_core, timeframe) 取 `value_date <= as_of` 最新一筆。"""
    sql = """
        SELECT DISTINCT ON (source_core, timeframe)
               stock_id, value_date, timeframe, source_core, source_version,
               value, params_hash
        FROM indicator_values
        WHERE stock_id = %s
          AND value_date <= %s
    """
    params: list[Any] = [stock_id, as_of]
    if cores:
        core_placeholders = ",".join(["%s"] * len(cores))
        sql += f" AND source_core IN ({core_placeholders})"
        params.extend(cores)
    if timeframes:
        tf_placeholders = ",".join(["%s"] * len(timeframes))
        sql += f" AND timeframe IN ({tf_placeholders})"
        params.extend(timeframes)
    sql += " ORDER BY source_core, timeframe, value_date DESC"

    with conn.cursor() as cur:
        cur.execute(sql, params)
        return cur.fetchall()


def fetch_ohlc(
    conn,
    *,
    stock_id: str,
    as_of,
    lookback_days: int,
) -> list[dict[str, Any]]:
    """從 price_daily_fwd 撈 OHLCV(供 dashboard K-line 用)。

    Args:
        stock_id: 股票代號(支援保留字 _index_taiex_ 等)
        as_of: 上界(包含)
        lookback_days: 期間天數

    Returns:
        list of dict {date, open, high, low, close, volume}, ORDER BY date ASC
    """
    from datetime import timedelta

    start = as_of - timedelta(days=lookback_days)
    sql = """
        SELECT date, open, high, low, close, volume
        FROM price_daily_fwd
        WHERE market = 'TW' AND stock_id = %s
          AND date BETWEEN %s AND %s
        ORDER BY date ASC
    """
    with conn.cursor() as cur:
        cur.execute(sql, [stock_id, start, as_of])
        return cur.fetchall()


def fetch_cross_stock_ranked(
    conn,
    *,
    source_table: str,
    as_of,
    top_n: int = 30,
    rank_col: str = "combined_rank",
    is_top_col: str = "is_top_30",
    extra_cols: list[str] | None = None,
) -> tuple[Any | None, list[dict[str, Any]]]:
    """v3.5 R5 C13:cross-stock ranked 結果通用 fetcher。

    給 cross_cores/ Layer 2.5 各 builder 的對應 MCP / dashboard tool 用
    (magic_formula / 未來 pairs_trading / sector_rotation 都用同一個 helper)。

    Args:
        source_table:  cross-stock derived 表名(e.g. "magic_formula_ranked_derived")
        as_of:         上界(包含);先找 latest ranking_date ≤ as_of
        top_n:         取 top N rank rows
        rank_col:      排名欄名(預設 "combined_rank")
        is_top_col:    top-N 旗標欄名(預設 "is_top_30")
        extra_cols:    額外要 SELECT 的欄位(預設 None = 全選 *)

    Returns:
        (ranking_date, rows):若無資料則 (None, [])
    """
    # 1. latest ranking_date ≤ as_of
    sql_date = f"""
        SELECT MAX(date) AS d FROM {source_table}
         WHERE market = 'TW' AND date <= %s
    """
    with conn.cursor() as cur:
        cur.execute(sql_date, [as_of])
        row = cur.fetchone()
    ranking_date = row["d"] if row else None
    if ranking_date is None:
        return None, []

    # 2. top N rows(LEFT JOIN stock_info_ref 取 name + industry)
    select_cols = "t.*" if not extra_cols else ", ".join(f"t.{c}" for c in extra_cols)
    sql_top = f"""
        SELECT {select_cols},
               s.stock_name, s.industry_category
        FROM {source_table} t
        LEFT JOIN stock_info_ref s
            ON s.market = t.market AND s.stock_id = t.stock_id
        WHERE t.market = 'TW' AND t.date = %s AND t.{is_top_col} = TRUE
        ORDER BY t.{rank_col} ASC
        LIMIT %s
    """
    with conn.cursor() as cur:
        cur.execute(sql_top, [ranking_date, top_n])
        rows = cur.fetchall()
    return ranking_date, rows


def fetch_stock_info_ref(
    conn,
    stock_ids: list[str] | None = None,
) -> dict[str, dict[str, Any]]:
    """v3.5 R5 C13:取 stock_info_ref name + industry,key by stock_id。

    給 cross-stock cores / dashboards 共用(避免每處自寫 SELECT)。
    若 stock_ids 為 None 一次取全市場(~1700 rows)。
    """
    if stock_ids is not None and not stock_ids:
        return {}
    sql = "SELECT stock_id, stock_name, industry_category FROM stock_info_ref WHERE market = 'TW'"
    params: list[Any] = []
    if stock_ids:
        placeholders = ",".join(["%s"] * len(stock_ids))
        sql += f" AND stock_id IN ({placeholders})"
        params.extend(stock_ids)
    with conn.cursor() as cur:
        cur.execute(sql, params)
        rows = cur.fetchall()
    return {r["stock_id"]: r for r in rows}


def fetch_structural_latest(
    conn,
    *,
    stock_id: str,
    as_of,
    cores: list[str] | None = None,
) -> list[dict[str, Any]]:
    """每個 (core_name, timeframe) 取 `snapshot_date <= as_of` 最新一筆。"""
    sql = """
        SELECT DISTINCT ON (core_name, timeframe)
               stock_id, snapshot_date, timeframe, core_name, source_version,
               snapshot, params_hash, derived_from_core
        FROM structural_snapshots
        WHERE stock_id = %s
          AND snapshot_date <= %s
    """
    params: list[Any] = [stock_id, as_of]
    if cores:
        core_placeholders = ",".join(["%s"] * len(cores))
        sql += f" AND core_name IN ({core_placeholders})"
        params.extend(cores)
    sql += " ORDER BY core_name, timeframe, snapshot_date DESC"

    with conn.cursor() as cur:
        cur.execute(sql, params)
        return cur.fetchall()
