"""
silver/builders/monthly_revenue.py
==================================
monthly_revenue (Bronze) → monthly_revenue_derived (Silver)。

⚠️ v4.28+(2026-05-27) Bug A 修法:`revenue_yoy` / `revenue_mom` 改從
raw `revenue` 計算,**不再 rename FinMind Bronze 欄**。

歷史背景:PR #18.5 落地時 Silver builder 假設 Bronze `revenue_year` 是 YoY%
百分比,直接 rename revenue_year → revenue_yoy。但 FinMind
`TaiwanStockMonthRevenue` 的 `revenue_year` / `revenue_month` 實際上是
**該筆營收的日曆年份 / 月份**(e.g. 2026.0 / 4.0),非 YoY% / MoM% 百分比。
此 dataset 不提供 YoY% / MoM%,需自行從 `revenue` 跨月計算。

證據(v4.28+ 確認):
- `src/forecast/fundamental_forecast.py:96-128` `_compute_yoy_3m_avg` 把
  `revenue_year` / `revenue_month` 當 calendar tuple 用(`by_period[(y, m)]
  = rev`,`base_key = (y - 1, m)`),然後從 `revenue` 自行算 YoY
- M8 v4.24 production verify(8-stock 24/24 wins)反證 — 若 revenue_year
  是 YoY%,fundamental_forecast_core 永遠 match 不到 base_key,fusion 不會
  出 fundamental_cqr 訊號;but it does work

Bronze schema(不動):
- `revenue_year`  NUMERIC(10, 4) — calendar year(e.g. 2026.0)
- `revenue_month` NUMERIC(10, 4) — calendar month 1-12
- `revenue`       NUMERIC        — 本月營收(千元)
- `country`       TEXT
- `create_time`   TEXT(FinMind 對某些 row 回 "" 不是 NULL,PR #18.5 hotfix)

Silver:
- `revenue_yoy = (rev - rev_prev_year_same_month) / rev_prev_year_same_month * 100`
- `revenue_mom = (rev - rev_prev_month) / rev_prev_month * 100`
- 無 base(去年同月 / 前一月不存在 / base = 0)→ NULL
- detail JSONB:country / create_time

⚠️ Bronze fetch 範圍:YoY 計算需 13+ 月 history,**bypass v4.15 incremental
180-day READ window**(monthly_revenue 體量小:12 月/股 × 1300 股 × 7 yr ≈
110k rows,全 fetch < 1s)。對齊 cores_overview §四「per-builder 自治」原則,
單 builder bypass 既有 window 不影響其他 13 個 Silver builders。
"""

from __future__ import annotations

import logging
import time
from typing import Any

from .._common import upsert_silver


logger = logging.getLogger("collector.silver.builders.monthly_revenue")


NAME          = "monthly_revenue"
SILVER_TABLE  = "monthly_revenue_derived"
BRONZE_TABLES = ["monthly_revenue"]

DETAIL_KEYS = ("country", "create_time")


def _fetch_all_bronze_rows(
    db: Any, stock_ids: list[str] | None = None,
) -> list[dict[str, Any]]:
    """Bypass v4.15 incremental window — YoY/MoM 計算需 13+ 月 history。

    monthly_revenue 體量小(每股 12 月/年 × 7 yr × ~1300 股 ≈ 110k rows total)
    全 fetch < 1s,wall time 對 phase 7a 影響微乎其微。
    """
    sql = """
        SELECT market, stock_id, date, revenue, revenue_year, revenue_month,
               country, create_time
          FROM monthly_revenue
    """
    params: list[Any] | None = None
    if stock_ids:
        placeholders = ",".join(["%s"] * len(stock_ids))
        sql += f" WHERE stock_id IN ({placeholders})"
        params = list(stock_ids)
    sql += " ORDER BY market, stock_id, date"
    return db.query(sql, params)


def _build_period_index(
    bronze_rows: list[dict[str, Any]],
) -> dict[tuple[Any, Any, int, int], float]:
    """Build {(market, stock_id, year, month) → revenue} index for YoY/MoM lookup.

    `revenue` 必正(<= 0 視為髒資料 skip,避免除零 / 反向 percentage)。
    """
    index: dict[tuple[Any, Any, int, int], float] = {}
    for row in bronze_rows:
        market = row.get("market")
        stock_id = row.get("stock_id")
        y = row.get("revenue_year")
        m = row.get("revenue_month")
        rev = row.get("revenue")
        if stock_id is None or y is None or m is None or rev is None:
            continue
        try:
            y_i = int(y)
            m_i = int(m)
            rev_f = float(rev)
        except (TypeError, ValueError):
            continue
        if rev_f <= 0:
            continue
        index[(market, stock_id, y_i, m_i)] = rev_f
    return index


def _compute_yoy_mom(
    row: dict[str, Any],
    period_index: dict[tuple[Any, Any, int, int], float],
) -> tuple[float | None, float | None]:
    """Compute (revenue_yoy, revenue_mom) percentages from raw revenue cross-month.

    對齊 src/forecast/fundamental_forecast.py:_compute_yoy_3m_avg 的 (year, month)
    indexing pattern,但本 builder 算「決定論」per-row YoY/MoM,非平均。

    Returns (yoy, mom) in percentage units (e.g. -8.34 / +25.67);
    無 base → None(避免除零 / 反向 percentage)。
    """
    market = row.get("market")
    stock_id = row.get("stock_id")
    y = row.get("revenue_year")
    m = row.get("revenue_month")
    rev = row.get("revenue")
    if rev is None or y is None or m is None:
        return None, None
    try:
        y_i = int(y)
        m_i = int(m)
        rev_f = float(rev)
    except (TypeError, ValueError):
        return None, None
    if rev_f <= 0:
        return None, None

    # YoY: same month, prior year
    base_yoy = period_index.get((market, stock_id, y_i - 1, m_i))
    yoy: float | None = None
    if base_yoy is not None and base_yoy > 0:
        yoy = round((rev_f - base_yoy) / base_yoy * 100, 4)

    # MoM: prior month(wrap-around: Jan-N → Dec-(N-1))
    if m_i > 1:
        prev_y, prev_m = y_i, m_i - 1
    else:
        prev_y, prev_m = y_i - 1, 12
    base_mom = period_index.get((market, stock_id, prev_y, prev_m))
    mom: float | None = None
    if base_mom is not None and base_mom > 0:
        mom = round((rev_f - base_mom) / base_mom * 100, 4)

    return yoy, mom


def _build_silver_rows(bronze_rows: list[dict[str, Any]]) -> list[dict[str, Any]]:
    period_index = _build_period_index(bronze_rows)
    out: list[dict[str, Any]] = []
    for row in bronze_rows:
        yoy, mom = _compute_yoy_mom(row, period_index)
        out.append({
            "market":      row.get("market"),
            "stock_id":    row.get("stock_id"),
            "date":        row.get("date"),
            "revenue":     row.get("revenue"),
            "revenue_yoy": yoy,
            "revenue_mom": mom,
            "detail":      {k: row.get(k) for k in DETAIL_KEYS},
        })
    return out


def run(
    db: Any,
    stock_ids: list[str] | None = None,
    full_rebuild: bool = False,
) -> dict[str, Any]:
    start = time.monotonic()

    # v4.28+:bypass v4.15 incremental 180d READ window — YoY 需 13+ 月 history。
    # monthly_revenue 體量小,全 fetch < 1s。
    bronze = _fetch_all_bronze_rows(db, stock_ids=stock_ids)
    silver = _build_silver_rows(bronze)
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
