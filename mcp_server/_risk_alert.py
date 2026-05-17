"""Tool 8 內部演算法:`risk_alert_status` — 處置股風險警示狀態。

對齊 m3Spec/chip_cores.md §十二(v3.21 拍版)+ v3.22 MCP B-5。

設計:
- 直讀 Bronze `disposition_securities_period_tw`(對齊 fear_greed 例外風格,
  事件性低頻無需 Silver derived)
- as_of 當日是否在 period_start..period_end 區間 → in_disposition_period
- 60 天內 disposition 次數 → escalation_count_60d
- payload ~ 1.5 KB / ~400 tokens

呼叫端:`mcp_server.tools.data.risk_alert_status()`。

Reference:
  - 「證券交易所公布注意交易資訊處置作業要點」§4(2024 版)— 60 日內 ≥ 2 次升級。
"""

from __future__ import annotations

from datetime import date, timedelta
from typing import Any

from agg._db import get_connection


# 三級嚴重度中文對照
_SEVERITY_LABELS: dict[str, str] = {
    "warning":     "注意股",
    "disposition": "處置股(分盤撮合)",
    "cash_only":   "全額交割",
    "unknown":     "未分類",
}

ESCALATION_WINDOW_DAYS = 60   # 對齊監管 §4


def _parse_severity(measure: str | None) -> str:
    if not measure:
        return "unknown"
    if "全額交割" in measure:
        return "cash_only"
    if "人工管制" in measure:
        return "disposition"
    if "注意交易資訊" in measure:
        return "warning"
    return "unknown"


def compute_risk_alert_status(
    stock_id: str,
    as_of: date,
    *,
    database_url: str | None = None,
) -> dict[str, Any]:
    """處置股當下狀態 + 60 天升級鏈。

    Args:
        stock_id:     股票代號
        as_of:        查詢日
        database_url: 可選 PG 連線字串

    Returns:
        dict ~1.5 KB:
          {
            "stock_id": "3363",
            "as_of": "2026-05-15",
            "current_status": {
              "in_disposition_period": true,
              "severity": "disposition",
              "severity_label": "處置股(分盤撮合)",
              "period_start": "2025-01-14",
              "period_end": "2025-02-07",
              "days_remaining": 12,
            },
            "history_60d": [
              {"announced_date": "2025-01-13", "severity": "disposition", "cnt": 2},
            ],
            "escalation_count_60d": 1,
            "narrative": "..."
          }
    """
    conn = get_connection(database_url)
    window_start = as_of - timedelta(days=ESCALATION_WINDOW_DAYS)
    try:
        with conn.cursor() as cur:
            cur.execute(
                """
                SELECT date AS announced_date, disposition_cnt,
                       period_start, period_end, condition, measure
                  FROM disposition_securities_period_tw
                 WHERE stock_id = %s AND date <= %s AND date >= %s
                 ORDER BY date DESC
                """,
                [stock_id, as_of, window_start],
            )
            rows = cur.fetchall() or []
    finally:
        conn.close()

    # current_status:as_of 當日是否在任一 row 的 period_start..period_end 內
    current_row = None
    for r in rows:
        ps, pe = r.get("period_start"), r.get("period_end")
        if ps and pe and ps <= as_of <= pe:
            current_row = r
            break

    if current_row:
        cur_severity = _parse_severity(current_row.get("measure"))
        days_remaining = (current_row["period_end"] - as_of).days
        current_status = {
            "in_disposition_period": True,
            "severity":              cur_severity,
            "severity_label":        _SEVERITY_LABELS.get(cur_severity, cur_severity),
            "period_start":          current_row["period_start"].isoformat(),
            "period_end":            current_row["period_end"].isoformat(),
            "days_remaining":        max(days_remaining, 0),
        }
    else:
        current_status = {
            "in_disposition_period": False,
            "severity":              None,
            "severity_label":        None,
            "period_start":          None,
            "period_end":            None,
            "days_remaining":        None,
        }

    # history_60d:全 60 天內的 disposition 公告(對齊監管 escalation 規則)
    history_60d: list[dict[str, Any]] = []
    for r in rows:
        sev = _parse_severity(r.get("measure"))
        history_60d.append({
            "announced_date": r["announced_date"].isoformat(),
            "severity":       sev,
            "severity_label": _SEVERITY_LABELS.get(sev, sev),
            "cnt":            r.get("disposition_cnt") or 0,
            "period_start":   r["period_start"].isoformat() if r.get("period_start") else None,
            "period_end":     r["period_end"].isoformat() if r.get("period_end") else None,
        })

    escalation_count = len(history_60d)

    return {
        "stock_id":              stock_id,
        "as_of":                 as_of.isoformat(),
        "current_status":        current_status,
        "history_60d":           history_60d[:10],   # truncate budget(典型 < 5)
        "escalation_count_60d":  escalation_count,
        "narrative": _compose_narrative(
            stock_id=stock_id, current_status=current_status,
            escalation_count=escalation_count, as_of=as_of,
        ),
    }


def _compose_narrative(
    *, stock_id: str, current_status: dict[str, Any],
    escalation_count: int, as_of: date,
) -> str:
    if not current_status["in_disposition_period"]:
        if escalation_count == 0:
            return f"{stock_id} 過去 60 日無處置警示;當前無風險警訊。"
        return (
            f"{stock_id} 當前未在處置期間,但過去 60 日內有 {escalation_count} 次"
            f"處置公告,屬監管關注標的。"
        )

    sev_label = current_status["severity_label"]
    days_left = current_status["days_remaining"]
    period_end = current_status["period_end"]
    escalation_phrase = ""
    if escalation_count >= 2:
        escalation_phrase = f";60 日內第 {escalation_count} 次處置,風險警訊升級"

    return (
        f"{stock_id} 目前處於「{sev_label}」期間,剩餘 {days_left} 天"
        f"(至 {period_end}){escalation_phrase}。"
    )
