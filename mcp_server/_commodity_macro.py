"""Tool 9 內部演算法:`commodity_macro_snapshot` — 商品 macro 信號快照。

對齊 m3Spec/environment_cores.md §十(v3.21 拍版)+ v3.22 MCP B-5。

設計:
- 直接 SELECT commodity_price_daily_derived(Silver,GROUP BY commodity)
- 對每個 commodity 取 <= as_of 最新一筆 + 過去 60 天 z-score / streak / momentum
- payload ~ 1.5 KB / ~400 tokens(對應 1-3 個 commodity)

呼叫端:`mcp_server.tools.data.commodity_macro_snapshot()`。

Reference:
  - Brock, Lakonishok & LeBaron (1992), "Simple Technical Trading Rules"
    *Journal of Finance* 47(5):1731-1764 — macro/low-frequency streak ≥ 5。
  - Hamilton (1989), "A New Approach to the Economic Analysis of Nonstationary
    Time Series and the Business Cycle" *Econometrica* 57(2):357-384 — regime break。
"""

from __future__ import annotations

from datetime import date
from typing import Any

from agg._db import get_connection


SPIKE_THRESHOLD_Z = 2.0    # 對齊 spec §10.3 z_score_threshold


# commodity 中文 / 顯示名(可擴 SILVER / OIL 等)
_COMMODITY_LABELS: dict[str, str] = {
    "GOLD":   "黃金",
    "SILVER": "白銀",
    "OIL":    "原油",
}


def compute_commodity_macro_snapshot(
    as_of: date,
    *,
    commodities: list[str] | None = None,
    database_url: str | None = None,
) -> dict[str, Any]:
    """各 commodity 最新一筆 + macro 信號。

    Args:
        as_of:        查詢日
        commodities:  要取的 commodity 清單(預設 ["GOLD"])
        database_url: 可選 PG 連線字串

    Returns:
        dict ~1.5 KB:
          {
            "as_of": "2026-05-15",
            "snapshot_date": "2026-05-15",
            "commodities": [
              {"name": "GOLD", "label": "黃金", "price": 2630.50,
               "return_pct": +0.85, "return_z_score": 1.23,
               "momentum_state": "up", "streak_days": 4,
               "spike_alert": false}
            ],
            "lookback_days": 60,
            "narrative": "..."
          }
    """
    commodities = commodities or ["GOLD"]
    if not commodities:
        return _empty_result(as_of, commodities)

    conn = get_connection(database_url)
    rows_by_commodity: dict[str, dict[str, Any]] = {}
    latest_date: date | None = None
    try:
        with conn.cursor() as cur:
            cur.execute(
                """
                SELECT DISTINCT ON (commodity)
                       commodity, date,
                       price::float8 AS price, return_pct, return_z_score,
                       momentum_state, streak_days
                  FROM commodity_price_daily_derived
                 WHERE commodity = ANY(%s) AND date <= %s
                 ORDER BY commodity, date DESC
                """,
                [commodities, as_of],
            )
            for r in cur.fetchall() or []:
                rows_by_commodity[r["commodity"]] = r
                if latest_date is None or (r["date"] and r["date"] > latest_date):
                    latest_date = r["date"]
    finally:
        conn.close()

    out_list: list[dict[str, Any]] = []
    for c in commodities:
        r = rows_by_commodity.get(c)
        if not r:
            out_list.append({
                "name":           c,
                "label":          _COMMODITY_LABELS.get(c, c),
                "price":          None,
                "return_pct":     None,
                "return_z_score": None,
                "momentum_state": None,
                "streak_days":    None,
                "spike_alert":    False,
                "data_available": False,
            })
            continue

        z = r.get("return_z_score")
        spike = (z is not None) and (abs(float(z)) >= SPIKE_THRESHOLD_Z)
        out_list.append({
            "name":           c,
            "label":          _COMMODITY_LABELS.get(c, c),
            "price":          _round(r.get("price"), 2),
            "return_pct":     _round(r.get("return_pct"), 4),
            "return_z_score": _round(z, 2),
            "momentum_state": r.get("momentum_state"),
            "streak_days":    r.get("streak_days") or 0,
            "spike_alert":    bool(spike),
            "data_available": True,
        })

    if all(not item["data_available"] for item in out_list):
        return _empty_result(as_of, commodities)

    return {
        "as_of":          as_of.isoformat(),
        "snapshot_date":  latest_date.isoformat() if latest_date else None,
        "commodities":    out_list,
        "lookback_days":  60,
        "narrative": _compose_narrative(items=out_list, as_of=as_of),
    }


def _empty_result(as_of: date, commodities: list[str]) -> dict[str, Any]:
    return {
        "as_of":          as_of.isoformat(),
        "snapshot_date":  None,
        "commodities":    [
            {"name": c, "label": _COMMODITY_LABELS.get(c, c),
             "price": None, "return_pct": None, "return_z_score": None,
             "momentum_state": None, "streak_days": None,
             "spike_alert": False, "data_available": False}
            for c in commodities
        ],
        "lookback_days":  60,
        "narrative": (
            f"{', '.join(commodities)} 無 commodity_price_daily_derived 資料"
            f"(<= {as_of.isoformat()})。請確認 Silver builder commodity_macro 已跑過。"
        ),
    }


def _round(v: Any, digits: int) -> float | None:
    if v is None:
        return None
    return round(float(v), digits)


def _compose_narrative(*, items: list[dict[str, Any]], as_of: date) -> str:
    available = [i for i in items if i["data_available"]]
    if not available:
        return f"當日({as_of.isoformat()})無 commodity 資料。"

    phrases = []
    for it in available:
        state_label = {
            "up": "上漲", "down": "下跌", "neutral": "盤整", None: "未分類",
        }.get(it["momentum_state"], it["momentum_state"])
        streak_phrase = ""
        if it["streak_days"] and it["streak_days"] >= 2:
            streak_phrase = f"連續 {it['streak_days']} 日"
        spike_phrase = "(z 達 spike 警戒)" if it["spike_alert"] else ""
        phrases.append(
            f"{it['label']}({it['name']})收 {it['price']},"
            f"{streak_phrase}{state_label} {(it['return_pct'] or 0):+.2f}%"
            f"{spike_phrase}"
        )
    return ";".join(phrases) + "。"
