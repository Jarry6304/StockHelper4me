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


def extract_snapshot_with_provenance(
    row: Any, as_of: date,
) -> dict[str, Any] | None:
    """從 `fetch_fusion_doc` 的 row 取 snapshot dict,附上 `_provenance` 新鮮度揭露。

    物化讀回的是「snapshot_date <= as_of 的最新一筆」,該 snapshot_date 可能落後
    as_of(物化批次未跑到那麼新)。caller 過去只取 snapshot dict、丟掉 snapshot_date,
    LLM 無從得知資料其實落後 → 與 compute fallback(精確 as_of 現算)混在一起無法區分。

    本 helper 統一把 snapshot_date / staleness_days 揭露進 `_provenance`(對齊既有
    indicator_staleness / scenario_staleness 慣例:揭露新鮮度,讓 LLM 自行判斷)。
    物化路徑永遠帶 `_provenance.source="materialized"`;compute fallback 不帶 →
    「有無 _provenance」即可區分資料來源。

    Returns:
        snapshot dict(含 `_provenance`)或 None(無 row / snapshot 非 dict → caller
        走 compute fallback)。不 mutate 原 row(`{**snap, ...}`)。
    """
    snap = row.get("snapshot") if hasattr(row, "get") else None
    if not isinstance(snap, dict):
        return None
    sd = row.get("snapshot_date") if hasattr(row, "get") else None
    staleness: int | None = None
    if sd is not None:
        try:
            staleness = (as_of - sd).days
        except Exception:  # noqa: BLE001 — sd 型別異常不擋資料,只 provenance 缺值
            staleness = None
    return {
        **snap,
        "_provenance": {
            "source": "materialized",
            "snapshot_date": (
                sd.isoformat() if hasattr(sd, "isoformat")
                else (str(sd) if sd is not None else None)
            ),
            "as_of": as_of.isoformat(),
            "staleness_days": staleness,
            "is_stale": bool(staleness is not None and staleness > 0),
        },
    }
