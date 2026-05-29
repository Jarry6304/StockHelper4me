"""Golden L3 — 讀已物化 fusion row(MCP serving + Web API 共用)。

對齊 fetch_structural_latest(src/fusion/raw/_db.py:288)的 DISTINCT ON 慣例,但
fusion 讀只需「某 core_name 的 latest <= as_of 一筆」。供:
- MCP 工具 stock_levels / dual_track_resonance / market_context 改讀物化(缺 → compute fallback)
- Web API generic passthrough handler
"""

from __future__ import annotations

from datetime import date
from typing import Any


def fetch_fusion_doc(
    conn,
    *,
    stock_id: str,
    as_of: date,
    core_name: str,
    timeframe: str | None = None,
) -> dict[str, Any] | None:
    """取 (stock_id, core_name[, timeframe]) 的 `snapshot_date <= as_of` 最新一筆。

    Returns:
        row dict {snapshot, snapshot_date, timeframe, source_version, params_hash}
        或 None(無物化 row → caller 走 compute fallback)。
        `snapshot` 已是 dict(psycopg jsonb → dict)。
    """
    sql = """
        SELECT snapshot, snapshot_date, timeframe, source_version, params_hash
        FROM structural_snapshots
        WHERE stock_id = %s AND core_name = %s AND snapshot_date <= %s
    """
    params: list[Any] = [stock_id, core_name, as_of]
    if timeframe is not None:
        sql += " AND timeframe = %s"
        params.append(timeframe)
    sql += " ORDER BY snapshot_date DESC LIMIT 1"

    with conn.cursor() as cur:
        cur.execute(sql, params)
        return cur.fetchone()
