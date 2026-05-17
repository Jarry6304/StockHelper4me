"""
silver/builders/loan_collateral.py
===================================
loan_collateral_balance_tw (Bronze 35 cols) → loan_collateral_balance_derived
(Silver:5 主欄 + 5 change_pct + 跨類聚合 + JSONB pack 其他 25)。

對齊 m3Spec/chip_cores.md §10.2 拍版設計:
- 5 主欄:{margin/firm_loan/unrestricted_loan/finance_loan/settlement_margin}_current_balance
- 5 衍生欄:vs 前一交易日 % 變化
- 跨類:total_balance + dominant_category + dominant_category_ratio(Concentration EventKind 用)
- JSONB pack:其他 25 cols(Previous/Buy/Sell/CashRedemption/Replacement/NextDayQuota × 5)

change_pct 計算需 t-1 比對,window function 在 SQL 內 LAG;Python 端不 in-memory iter。
"""

from __future__ import annotations

import logging
import time
from typing import Any

from .._common import fetch_bronze, upsert_silver


logger = logging.getLogger("collector.silver.builders.loan_collateral")


NAME          = "loan_collateral"
SILVER_TABLE  = "loan_collateral_balance_derived"
BRONZE_TABLES = ["loan_collateral_balance_tw"]


# 5 大類(對應 Bronze schema {category}_current_day_balance)
CATEGORIES = [
    ("margin", "margin"),
    ("firm_loan", "firm_loan"),
    ("unrestricted_loan", "unrestricted_loan"),
    ("finance_loan", "finance_loan"),
    ("settlement_margin", "settlement_margin"),
]

# JSONB pack 細項:每類 7 sub-fields 除了 current_day_balance(其他 6 + NextDayQuota = 7)
SUB_FIELDS = [
    "previous_day_balance", "buy", "sell", "cash_redemption",
    "replacement", "next_day_quota",
]


def _build_silver_rows(bronze_rows: list[dict[str, Any]]) -> list[dict[str, Any]]:
    """Bronze → Silver row 轉換。change_pct 計算在 _enrich_change_pct 中(後 group by 算)。"""
    out: list[dict[str, Any]] = []
    for row in bronze_rows:
        s: dict[str, Any] = {
            "market":   row.get("market"),
            "stock_id": row.get("stock_id"),
            "date":     row.get("date"),
        }
        # 5 主欄(_current_balance 從 Bronze _current_day_balance 取)
        balances = {}
        for silver_cat, bronze_cat in CATEGORIES:
            col = f"{bronze_cat}_current_day_balance"
            silver_col = f"{silver_cat}_current_balance"
            val = row.get(col)
            s[silver_col] = val
            balances[silver_cat] = (val or 0)

        # 跨類聚合
        total = sum(balances.values())
        s["total_balance"] = total
        if total > 0:
            dominant = max(balances.items(), key=lambda kv: kv[1])
            s["dominant_category"] = dominant[0]
            s["dominant_category_ratio"] = dominant[1] / total
        else:
            s["dominant_category"] = None
            s["dominant_category_ratio"] = None

        # change_pct 預填 None,_enrich_change_pct 內二次掃描補
        for silver_cat, _ in CATEGORIES:
            s[f"{silver_cat}_change_pct"] = None

        # JSONB detail pack 其他 25 cols + ratios per category(占合計 %)
        detail = {}
        for silver_cat, bronze_cat in CATEGORIES:
            cat_detail = {}
            for sub in SUB_FIELDS:
                col = f"{bronze_cat}_{sub}"
                cat_detail[sub] = row.get(col)
            cat_detail["ratio"] = balances[silver_cat] / total if total > 0 else None
            detail[silver_cat] = cat_detail
        s["detail"] = detail

        out.append(s)
    return out


def _enrich_change_pct(silver_rows: list[dict[str, Any]]) -> None:
    """Per-stock 排序後算 change_pct vs t-1。in-place 修改 silver_rows。"""
    # Group by stock_id,sort by date,計算 change_pct
    from collections import defaultdict
    by_stock: dict[str, list[dict[str, Any]]] = defaultdict(list)
    for r in silver_rows:
        by_stock[r["stock_id"]].append(r)

    for sid, rows in by_stock.items():
        rows.sort(key=lambda r: r["date"])
        for i in range(1, len(rows)):
            cur, prev = rows[i], rows[i - 1]
            for silver_cat, _ in CATEGORIES:
                cur_bal = cur[f"{silver_cat}_current_balance"]
                prev_bal = prev[f"{silver_cat}_current_balance"]
                if cur_bal is not None and prev_bal is not None and prev_bal != 0:
                    cur[f"{silver_cat}_change_pct"] = (cur_bal - prev_bal) / prev_bal * 100.0


def run(
    db: Any,
    stock_ids: list[str] | None = None,
    full_rebuild: bool = False,
) -> dict[str, Any]:
    start = time.monotonic()

    bronze = fetch_bronze(db, "loan_collateral_balance_tw", stock_ids=stock_ids)
    silver = _build_silver_rows(bronze)
    _enrich_change_pct(silver)
    written = upsert_silver(
        db, SILVER_TABLE, silver,
        pk_cols=["market", "stock_id", "date"],
    )

    elapsed_ms = int((time.monotonic() - start) * 1000)
    logger.info(f"[{NAME}] read={len(bronze)} → wrote={written}({elapsed_ms}ms)")
    return {
        "name":         NAME,
        "rows_read":    len(bronze),
        "rows_written": written,
        "elapsed_ms":   elapsed_ms,
    }
