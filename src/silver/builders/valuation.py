"""
silver/builders/valuation.py
============================
valuation_per_tw + price_daily + foreign_investor_share_tw (Bronze) →
                                  valuation_daily_derived (Silver)。

PR #19b 階段:3 stored cols 1:1,market_value_weight = NULL。
PR #21-A:加 market_value_weight 計算(per spec §2.6.4)。

market_value_weight =
    (price_daily.close × foreign_investor_share_tw.total_issued)
    / SUM_over_all_stocks(close × total_issued)  by (market, date)

denominator 取 *全市場* 加總(不受 --stocks 過濾影響),這樣 weight 對 partial
backfill 仍正確。三表 INNER JOIN — 沒同股同日 close × total_issued 的 stock
不進分母也不進分子(對齊「該股那天沒交易 / 沒外資揭露 → 不算市值」)。

⚠️ Dev env caveat:分母是「`valuation_per_tw` 內所有 stock × close × total_issued」
聚合。若 dev 機只反推填了少數股票(e.g. user verify 場景:`valuation_per_tw`
8881 row / 1776 date ≈ 5 stocks),分母只算這 5 檔,2330 算出 ~99.5% 是「在
這 5 檔裡的比重」,不是「全市場 1700+ 檔的比重」。production 全市場 backfill
後 2330 應該是 ~25-30%。

- 3 stored cols(1:1):per / pbr / dividend_yield
- 無 detail JSONB
- market_value_weight:NUMERIC(10, 6),範圍 [0, 1]

Round-trip:Silver 3 stored 應與 v2.0 valuation_daily 等值;market_value_weight
是 PR #21-A 新增,不在 v2.0 legacy 比對範圍(verifier skip)。
"""

from __future__ import annotations

import logging
import time
from typing import Any

from .._common import upsert_silver


logger = logging.getLogger("collector.silver.builders.valuation")


NAME          = "valuation"
SILVER_TABLE  = "valuation_daily_derived"
BRONZE_TABLES = ["valuation_per_tw", "price_daily", "foreign_investor_share_tw"]

STORED_COLS = ("per", "pbr", "dividend_yield")


def _fetch_market_totals(db: Any) -> dict[tuple[str, Any], float]:
    """SELECT (market, date) → SUM(close × total_issued)。

    INNER JOIN 三張 — 該股那天沒 close 或沒 total_issued 都不進分母。
    永遠不傳 stock_ids 過濾(分母必須是全市場)。
    """
    sql = (
        "SELECT v.market, v.date, "
        "       SUM(pd.close * fis.total_issued) AS total_mv "
        "FROM valuation_per_tw v "
        "JOIN price_daily pd "
        "  ON v.market = pd.market AND v.stock_id = pd.stock_id AND v.date = pd.date "
        "JOIN foreign_investor_share_tw fis "
        "  ON v.market = fis.market AND v.stock_id = fis.stock_id AND v.date = fis.date "
        "GROUP BY v.market, v.date"
    )
    rows = db.query(sql)
    return {(r["market"], r["date"]): r["total_mv"] for r in rows}


def _fetch_per_stock_rows(
    db: Any, stock_ids: list[str] | None,
) -> list[dict[str, Any]]:
    """SELECT 每股每日的 valuation 三欄 + 市值(close × total_issued,LEFT JOIN
    讓 stock 不在 price_daily / foreign_investor_share_tw 的也保留 row,
    但 mv 為 NULL → weight 為 NULL)。
    """
    where = ""
    params: list[Any] = []
    if stock_ids:
        placeholders = ",".join(["%s"] * len(stock_ids))
        where = f"WHERE v.stock_id IN ({placeholders})"
        params = list(stock_ids)
    sql = (
        "SELECT v.market, v.stock_id, v.date, "
        "       v.per, v.pbr, v.dividend_yield, "
        "       (pd.close * fis.total_issued) AS mv "
        "FROM valuation_per_tw v "
        "LEFT JOIN price_daily pd "
        "  ON v.market = pd.market AND v.stock_id = pd.stock_id AND v.date = pd.date "
        "LEFT JOIN foreign_investor_share_tw fis "
        "  ON v.market = fis.market AND v.stock_id = fis.stock_id AND v.date = fis.date "
        f"{where} "
        "ORDER BY v.market, v.stock_id, v.date"
    )
    return db.query(sql, params if params else None)


def _build_silver_rows(
    per_stock: list[dict[str, Any]],
    market_totals: dict[tuple[str, Any], float],
) -> list[dict[str, Any]]:
    out: list[dict[str, Any]] = []
    for r in per_stock:
        s: dict[str, Any] = {
            "market":   r["market"],
            "stock_id": r["stock_id"],
            "date":     r["date"],
        }
        for c in STORED_COLS:
            s[c] = r.get(c)

        weight: float | None = None
        mv = r.get("mv")
        if mv is not None:
            total = market_totals.get((r["market"], r["date"]))
            if total is not None and float(total) > 0:
                weight = float(mv) / float(total)
        s["market_value_weight"] = weight
        out.append(s)
    return out


def run(
    db: Any,
    stock_ids: list[str] | None = None,
    full_rebuild: bool = False,
) -> dict[str, Any]:
    start = time.monotonic()

    market_totals = _fetch_market_totals(db)
    per_stock     = _fetch_per_stock_rows(db, stock_ids)
    silver        = _build_silver_rows(per_stock, market_totals)
    written = upsert_silver(
        db, SILVER_TABLE, silver,
        pk_cols=["market", "stock_id", "date"],
    )

    elapsed_ms = int((time.monotonic() - start) * 1000)
    logger.info(
        f"[{NAME}] read={len(per_stock)} (market_totals={len(market_totals)}) "
        f"→ wrote={written}({elapsed_ms}ms)"
    )
    return {
        "name":         NAME,
        "rows_read":    len(per_stock),
        "rows_written": written,
        "elapsed_ms":   elapsed_ms,
    }
