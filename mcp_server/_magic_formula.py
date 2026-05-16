"""Tool 4 內部演算法:`magic_formula_screen` — Greenblatt 2005 Top-N 篩選。

對齊 v3.4 plan §Phase C(2026-05-15)。

設計:
- 跨股 query → 不走 agg.as_of()(它是 per-stock)
- 直接 SELECT magic_formula_ranked_derived 撈 latest date 的 top N
- JOIN stock_info_ref 加公司名 + industry_category
- payload ~ 5 KB / ~1250 tokens(對齊 toolkit v2 ≤ 5K tokens 目標)

呼叫端:`mcp_server.tools.data.magic_formula_screen()`。

Reference:Greenblatt, J. (2005). *The Little Book That Beats the Market*. Wiley.
"""

from __future__ import annotations

from datetime import date
from typing import Any

from agg._db import (
    fetch_cross_stock_ranked,
    get_connection,
)  # v3.5 R5 C12+C13:connection single entry + cross-stock helper


def compute_magic_formula_screen(
    as_of: date,
    *,
    top_n: int = 30,
    database_url: str | None = None,
) -> dict[str, Any]:
    """跑 Magic Formula top-N 篩選。

    v3.5 R5 C13:cross-stock 邏輯走 `agg._db.fetch_cross_stock_ranked` 通用 helper;
    universe_size + stats 仍是 magic_formula 特有的 percentile 統計,留在本 file。

    Args:
        as_of:        查詢日(回 ≤ as_of 的最新 ranking date 之 top N)
        top_n:        取 top N(預設 30,對齊 Greenblatt 2005 原版)
        database_url: 可選 PG 連線字串

    Returns:
        dict 結構:
          {
            "as_of": ISO date,
            "ranking_date": "2026-05-15",      # 實際 ranking 來自的最新日(可能 ≠ as_of)
            "universe_size": 1432,
            "top_n": 30,
            "top_stocks": [
              {"rank": 1, "stock_id": "2330", "name": "...", "industry": "...",
               "earnings_yield": 0.082, "roic": 0.31,
               "ey_rank": 145, "roic_rank": 12, "combined_rank": 157},
              ...
            ],
            "stats": {"median_ey": ..., "median_roic": ...,
                      "min_combined_rank": ..., "max_combined_rank_in_top_n": ...},
            "narrative": "..."
          }
    """
    conn = get_connection(database_url)
    try:
        # 1+3. 用通用 helper 拉 latest ranking_date + top N rows(LEFT JOIN stock_info_ref)
        ranking_date, rows = fetch_cross_stock_ranked(
            conn,
            source_table="magic_formula_ranked_derived",
            as_of=as_of,
            top_n=top_n,
            rank_col="combined_rank",
            is_top_col="is_top_30",
        )
        if ranking_date is None:
            return _empty_result(as_of, top_n,
                reason="no_magic_formula_data <= as_of")

        # 2. universe_size + stats(median EY / ROIC for eligible stocks) — MF 特有
        with conn.cursor() as cur:
            cur.execute(
                """
                SELECT
                    universe_size,
                    percentile_cont(0.5) WITHIN GROUP (ORDER BY earnings_yield) AS median_ey,
                    percentile_cont(0.5) WITHIN GROUP (ORDER BY roic) AS median_roic
                FROM magic_formula_ranked_derived
                 WHERE market = 'TW' AND date = %s AND excluded_reason IS NULL
                 GROUP BY universe_size
                """,
                [ranking_date],
            )
            stats_row = cur.fetchone() or {}
        universe_size = stats_row.get("universe_size", 0)
        median_ey   = float(stats_row.get("median_ey") or 0.0)
        median_roic = float(stats_row.get("median_roic") or 0.0)
    finally:
        conn.close()

    top_stocks: list[dict[str, Any]] = []
    for i, r in enumerate(rows, 1):
        top_stocks.append({
            "rank": i,
            "stock_id":       r["stock_id"],
            "name":           r.get("stock_name") or "",
            "industry":       r.get("industry_category") or "",
            "earnings_yield": _round(r["earnings_yield"], 4),
            "roic":           _round(r["roic"], 4),
            "ey_rank":        r["ey_rank"],
            "roic_rank":      r["roic_rank"],
            "combined_rank":  r["combined_rank"],
        })

    min_combined  = top_stocks[0]["combined_rank"]  if top_stocks else None
    max_combined  = top_stocks[-1]["combined_rank"] if top_stocks else None

    return {
        "as_of":         as_of.isoformat(),
        "ranking_date":  ranking_date.isoformat(),
        "universe_size": universe_size,
        "top_n":         top_n,
        "top_stocks":    top_stocks,
        "stats": {
            "median_ey":                  _round(median_ey, 4),
            "median_roic":                _round(median_roic, 4),
            "min_combined_rank":          min_combined,
            "max_combined_rank_in_top_n": max_combined,
        },
        "narrative": _compose_narrative(
            top_stocks=top_stocks, universe_size=universe_size,
            ranking_date=ranking_date, as_of=as_of,
        ),
    }


def _empty_result(as_of: date, top_n: int, *, reason: str) -> dict[str, Any]:
    return {
        "as_of":         as_of.isoformat(),
        "ranking_date":  None,
        "universe_size": 0,
        "top_n":         top_n,
        "top_stocks":    [],
        "stats":         {"median_ey": 0.0, "median_roic": 0.0,
                          "min_combined_rank": None, "max_combined_rank_in_top_n": None},
        "narrative":     f"Magic Formula 資料缺失({reason});請確認 silver builder "
                         f"magic_formula_ranked 已對 as_of {as_of.isoformat()} 之前的 "
                         f"date 跑過。",
    }


def _round(v: float | None, digits: int) -> float | None:
    if v is None:
        return None
    return round(float(v), digits)


def _compose_narrative(
    *, top_stocks: list[dict[str, Any]], universe_size: int,
    ranking_date: date, as_of: date,
) -> str:
    """1 句敘述,~150 chars。"""
    if not top_stocks:
        return f"當日({as_of.isoformat()})無 Magic Formula top stocks 可回。"

    # 列出 top 3 名稱(若有 stock_name)
    top3_names = []
    for s in top_stocks[:3]:
        label = s.get("name") or s["stock_id"]
        top3_names.append(f"{s['stock_id']}({label})" if s.get("name") else s["stock_id"])

    age_days = (as_of - ranking_date).days
    age_str = "今日" if age_days == 0 else f"{age_days} 天前更新"

    return (
        f"Greenblatt 2005 Magic Formula:在排除金融 + 公用後的 {universe_size} 檔股票"
        f"({age_str})中,top {len(top_stocks)} 由 EBIT/EV + EBIT/IC 雙排名 combined "
        f"rank 篩出 ({', '.join(top3_names)} 居首)。"
    )
