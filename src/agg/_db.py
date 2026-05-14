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
