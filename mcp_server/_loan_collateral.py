"""Tool 6 內部演算法:`loan_collateral_snapshot` — 5 大類借券抵押餘額快照。

對齊 m3Spec/chip_cores.md §十(v3.21 拍版)+ v3.22 MCP B-5。

設計:
- 直接 SELECT loan_collateral_balance_derived(Silver,5 主欄 + 5 change_pct + JSONB)
- 取 <= as_of 最新一筆(per-stock)
- payload ~ 1.5 KB / ~400 tokens

呼叫端:`mcp_server.tools.data.loan_collateral_snapshot()`。

Reference:
  - Basel Committee on Banking Supervision (2006), "Studies on Credit Risk
    Concentration" Working Paper 15 — CR1 > 0.7 視為 high concentration。
"""

from __future__ import annotations

from datetime import date
from typing import Any

from agg._db import get_connection


# 5 大類英譯 → 中文 label(narrative 用)
_CATEGORY_LABELS: dict[str, str] = {
    "margin":             "融資",
    "firm_loan":          "券商自有借券",
    "unrestricted_loan":  "無限制借券",
    "finance_loan":       "證金擔保借券",
    "settlement_margin":  "交割保證金",
}

CONCENTRATION_THRESHOLD = 0.70   # 對齊 spec §10.3


def compute_loan_collateral_snapshot(
    stock_id: str,
    as_of: date,
    *,
    database_url: str | None = None,
) -> dict[str, Any]:
    """5 大類借券抵押餘額 + 集中度警示。

    Args:
        stock_id:     股票代號(例 "2330")
        as_of:        查詢日;回 <= as_of 最新一筆
        database_url: 可選 PG 連線字串

    Returns:
        dict ~1.5 KB:
          {
            "stock_id": "2330",
            "as_of": "2026-05-15",
            "snapshot_date": "2026-05-14",  # 實際資料日
            "categories": {
              "margin":             {"balance": 26483, "change_pct": +1.22, "ratio": 0.24},
              "firm_loan":          {...},
              ...
            },
            "total_balance": 112321,
            "dominant_category": "unrestricted_loan",
            "dominant_category_label": "無限制借券",
            "concentration_ratio": 0.69,
            "concentration_alert": false,
            "narrative": "..."
          }
    """
    conn = get_connection(database_url)
    try:
        with conn.cursor() as cur:
            cur.execute(
                """
                SELECT date,
                       margin_current_balance, firm_loan_current_balance,
                       unrestricted_loan_current_balance, finance_loan_current_balance,
                       settlement_margin_current_balance,
                       margin_change_pct, firm_loan_change_pct,
                       unrestricted_loan_change_pct, finance_loan_change_pct,
                       settlement_margin_change_pct,
                       total_balance, dominant_category, dominant_category_ratio
                  FROM loan_collateral_balance_derived
                 WHERE stock_id = %s AND date <= %s
                 ORDER BY date DESC
                 LIMIT 1
                """,
                [stock_id, as_of],
            )
            row = cur.fetchone()
    finally:
        conn.close()

    if not row:
        return _empty_result(stock_id, as_of, reason="no_loan_collateral_data")

    total = row.get("total_balance") or 0
    categories: dict[str, dict[str, Any]] = {}
    for cat in _CATEGORY_LABELS:
        balance = row.get(f"{cat}_current_balance") or 0
        change = row.get(f"{cat}_change_pct")
        categories[cat] = {
            "balance": int(balance),
            "change_pct": _round(change, 2),
            "ratio": _round(balance / total, 4) if total > 0 else None,
        }

    dominant = row.get("dominant_category")
    concentration = row.get("dominant_category_ratio")
    concentration_alert = (
        concentration is not None and concentration >= CONCENTRATION_THRESHOLD
    )

    return {
        "stock_id":                  stock_id,
        "as_of":                     as_of.isoformat(),
        "snapshot_date":             row["date"].isoformat(),
        "categories":                categories,
        "total_balance":             int(total),
        "dominant_category":         dominant,
        "dominant_category_label":   _CATEGORY_LABELS.get(dominant or "", dominant or ""),
        "concentration_ratio":       _round(concentration, 4),
        "concentration_alert":       bool(concentration_alert),
        "narrative": _compose_narrative(
            stock_id=stock_id, total=total, dominant=dominant,
            concentration=concentration, categories=categories,
        ),
    }


def _empty_result(stock_id: str, as_of: date, *, reason: str) -> dict[str, Any]:
    return {
        "stock_id":                stock_id,
        "as_of":                   as_of.isoformat(),
        "snapshot_date":           None,
        "categories":              {},
        "total_balance":           0,
        "dominant_category":       None,
        "dominant_category_label": None,
        "concentration_ratio":     None,
        "concentration_alert":     False,
        "narrative": (
            f"{stock_id} 無借券抵押餘額資料({reason})。請確認 Silver builder "
            f"loan_collateral 已對 as_of {as_of.isoformat()} 之前的 date 跑過。"
        ),
    }


def _round(v: float | None, digits: int) -> float | None:
    if v is None:
        return None
    return round(float(v), digits)


def _compose_narrative(
    *, stock_id: str, total: int, dominant: str | None,
    concentration: float | None, categories: dict[str, dict[str, Any]],
) -> str:
    if total <= 0:
        return f"{stock_id} 5 類借券餘額皆為 0;當日無借券活動。"

    dom_label = _CATEGORY_LABELS.get(dominant or "", dominant or "")
    conc_pct = (concentration or 0.0) * 100
    alert_phrase = "已達" if concentration and concentration >= CONCENTRATION_THRESHOLD else "未達"

    # 找變化最大的類別
    biggest_change = max(
        categories.items(),
        key=lambda kv: abs(kv[1].get("change_pct") or 0.0),
    )
    bc_name, bc_data = biggest_change
    bc_pct = bc_data.get("change_pct") or 0.0
    change_phrase = ""
    if abs(bc_pct) >= 5.0:
        bc_label = _CATEGORY_LABELS.get(bc_name, bc_name)
        change_phrase = f";{bc_label} 變化 {bc_pct:+.1f}%"

    return (
        f"{stock_id} 5 類借券共 {total:,} 股,「{dom_label}」主導 {conc_pct:.1f}%"
        f"{alert_phrase} 70% 集中警戒{change_phrase}。"
    )
