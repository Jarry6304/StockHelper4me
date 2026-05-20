"""Fusion Layer · Integration 端口 — market_events(D 視角)。

對齊 m3Spec/fusion_layer.md §4.2 / §8.2 + api_roadmap_v1.md §6.4.2。

把 environment cores 寫進 `facts` 的事件,依日期區間 + severity filter 後,以統一
Event schema 回傳時間軸。**純 SQL filter** — severity 由各 core 寫入時決定,本層
不二次判斷(fusion_layer §9 #6)。
"""

from __future__ import annotations

from datetime import date
from typing import Any

from fusion._shared import (
    ENVIRONMENT_CORES,
    fact_to_event,
    severity_to_int,
    severity_to_label,
)
from fusion.raw._db import get_connection


def market_events(
    start_date: date,
    end_date: date,
    *,
    severity_min: str = "info",
    database_url: str | None = None,
    conn: Any = None,
) -> dict[str, Any]:
    """environment cores 的事件時間軸。

    Args:
        start_date: 起始日(含)。
        end_date:   結束日(含)— 同時是 look-ahead 上界。environment facts 皆為
                    日頻(fact_date = 事件當日),故 `fact_date <= end_date` 即完整
                    look-ahead 防衛(對齊 fusion_layer §9 #5)。
        severity_min: 最低嚴重度(info / notable / warning / critical)。
        database_url / conn: 連線;傳 conn 則重用不自行開關。

    Returns:
        {start_date, end_date, severity_min, event_count, by_severity, events}
        events 依 (date DESC, severity DESC) 排序,每筆統一 schema:
        {date, source, kind, severity, statement, value, metadata}
    """
    min_rank = severity_to_int(severity_min)
    own_conn = conn is None
    if own_conn:
        conn = get_connection(database_url)
    try:
        rows = _fetch_env_events(conn, start_date, end_date, min_rank)
    finally:
        if own_conn:
            conn.close()

    events = [fact_to_event(r) for r in rows]
    by_severity: dict[str, int] = {}
    for e in events:
        by_severity[e["severity"]] = by_severity.get(e["severity"], 0) + 1

    return {
        "start_date": start_date.isoformat(),
        "end_date": end_date.isoformat(),
        "severity_min": severity_to_label(min_rank),
        "event_count": len(events),
        "by_severity": by_severity,
        "events": events,
    }


def _fetch_env_events(
    conn: Any, start_date: date, end_date: date, min_rank: int
) -> list[dict[str, Any]]:
    """撈 environment cores 在區間內 severity >= min_rank 的 facts。"""
    placeholders = ",".join(["%s"] * len(ENVIRONMENT_CORES))
    sql = f"""
        SELECT stock_id, fact_date, timeframe, source_core,
               statement, metadata, severity
        FROM facts
        WHERE source_core IN ({placeholders})
          AND fact_date BETWEEN %s AND %s
          AND severity >= %s
        ORDER BY fact_date DESC, severity DESC, source_core ASC
    """
    params = list(ENVIRONMENT_CORES) + [start_date, end_date, min_rank]
    with conn.cursor() as cur:
        cur.execute(sql, params)
        return cur.fetchall()


# fact → 統一 Event schema 的轉換走 fusion._shared.fact_to_event(共用 helper)。
