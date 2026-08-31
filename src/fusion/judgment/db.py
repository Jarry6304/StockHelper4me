"""wave_judgments 存取(m3Spec/wave_judgment_loop.md §5)。

append-only PIT:狀態變更一律 INSERT 新列 + supersedes_id;UPDATE/DELETE 被
DB trigger 拒絕(P0001)。「active judgment」= `status='active'` 且無子列
(supersedes 鏈最新);同 (stock, timeframe) 人與 LLM 並存時消費端取
human 優先、其次最新(§11)。
"""

from __future__ import annotations

import json
from typing import Any

__all__ = [
    "fetch_active_judgment",
    "fetch_active_judgments_batch",
    "fetch_all_active_judgments",
    "fetch_judgments",
    "insert_judgment",
]

_JSONB_COLS = ("accepted", "rationale", "invalidation", "diff_detail")

# active = status='active' 且無子列(鏈最新);human 優先、其次最新(§11)
_ACTIVE_WHERE = """
    w.status = 'active'
    AND NOT EXISTS (
        SELECT 1 FROM wave_judgments c WHERE c.supersedes_id = w.id
    )
"""
_ACTIVE_ORDER = "(w.judged_by = 'human') DESC, w.as_of DESC, w.created_at DESC"


def fetch_active_judgment(conn, *, stock_id: str, timeframe: str) -> dict[str, Any] | None:
    """單檔單 timeframe 的 active judgment(消費端優先序取一筆)。"""
    sql = f"""
        SELECT w.* FROM wave_judgments w
        WHERE w.stock_id = %s AND w.timeframe = %s AND {_ACTIVE_WHERE}
        ORDER BY {_ACTIVE_ORDER}
        LIMIT 1
    """
    with conn.cursor() as cur:
        cur.execute(sql, [stock_id, timeframe])
        return cur.fetchone()


def fetch_active_judgments_batch(
    conn, *, stock_ids: list[str], timeframe: str,
) -> dict[str, dict[str, Any]]:
    """多檔批次(V2 /waves/summary 用):每檔取消費優先序第一筆。"""
    if not stock_ids:
        return {}
    sql = f"""
        SELECT DISTINCT ON (w.stock_id) w.*
        FROM wave_judgments w
        WHERE w.stock_id = ANY(%s) AND w.timeframe = %s AND {_ACTIVE_WHERE}
        ORDER BY w.stock_id, {_ACTIVE_ORDER}
    """
    with conn.cursor() as cur:
        cur.execute(sql, [stock_ids, timeframe])
        return {r["stock_id"]: r for r in cur.fetchall()}


def fetch_all_active_judgments(conn) -> list[dict[str, Any]]:
    """全部 active 列(J2 diff 逐筆比對用;不做消費優先序 — 每筆各自追蹤)。"""
    sql = f"""
        SELECT w.* FROM wave_judgments w
        WHERE {_ACTIVE_WHERE}
        ORDER BY w.stock_id, w.timeframe, w.id
    """
    with conn.cursor() as cur:
        cur.execute(sql)
        return cur.fetchall()


def fetch_judgments(
    conn, *, stock_id: str, timeframe: str | None = None, limit: int = 50,
) -> list[dict[str, Any]]:
    """單檔判讀歷史(supersedes 鏈全列,新到舊;CLI `judgment list` 用)。"""
    sql = "SELECT w.* FROM wave_judgments w WHERE w.stock_id = %s"
    params: list[Any] = [stock_id]
    if timeframe is not None:
        sql += " AND w.timeframe = %s"
        params.append(timeframe)
    sql += " ORDER BY w.id DESC LIMIT %s"
    params.append(limit)
    with conn.cursor() as cur:
        cur.execute(sql, params)
        return cur.fetchall()


def insert_judgment(conn, row: dict[str, Any]) -> int:
    """INSERT 一筆判讀(或 J2 狀態列),回傳 id。

    caller 責任:row 已過 `validate.validate_judgment`(人/LLM 判讀)或為
    J2 diff 產物(內容欄拷貝原列);jsonb 欄位收 Python dict/list。
    """
    cols = [
        "stock_id", "timeframe", "as_of", "judged_by",
        "snapshot_date", "params_hash", "engine_version", "assumption_hash",
        "accepted", "degree_read", "rationale", "invalidation",
        "confidence_class", "status", "supersedes_id", "diff_detail",
    ]
    values = []
    for c in cols:
        v = row.get(c)
        if c in _JSONB_COLS and v is not None:
            v = json.dumps(v, ensure_ascii=False)
        if c == "status" and v is None:
            v = "active"
        values.append(v)
    placeholders = ", ".join(["%s"] * len(cols))
    sql = f"""
        INSERT INTO wave_judgments ({", ".join(cols)})
        VALUES ({placeholders})
        RETURNING id
    """
    with conn.cursor() as cur:
        cur.execute(sql, values)
        new_id = cur.fetchone()["id"]
    return int(new_id)
