"""Aggregation Layer 主入口 — as_of(stock_id, date) → AsOfSnapshot。

對齊 m3Spec/aggregation_layer.md §四 / §九 use cases。

設計重點:
- as_of 強制參數 — 不允許隱含「最新」
- look-ahead bias 防衛集中在 _lookahead.filter_visible()
- market 並排走 _market.fetch_market_facts() 5 保留字
- 不做跨 Core 整合(對齊 §九 / §十一)
"""

from __future__ import annotations

from datetime import date
from typing import Any

from agg._db import (
    fetch_facts,
    fetch_indicator_latest,
    fetch_ohlc,
    fetch_structural_latest,
    get_connection,
)
from agg._lookahead import filter_visible
from agg._market import fetch_market_facts
from agg._types import (
    AsOfSnapshot,
    FactRow,
    IndicatorRow,
    QueryMetadata,
    StructuralRow,
)


def as_of(
    stock_id: str,
    as_of: date,
    *,
    cores: list[str] | None = None,
    lookback_days: int = 90,
    include_market: bool = True,
    timeframes: list[str] | None = None,
    database_url: str | None = None,
    conn=None,
) -> AsOfSnapshot:
    """單股 as_of 快照查詢。

    Args:
        stock_id: 股票代號(例 "2330",或保留字 "_index_taiex_")
        as_of: 查詢日(回測 / 即時都用同一介面)
        cores: 限制 source_core 範圍。None = 全部 23 cores
        lookback_days: facts 期間。預設 90 天
        include_market: 是否並排 market-level facts(5 個保留字 stock_id)
        timeframes: 限制 indicator timeframe。None = 全部
        database_url: PG 連線字串。None 走 .env / 環境變數
        conn: 既有 psycopg connection(test / 重複利用)。None 則開新連線

    Returns:
        AsOfSnapshot dataclass

    Raises:
        RuntimeError: 無法解出 DATABASE_URL
        ImportError: psycopg 未安裝
    """
    # Input validation(失敗 early,避免 silently 跑 SQL 或誤觸 spec gap)
    if not stock_id or not stock_id.strip():
        raise ValueError("stock_id 不可為空字串")
    if lookback_days < 0:
        raise ValueError(f"lookback_days 不可為負(got {lookback_days})")
    if cores is not None and len(cores) == 0:
        # 明確傳空 list 容易誤殺整次查詢,raise 比 silently 回空 snapshot 安全
        raise ValueError("cores 不可為空 list;若要查全部請傳 None")

    owns_conn = conn is None
    if owns_conn:
        conn = get_connection(database_url)

    try:
        # 1. 個股 facts(直接拉 + look-ahead 過濾)
        raw_facts = fetch_facts(
            conn,
            stock_ids=[stock_id],
            as_of=as_of,
            lookback_days=lookback_days,
            cores=cores,
        )
        visible_facts = filter_visible(raw_facts, as_of)
        facts = [_to_fact_row(r) for r in visible_facts]

        # 2. 個股 indicator_values 最新一筆 per (core, timeframe)
        indicator_rows = fetch_indicator_latest(
            conn,
            stock_id=stock_id,
            as_of=as_of,
            cores=cores,
            timeframes=timeframes,
        )
        # 同 source_core 不同 timeframe 都保留;dict key = "core_name@timeframe"
        indicator_latest: dict[str, IndicatorRow] = {}
        for r in indicator_rows:
            key = _indicator_key(r["source_core"], r["timeframe"])
            indicator_latest[key] = _to_indicator_row(r)

        # 3. structural_snapshots 最新一筆 per (core, timeframe)
        structural_rows = fetch_structural_latest(
            conn,
            stock_id=stock_id,
            as_of=as_of,
            cores=cores,
        )
        structural: dict[str, StructuralRow] = {}
        for r in structural_rows:
            key = _indicator_key(r["core_name"], r["timeframe"])
            structural[key] = _to_structural_row(r)

        # 4. market-level facts(5 保留字 stock_id 並排;_market.fetch_market_facts 內建 look-ahead filter)
        market: dict[str, list[FactRow]] = {}
        if include_market:
            visible_market = fetch_market_facts(
                conn,
                as_of=as_of,
                lookback_days=lookback_days,
                cores=cores,
            )
            for sid, sid_facts in visible_market.items():
                market[sid] = [_to_fact_row(r) for r in sid_facts]

        metadata = QueryMetadata(
            stock_id=stock_id,
            as_of=as_of,
            lookback_days=lookback_days,
            cores=list(cores) if cores else None,
            include_market=include_market,
            timeframes=list(timeframes) if timeframes else None,
        )
        return AsOfSnapshot(
            stock_id=stock_id,
            as_of=as_of,
            facts=facts,
            indicator_latest=indicator_latest,
            structural=structural,
            market=market,
            metadata=metadata,
        )
    finally:
        if owns_conn:
            conn.close()


# ────────────────────────────────────────────────────────────
# Row converters
# ────────────────────────────────────────────────────────────

def _to_fact_row(r: dict[str, Any]) -> FactRow:
    return FactRow(
        stock_id=r["stock_id"],
        fact_date=r["fact_date"],
        timeframe=r["timeframe"],
        source_core=r["source_core"],
        source_version=r["source_version"],
        statement=r["statement"],
        metadata=r.get("metadata") or {},
        params_hash=r.get("params_hash"),
    )


def _to_indicator_row(r: dict[str, Any]) -> IndicatorRow:
    return IndicatorRow(
        stock_id=r["stock_id"],
        value_date=r["value_date"],
        timeframe=r["timeframe"],
        source_core=r["source_core"],
        source_version=r["source_version"],
        value=r.get("value") or {},
        params_hash=r.get("params_hash") or "",
    )


def _to_structural_row(r: dict[str, Any]) -> StructuralRow:
    return StructuralRow(
        stock_id=r["stock_id"],
        snapshot_date=r["snapshot_date"],
        timeframe=r["timeframe"],
        core_name=r["core_name"],
        source_version=r["source_version"],
        snapshot=r.get("snapshot") or {},
        params_hash=r.get("params_hash") or "",
        derived_from_core=r.get("derived_from_core"),
    )


def _indicator_key(core: str, timeframe: str) -> str:
    """組裝 dict key:同 core 不同 timeframe 分開(例 ma_core@daily / ma_core@weekly)。"""
    return f"{core}@{timeframe}"


# ────────────────────────────────────────────────────────────
# Convenience helpers
# ────────────────────────────────────────────────────────────

def as_of_with_ohlc(
    stock_id: str,
    as_of_date: date,
    *,
    cores: list[str] | None = None,
    lookback_days: int = 90,
    include_market: bool = True,
    timeframes: list[str] | None = None,
    database_url: str | None = None,
    conn=None,
) -> tuple[AsOfSnapshot, list[dict[str, Any]]]:
    """組合 as_of() + fetch_ohlc(),dashboards 一次撈完所需資料。

    舊 as_of() 簽章不動;本函式只是便利 wrapper。

    Returns:
        (snapshot, ohlc_rows)
    """
    owns_conn = conn is None
    if owns_conn:
        conn = get_connection(database_url)
    try:
        snapshot = as_of(
            stock_id,
            as_of_date,
            cores=cores,
            lookback_days=lookback_days,
            include_market=include_market,
            timeframes=timeframes,
            conn=conn,
        )
        ohlc = fetch_ohlc(
            conn,
            stock_id=stock_id,
            as_of=as_of_date,
            lookback_days=lookback_days,
        )
        return snapshot, ohlc
    finally:
        if owns_conn:
            conn.close()


def health_check(
    database_url: str | None = None,
    *,
    conn=None,
) -> dict[str, Any]:
    """快速確認 PG 可達 + Aggregation Layer 三表存在 + row counts。

    對齊 m3Spec/aggregation_layer.md §三 三張 M3 表。供 dashboard / CLI / MCP server
    啟動時呼叫,失敗時點明是 PG / schema / 三表中哪一環掛掉。

    Args:
        database_url: 可選 PG 連線字串
        conn: 既有 connection(重複利用)

    Returns:
        dict {
            "ok": bool,                 # True 若 PG 連得到 + 三表皆存在
            "database_url": str | None, # 不含 password masked
            "tables": {
                "facts": {"exists": bool, "row_count": int | None},
                "indicator_values": {...},
                "structural_snapshots": {...},
            },
            "errors": list[str],
        }
    """
    result: dict[str, Any] = {
        "ok": False,
        "database_url": None,
        "tables": {},
        "errors": [],
    }
    owns_conn = conn is None
    try:
        if owns_conn:
            conn = get_connection(database_url)
    except Exception as e:
        result["errors"].append(f"connect_failed: {type(e).__name__}: {e}")
        return result

    try:
        for tbl in ("facts", "indicator_values", "structural_snapshots"):
            try:
                with conn.cursor() as cur:
                    cur.execute(
                        "SELECT to_regclass(%s) IS NOT NULL AS exists_flag",
                        [tbl],
                    )
                    row = cur.fetchone()
                    exists = bool(row and row.get("exists_flag"))
                    info: dict[str, Any] = {"exists": exists, "row_count": None}
                    if exists:
                        cur.execute(f"SELECT COUNT(*) AS n FROM {tbl}")
                        cnt_row = cur.fetchone()
                        info["row_count"] = int(cnt_row["n"]) if cnt_row else None
                    result["tables"][tbl] = info
            except Exception as e:
                result["tables"][tbl] = {"exists": False, "row_count": None}
                result["errors"].append(f"{tbl}: {type(e).__name__}: {e}")

        result["ok"] = (
            not result["errors"]
            and all(t.get("exists") for t in result["tables"].values())
        )
        # PG dsn 通常不放 password 在 conn.info.dsn 之外的位置,但保險不回 raw url
        try:
            result["database_url"] = f"{conn.info.host}:{conn.info.port}/{conn.info.dbname}"
        except Exception:
            result["database_url"] = "unknown"
        return result
    finally:
        if owns_conn and conn is not None:
            conn.close()


def find_facts_today(
    today: date,
    *,
    source_core: str | None = None,
    kind: str | None = None,
    database_url: str | None = None,
    conn=None,
) -> list[FactRow]:
    """跨 stock 搜尋:今天有哪些股票觸發某 fact(對齊 §9.4 use case)。

    Args:
        today: 查詢日
        source_core: 限制 source_core(例 "rsi_core")
        kind: 限制 metadata.kind(例 "RsiOversold");走 JSONB 過濾
        database_url / conn: 同 as_of()

    Returns:
        當日該 fact 的 FactRow list(已過 look-ahead 防衛)
    """
    owns_conn = conn is None
    if owns_conn:
        conn = get_connection(database_url)

    try:
        sql_parts = [
            "SELECT stock_id, fact_date, timeframe, source_core, source_version,",
            "       statement, metadata, params_hash",
            "FROM facts",
            "WHERE fact_date = %s",
        ]
        params: list[Any] = [today]
        if source_core:
            sql_parts.append("AND source_core = %s")
            params.append(source_core)
        if kind:
            # metadata.kind JSONB 過濾
            sql_parts.append("AND metadata ->> 'kind' = %s")
            params.append(kind)
        sql_parts.append("ORDER BY stock_id ASC")
        sql = "\n".join(sql_parts)

        with conn.cursor() as cur:
            cur.execute(sql, params)
            rows = cur.fetchall()

        visible = filter_visible(rows, today)
        return [_to_fact_row(r) for r in visible]
    finally:
        if owns_conn:
            conn.close()
