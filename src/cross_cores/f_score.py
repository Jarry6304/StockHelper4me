"""
cross_cores/f_score.py
======================
Toolkit B B1:Piotroski F-Score (Piotroski 2000)。

9 個 binary 條件加總(0-9):
  Profitability(4): ROA > 0, CFO > 0, ROA YoY > 0, CFO > Net Income (accrual quality)
  Leverage(3):     LT Debt YoY decline, Current Ratio YoY increase, No new shares
  Efficiency(2):   Gross Margin YoY increase, Asset Turnover YoY increase

F-Score ≥ 7 為 strong winner(原文要求 ≥ 8,本實作放寬對齊提案 v1.1)。

對齊 magic_formula 既有 financial_statement_derived.detail JSONB key fallback chain。
若關鍵 key 缺(verification A-1 SQL 揭露),mark excluded_reason 不入 ranking。

Refs:
  - Piotroski, J. D. (2000). "Value Investing: Using Historical Financial Statement
    Information to Separate Winners from Losers." *Journal of Accounting Research* 38, 1-41.
  - Walkshäusl, C. (2020). "Piotroski's FSCORE: international evidence." *JAM*.
"""

from __future__ import annotations

import logging
import time
from typing import Any

from silver._common import upsert_silver

from cross_cores._shared import (
    assign_ranks,
    empty_row,
    fetch_latest_date,
    fetch_universe_filter,
)

logger = logging.getLogger("collector.cross_cores.f_score")

NAME            = "f_score"
OUTPUT_TABLE    = "f_score_ranked_derived"
UPSTREAM_TABLES = ["financial_statement_derived", "stock_info_ref"]

TOP_N = 30
# v3.32 hotfix(2026-05-18 production):F_SCORE_MIN 7 → 6
# 原 7 在台股 production 揭露 eligible = 18(0.6% of 3057),信號太稀疏。
# Walkshäusl (2020) JAM 國際 OOS(含台灣)證明 ≥ 6 仍有 alpha,且 cutoff 邊際
# alpha 在 5→6→7 平緩 — 收下 6 換 universe size。
F_SCORE_MIN = 6   # ≥ 6(對齊 Walkshäusl 2020 台股 OOS;原 Piotroski 2000 ≥ 8)

# detail JSONB key fallback chains(對齊 magic_formula.py 風格)
KEY_NET_INCOME    = ("本期淨利（淨損）", "本期淨利(淨損)", "本期淨利", "淨利", "NetIncome")
KEY_CFO           = ("營業活動之現金流量", "營業活動現金流量", "OperatingCashFlow", "CashFromOperations")
KEY_TOTAL_ASSETS  = ("資產總額", "資產總計", "TotalAssets")
KEY_CURRENT_ASSETS = ("流動資產", "流動資產合計", "CurrentAssets")
KEY_CURRENT_LIAB  = ("流動負債", "流動負債合計", "CurrentLiabilities")
KEY_LT_DEBT       = ("長期借款", "長期負債", "LongTermDebt", "NonCurrentLiabilities")
KEY_REVENUE       = ("營業收入合計", "營業收入", "Revenue", "OperatingRevenue")
KEY_COGS          = ("營業成本合計", "營業成本", "CostOfGoodsSold", "COGS")
KEY_SHARES        = ("普通股股本", "股本", "CommonStock", "OrdinaryShares")


def _detail_get(detail: dict, keys: tuple[str, ...]) -> float | None:
    if not detail:
        return None
    for k in keys:
        v = detail.get(k)
        if v is None:
            continue
        try:
            return float(v)
        except (TypeError, ValueError):
            continue
    return None


def _fetch_quarterly_financials(
    db: Any, end_date: Any, *, market: str = "TW",
) -> dict[str, dict[str, list[dict]]]:
    """每股 income / balance / cashflow 過去 8 季(用來算 YoY)。"""
    rows = db.query(
        """
        SELECT stock_id, date, type, detail
          FROM financial_statement_derived
         WHERE market = %s AND date <= %s
           AND type IN ('income', 'balance', 'cashflow')
           AND detail IS NOT NULL
         ORDER BY stock_id, type, date DESC
        """,
        [market, end_date],
    )
    out: dict[str, dict[str, list[dict]]] = {}
    for r in rows:
        d = out.setdefault(r["stock_id"], {"income": [], "balance": [], "cashflow": []})
        d[r["type"]].append(r)
    # cap 8 quarters per type
    for sid in out:
        for t in ("income", "balance", "cashflow"):
            out[sid][t] = out[sid][t][:8]
    return out


def _compute_f_score(financials: dict[str, list[dict]]) -> tuple[int | None, dict[str, int]]:
    """回 (f_score, dim_breakdown {profitability, leverage, efficiency})。
    若必要資料缺失 → (None, {})。
    """
    inc = financials["income"]
    bal = financials["balance"]
    cf  = financials["cashflow"]
    if len(inc) < 2 or len(bal) < 2 or len(cf) < 1:
        return None, {}

    # latest + prior(對齊 YoY)
    inc0, inc1 = inc[0], inc[1]
    bal0, bal1 = bal[0], bal[1]
    cf0 = cf[0]

    ni0 = _detail_get(inc0["detail"], KEY_NET_INCOME)
    ni1 = _detail_get(inc1["detail"], KEY_NET_INCOME)
    cfo0 = _detail_get(cf0["detail"], KEY_CFO)
    ta0 = _detail_get(bal0["detail"], KEY_TOTAL_ASSETS)
    ta1 = _detail_get(bal1["detail"], KEY_TOTAL_ASSETS)
    ca0 = _detail_get(bal0["detail"], KEY_CURRENT_ASSETS)
    cl0 = _detail_get(bal0["detail"], KEY_CURRENT_LIAB)
    ca1 = _detail_get(bal1["detail"], KEY_CURRENT_ASSETS)
    cl1 = _detail_get(bal1["detail"], KEY_CURRENT_LIAB)
    ltd0 = _detail_get(bal0["detail"], KEY_LT_DEBT)
    ltd1 = _detail_get(bal1["detail"], KEY_LT_DEBT)
    rev0 = _detail_get(inc0["detail"], KEY_REVENUE)
    rev1 = _detail_get(inc1["detail"], KEY_REVENUE)
    cogs0 = _detail_get(inc0["detail"], KEY_COGS)
    cogs1 = _detail_get(inc1["detail"], KEY_COGS)
    shr0 = _detail_get(bal0["detail"], KEY_SHARES)
    shr1 = _detail_get(bal1["detail"], KEY_SHARES)

    if any(x is None for x in [ni0, ta0, ta1, rev0, rev1]):
        return None, {}

    score = 0
    prof, lev, eff = 0, 0, 0

    # === Profitability(4)===
    roa0 = ni0 / ta0 if ta0 > 0 else None
    roa1 = ni1 / ta1 if (ni1 is not None and ta1 and ta1 > 0) else None
    if roa0 is not None and roa0 > 0: score += 1; prof += 1
    if cfo0 is not None and cfo0 > 0: score += 1; prof += 1
    if roa0 is not None and roa1 is not None and roa0 > roa1: score += 1; prof += 1
    if cfo0 is not None and ni0 is not None and cfo0 > ni0: score += 1; prof += 1

    # === Leverage / Liquidity(3)===
    if ltd0 is not None and ltd1 is not None and ltd0 < ltd1: score += 1; lev += 1
    cur0 = (ca0 / cl0) if (ca0 is not None and cl0 and cl0 > 0) else None
    cur1 = (ca1 / cl1) if (ca1 is not None and cl1 and cl1 > 0) else None
    if cur0 is not None and cur1 is not None and cur0 > cur1: score += 1; lev += 1
    if shr0 is not None and shr1 is not None and shr0 <= shr1: score += 1; lev += 1

    # === Efficiency(2)===
    gm0 = ((rev0 - cogs0) / rev0) if (cogs0 is not None and rev0 > 0) else None
    gm1 = ((rev1 - cogs1) / rev1) if (cogs1 is not None and rev1 and rev1 > 0) else None
    if gm0 is not None and gm1 is not None and gm0 > gm1: score += 1; eff += 1
    at0 = rev0 / ta0 if ta0 > 0 else None
    at1 = rev1 / ta1 if (ta1 and ta1 > 0) else None
    if at0 is not None and at1 is not None and at0 > at1: score += 1; eff += 1

    return score, {"profitability": prof, "leverage": lev, "efficiency": eff}


def run(
    db: Any,
    stock_ids: list[str] | None = None,
    full_rebuild: bool = False,
    lookback_days: int | None = None,
) -> dict[str, Any]:
    start = time.monotonic()
    target_date = fetch_latest_date(db, "price_daily_fwd")
    if target_date is None:
        return {"name": NAME, "rows_read": 0, "rows_written": 0,
                "elapsed_ms": int((time.monotonic() - start) * 1000)}

    universe = fetch_universe_filter(db)
    fins_by_stock = _fetch_quarterly_financials(db, target_date)

    rows: list[dict[str, Any]] = []
    for sid, excluded in universe.items():
        if stock_ids and sid not in stock_ids:
            continue
        if excluded is not None:
            rows.append(empty_row(sid, target_date, excluded_reason=excluded,
                                  extras={"f_score": None, "profitability": None,
                                          "leverage": None, "efficiency": None,
                                          "score_rank": None}))
            continue

        fins = fins_by_stock.get(sid)
        if not fins:
            rows.append(empty_row(sid, target_date, excluded_reason="no_financial_data",
                                  extras={"f_score": None, "profitability": None,
                                          "leverage": None, "efficiency": None,
                                          "score_rank": None}))
            continue

        score, breakdown = _compute_f_score(fins)
        if score is None:
            rows.append(empty_row(sid, target_date, excluded_reason="insufficient_fs_keys",
                                  extras={"f_score": None, "profitability": None,
                                          "leverage": None, "efficiency": None,
                                          "score_rank": None}))
            continue

        excluded_reason = None if score >= F_SCORE_MIN else "f_score_below_threshold"

        rows.append({
            "market": "TW", "stock_id": sid, "date": target_date,
            "f_score": score,
            "profitability": breakdown.get("profitability"),
            "leverage": breakdown.get("leverage"),
            "efficiency": breakdown.get("efficiency"),
            "score_rank": None,
            "universe_size": None, "is_top_n": False,
            "excluded_reason": excluded_reason,
        })

    # rank by f_score(高分好);只 rank `excluded_reason IS NULL` 的
    eligible_rows = [r for r in rows if r.get("excluded_reason") is None]
    eligible_rows.sort(key=lambda r: r["f_score"], reverse=True)
    n = len(eligible_rows)
    for i, r in enumerate(eligible_rows, 1):
        r["score_rank"] = i
        r["universe_size"] = n
    for r in eligible_rows[:TOP_N]:
        r["is_top_n"] = True

    written = upsert_silver(db, OUTPUT_TABLE, rows,
                            pk_cols=["market", "stock_id", "date"])
    elapsed_ms = int((time.monotonic() - start) * 1000)
    logger.info(f"[{NAME}] eligible={n} rows={len(rows)} written={written} ({elapsed_ms}ms)")
    return {"name": NAME, "rows_read": len(rows), "rows_written": written,
            "elapsed_ms": elapsed_ms}
