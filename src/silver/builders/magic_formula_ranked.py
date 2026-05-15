"""
silver/builders/magic_formula_ranked.py
========================================
Magic Formula(Greenblatt 2005)Bronze → Silver derived。

對齊 m3Spec/magic_formula_core.md(v3.4 plan 同 PR 加)。
公式:
  EBIT_TTM         = SUM(income.detail->>'營業利益(損失)' 過去 4 季)
  Total Assets     = balance.detail->>'資產總額'(最近一季元值)
  Total Liab       = balance.detail->>'負債總額'
  Cash             = balance.detail->>'現金及約當現金'(常見命名,fallback chain)
  Market Cap       = price_daily_fwd.close × foreign_investor_share_tw.total_issued
  EV               = Market Cap + Total Liab - Cash
  Invested Capital = Total Assets - Cash(working-capital proxy)
  EY               = EBIT_TTM / EV
  ROIC             = EBIT_TTM / Invested Capital
  combined_rank    = ey_rank + roic_rank(愈低愈好;對齊 Greenblatt 原版)

Universe:排除金融保險 + 公用事業(Greenblatt 2005 §六 "Special Industries")。
判定:stock_info_ref.industry_category 含 keyword('金融' / '保險' / '銀行'
/ '證券' / '電力' / '燃氣' / '水') → excluded_reason 紀錄。

Date semantics:每次 run 對「latest price_daily_fwd.date」算 1 個 cross-rank,
寫一 row per stock。第一次 backfill 對 last N days(預設 30)算;之後 daily
incremental 加 1 day。對齊 LLM screening tool 主要消費的是「now」+ 短期歷史。
"""

from __future__ import annotations

import logging
import time
from typing import Any

from .._common import upsert_silver


logger = logging.getLogger("collector.silver.builders.magic_formula_ranked")


NAME          = "magic_formula_ranked"
SILVER_TABLE  = "magic_formula_ranked_derived"
BRONZE_TABLES = [
    "financial_statement_derived",     # 既算過的 Silver(7b 依賴);Bronze 上游 financial_statement
    "valuation_per_tw",                # 估值(備用)
    "price_daily_fwd",                 # close 對最新交易日
    "foreign_investor_share_tw",       # total_issued(同 valuation_core market_value_weight 慣例)
    "stock_info_ref",                  # universe filter
]


# Greenblatt 2005 §六:排除金融保險 + 公用事業。
# 對齊 financial_statement_core detail key 風格(IFRS 中文 + best-guess fallback)。
EXCLUDED_KEYWORDS = ("金融", "保險", "銀行", "證券", "壽險", "電力", "燃氣", "自來水")

# 對齊 financial_statement_core EBIT key(全形括號 + 半形 fallback)
EBIT_KEYS = (
    "營業利益（損失）",   # 全形 U+FF08/FF09(實際 user 真 key)
    "營業利益(損失)",                # 半形 fallback
    "營業利益",
    "OperatingProfit",
)
TOTAL_ASSETS_KEYS = ("資產總額", "資產總計", "TotalAssets")
TOTAL_LIAB_KEYS   = ("負債總額", "負債總計", "TotalLiabilities")
CASH_KEYS = (
    "現金及約當現金",
    "現金與約當現金",
    "現金及銀行存款",
    "CashAndCashEquivalents",
)

# 預設 backfill 歷史天數(每天 cross-rank 算一次,1700 stocks × 30d = 51K rows)
DEFAULT_BACKFILL_DAYS = 30
# Magic Formula top-N(Greenblatt 2005 原版 20-30)
TOP_N = 30


def _detail_get(detail: dict, keys: tuple[str, ...]) -> float | None:
    """從 detail JSONB 走 fallback chain 取 numeric。"""
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


def _fetch_universe_filter(db: Any) -> dict[str, str | None]:
    """每股 → excluded_reason(None 表示在 universe 內)。"""
    rows = db.query(
        "SELECT stock_id, industry_category FROM stock_info_ref WHERE market = 'TW'"
    )
    out: dict[str, str | None] = {}
    for r in rows:
        ind = r.get("industry_category") or ""
        reason: str | None = None
        for kw in EXCLUDED_KEYWORDS:
            if kw in ind:
                reason = "financial" if kw in ("金融", "保險", "銀行", "證券", "壽險") else "utility"
                break
        out[r["stock_id"]] = reason
    return out


def _fetch_latest_financials(
    db: Any, stock_ids: list[str] | None,
) -> dict[str, dict[str, Any]]:
    """每股的 latest 4 quarterly income + latest 1 balance,組裝成
    {stock_id: {"ebit_ttm": NTD, "total_assets": NTD, "total_liab": NTD, "cash": NTD}}。
    detail key 走 fallback chain;命名不對齊 → 各 field 為 None,builder 判 excluded_reason。
    """
    where = ""
    params: list[Any] = []
    if stock_ids:
        ph = ",".join(["%s"] * len(stock_ids))
        where = f"AND stock_id IN ({ph})"
        params = list(stock_ids)

    # Pass 1:每股最近 4 季 income(取 EBIT)
    income_rows = db.query(
        f"""
        SELECT stock_id, date, detail
          FROM financial_statement_derived
         WHERE market = 'TW' AND type = 'income' {where}
         ORDER BY stock_id, date DESC
        """,
        params if params else None,
    )
    income_by_stock: dict[str, list[dict]] = {}
    for r in income_rows:
        income_by_stock.setdefault(r["stock_id"], []).append(r)

    # Pass 2:每股最近 1 季 balance
    balance_rows = db.query(
        f"""
        SELECT DISTINCT ON (stock_id) stock_id, date, detail
          FROM financial_statement_derived
         WHERE market = 'TW' AND type = 'balance' {where}
         ORDER BY stock_id, date DESC
        """,
        params if params else None,
    )
    balance_by_stock = {r["stock_id"]: r for r in balance_rows}

    out: dict[str, dict[str, Any]] = {}
    for stock_id, inc_list in income_by_stock.items():
        last4 = inc_list[:4]
        ebit_q_values = [_detail_get(r.get("detail") or {}, EBIT_KEYS) for r in last4]
        if any(v is None for v in ebit_q_values) or len(ebit_q_values) < 4:
            # 季數不足或 key 命名不對齊 → mark excluded
            out[stock_id] = {"excluded_reason": "no_ebit_data"}
            continue
        ebit_ttm = sum(ebit_q_values)

        bal = balance_by_stock.get(stock_id)
        if bal is None:
            out[stock_id] = {"excluded_reason": "no_balance_data"}
            continue
        bd = bal.get("detail") or {}
        total_assets = _detail_get(bd, TOTAL_ASSETS_KEYS)
        total_liab   = _detail_get(bd, TOTAL_LIAB_KEYS)
        cash         = _detail_get(bd, CASH_KEYS) or 0.0
        if total_assets is None or total_liab is None:
            out[stock_id] = {"excluded_reason": "no_balance_data"}
            continue

        out[stock_id] = {
            "ebit_ttm":     ebit_ttm,
            "total_assets": total_assets,
            "total_liab":   total_liab,
            "cash":         cash,
        }
    return out


def _fetch_market_caps_for_date(
    db: Any, target_date: Any, stock_ids: list[str] | None,
) -> dict[str, float]:
    """對特定 date,每股的市值 = close × total_issued(LEFT JOIN 對齊
    valuation_core market_value_weight pattern)。"""
    where = ""
    params: list[Any] = [target_date]
    if stock_ids:
        ph = ",".join(["%s"] * len(stock_ids))
        where = f"AND pd.stock_id IN ({ph})"
        params.extend(stock_ids)
    rows = db.query(
        f"""
        SELECT pd.stock_id, (pd.close * fis.total_issued)::float8 AS mv
          FROM price_daily_fwd pd
          LEFT JOIN foreign_investor_share_tw fis
            ON pd.market = fis.market AND pd.stock_id = fis.stock_id AND pd.date = fis.date
         WHERE pd.market = 'TW' AND pd.date = %s {where}
        """,
        params,
    )
    out: dict[str, float] = {}
    for r in rows:
        mv = r.get("mv")
        if mv is not None and float(mv) > 0:
            out[r["stock_id"]] = float(mv)
    return out


def _fetch_target_dates(
    db: Any, lookback_days: int,
) -> list[Any]:
    """回最近 lookback_days 個 distinct price_daily_fwd.date(降序)。"""
    rows = db.query(
        """
        SELECT DISTINCT date
          FROM price_daily_fwd
         WHERE market = 'TW'
         ORDER BY date DESC
         LIMIT %s
        """,
        [lookback_days],
    )
    return [r["date"] for r in rows]


def _build_rank_rows_for_date(
    target_date: Any,
    universe_filter: dict[str, str | None],
    financials: dict[str, dict[str, Any]],
    market_caps: dict[str, float],
) -> list[dict[str, Any]]:
    """對單一 date,組裝所有 stock 的 Silver rows(含 rank)。"""
    rows: list[dict[str, Any]] = []
    eligible: list[dict[str, Any]] = []   # 可進 rank 的 stocks(計算用)

    for stock_id, fin in financials.items():
        excluded = universe_filter.get(stock_id)
        row: dict[str, Any] = {
            "market":           "TW",
            "stock_id":         stock_id,
            "date":             target_date,
            "ebit_ttm":         None,
            "market_cap":       None,
            "total_debt":       None,
            "cash":             None,
            "enterprise_value": None,
            "invested_capital": None,
            "earnings_yield":   None,
            "roic":             None,
            "ey_rank":          None,
            "roic_rank":        None,
            "combined_rank":    None,
            "universe_size":    None,
            "is_top_30":        False,
            "excluded_reason":  excluded,
        }

        # 早 excluded 不算財務 metrics
        if excluded is not None:
            rows.append(row)
            continue

        if "excluded_reason" in fin:
            row["excluded_reason"] = fin["excluded_reason"]
            rows.append(row)
            continue

        mv = market_caps.get(stock_id)
        if mv is None:
            row["excluded_reason"] = "no_market_cap"
            rows.append(row)
            continue

        ebit  = fin["ebit_ttm"]
        ta    = fin["total_assets"]
        tl    = fin["total_liab"]
        cash  = fin["cash"]
        ev    = mv + tl - cash
        ic    = ta - cash

        # EBIT 為負 / EV ≤ 0 / IC ≤ 0 都 disqualify(對齊 Greenblatt 「賺錢」前提)
        if ebit <= 0 or ev <= 0 or ic <= 0:
            row.update({
                "ebit_ttm": ebit, "market_cap": mv, "total_debt": tl, "cash": cash,
                "enterprise_value": ev, "invested_capital": ic,
                "excluded_reason": "negative_ebit_or_ev",
            })
            rows.append(row)
            continue

        ey   = ebit / ev
        roic = ebit / ic
        row.update({
            "ebit_ttm": ebit, "market_cap": mv, "total_debt": tl, "cash": cash,
            "enterprise_value": ev, "invested_capital": ic,
            "earnings_yield": ey, "roic": roic,
        })
        eligible.append(row)
        rows.append(row)

    # Cross-stock rank within universe
    n = len(eligible)
    # ey 高 → rank 低(rank 1 = highest EY,愈低愈好);ROIC 同
    by_ey   = sorted(eligible, key=lambda r: -r["earnings_yield"])
    by_roic = sorted(eligible, key=lambda r: -r["roic"])
    for i, r in enumerate(by_ey, 1):
        r["ey_rank"] = i
    for i, r in enumerate(by_roic, 1):
        r["roic_rank"] = i
    for r in eligible:
        r["combined_rank"] = r["ey_rank"] + r["roic_rank"]
        r["universe_size"] = n

    # Top N 判定:依 combined_rank 升序;若有 tie 都記 is_top_30(rank ≤ 30)
    eligible.sort(key=lambda r: r["combined_rank"])
    for i, r in enumerate(eligible[:TOP_N]):
        r["is_top_30"] = True

    return rows


def run(
    db: Any,
    stock_ids: list[str] | None = None,
    full_rebuild: bool = False,
    lookback_days: int = DEFAULT_BACKFILL_DAYS,
) -> dict[str, Any]:
    """跑 Magic Formula cross-stock ranking。

    Args:
        db:             DBWriter
        stock_ids:      None = 全市場;否則只算指定 stocks(注意:cross-rank 仍對
                        whole universe 內既有 financials 進行,只是輸出 row 限縮)
        full_rebuild:   True = 重算 lookback_days 全部 dates;False = 只算 latest date
        lookback_days:  full_rebuild 時往回幾天(預設 30)
    """
    start = time.monotonic()

    universe_filter = _fetch_universe_filter(db)
    # financials 對齊「最近 latest 4 季 income + 1 季 balance」— 對所有 universe 拉
    # (不受 stock_ids 過濾,因為 cross-rank 需要 universe 完整)
    financials = _fetch_latest_financials(db, stock_ids=None)

    target_dates = _fetch_target_dates(
        db, lookback_days=lookback_days if full_rebuild else 1
    )
    if not target_dates:
        logger.warning(f"[{NAME}] price_daily_fwd 為空,跳過 ranking")
        return {"name": NAME, "rows_read": 0, "rows_written": 0,
                "elapsed_ms": int((time.monotonic() - start) * 1000)}

    all_rows: list[dict[str, Any]] = []
    for d in target_dates:
        market_caps = _fetch_market_caps_for_date(db, d, stock_ids=None)
        per_date_rows = _build_rank_rows_for_date(
            d, universe_filter, financials, market_caps
        )
        # 若 caller 傳 stock_ids,只 emit 那些 stocks 的 row(rank 已對 whole universe 算)
        if stock_ids:
            sid_set = set(stock_ids)
            per_date_rows = [r for r in per_date_rows if r["stock_id"] in sid_set]
        all_rows.extend(per_date_rows)

    written = upsert_silver(
        db, SILVER_TABLE, all_rows,
        pk_cols=["market", "stock_id", "date"],
    )

    elapsed_ms = int((time.monotonic() - start) * 1000)
    logger.info(
        f"[{NAME}] dates={len(target_dates)} eligible_universe~={sum(1 for v in financials.values() if 'excluded_reason' not in v)} "
        f"rows={len(all_rows)} written={written} ({elapsed_ms}ms)"
    )
    return {
        "name":         NAME,
        "rows_read":    len(all_rows),
        "rows_written": written,
        "elapsed_ms":   elapsed_ms,
    }
