"""Golden 讀取 passthrough — 取 snapshot::text 原文直出(不 deserialize、不建 model)。

對齊 m3Spec/read-api.md:每個 Golden 讀都是 `SELECT snapshot WHERE core_name=?`,
一個 generic handler 服務 forest / levels / resonance / climate / snapshot。
forest 保險絲:N > 250 → 422(引擎 cap 200,production max 37)。
"""

from __future__ import annotations

from datetime import date
from typing import Any

from fastapi import HTTPException, Response

# forest 完整性保險絲(高於引擎 cap 200 留 headroom;與 cap 解耦)
FOREST_FUSE_CAP = 250


async def fetch_snapshot_text(
    pool: Any,
    *,
    stock_id: str,
    as_of: date,
    core_name: str,
    timeframe: str | None = None,
) -> str | None:
    """取 (stock_id, core_name[, timeframe]) 的 snapshot_date <= as_of 最新一筆 JSON 原文。"""
    sql = (
        "SELECT snapshot::text AS j FROM structural_snapshots "
        "WHERE stock_id = %s AND core_name = %s AND snapshot_date <= %s"
    )
    params: list[Any] = [stock_id, core_name, as_of]
    if timeframe is not None:
        sql += " AND timeframe = %s"
        params.append(timeframe)
    sql += " ORDER BY snapshot_date DESC LIMIT 1"

    async with pool.connection() as conn:
        async with conn.cursor() as cur:
            await cur.execute(sql, params)
            row = await cur.fetchone()
    return row["j"] if row else None


async def scenario_forest_len(
    pool: Any,
    *,
    stock_id: str,
    as_of: date,
    timeframe: str = "daily",
) -> int | None:
    """取 neely_core 最新 snapshot 的 scenario_forest 長度(不解析整 doc)。None = 無 row。"""
    sql = (
        "SELECT COALESCE(jsonb_array_length(snapshot->'scenario_forest'), 0) AS n "
        "FROM structural_snapshots "
        "WHERE stock_id = %s AND core_name = 'neely_core' "
        "  AND snapshot_date <= %s AND timeframe = %s "
        "ORDER BY snapshot_date DESC LIMIT 1"
    )
    async with pool.connection() as conn:
        async with conn.cursor() as cur:
            await cur.execute(sql, [stock_id, as_of, timeframe])
            row = await cur.fetchone()
    return row["n"] if row else None


def raw_json_response(text: str | None) -> Response:
    """原文直出 application/json;None → 404。"""
    if text is None:
        raise HTTPException(status_code=404, detail="not_found")
    return Response(content=text, media_type="application/json")


async def guard_forest_cap(pool: Any, *, stock_id: str, as_of: date, timeframe: str) -> None:
    """neely forest 完整性保險絲:N > 250 → 422(不靜默截斷)。None(無 row)放行交 404。"""
    n = await scenario_forest_len(pool, stock_id=stock_id, as_of=as_of, timeframe=timeframe)
    if n is not None and n > FOREST_FUSE_CAP:
        raise HTTPException(
            status_code=422,
            detail=f"forest_overflow: scenario_forest size {n} exceeds fuse cap "
                   f"{FOREST_FUSE_CAP} (engine cap 200, prod max 37)",
        )
