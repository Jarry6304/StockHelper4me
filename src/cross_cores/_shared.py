"""
cross_cores/_shared.py
======================
v3.32 共用 helper:universe filter / top-N rank / latest trading date 等。

對齊 magic_formula.py 既有實作風格(`EXCLUDED_KEYWORDS` / `_fetch_universe_filter`),
10 個新 builder 共用此 module 避免重複。
"""

from __future__ import annotations

import math
from typing import Any


# Greenblatt 2005 §六:排除金融保險 + 公用事業(對齊 magic_formula.EXCLUDED_KEYWORDS)
EXCLUDED_KEYWORDS = ("金融", "保險", "銀行", "證券", "壽險", "電力", "燃氣", "自來水")


def fetch_universe_filter(db: Any, *, market: str = "TW") -> dict[str, str | None]:
    """每股 → excluded_reason(None 表示在 universe 內)。

    對齊 magic_formula._fetch_universe_filter,supports survivorship-aware
    filter via `delisting_date IS NULL`。
    """
    rows = db.query(
        """
        SELECT stock_id, industry_category, delisting_date
          FROM stock_info_ref
         WHERE market = %s
        """,
        [market],
    )
    out: dict[str, str | None] = {}
    for r in rows:
        # 已下市 → 排除(防 survivorship bias 反向:避免 stale 已下市 stock 入 universe)
        if r.get("delisting_date") is not None:
            out[r["stock_id"]] = "delisted"
            continue
        ind = r.get("industry_category") or ""
        reason: str | None = None
        for kw in EXCLUDED_KEYWORDS:
            if kw in ind:
                reason = "financial" if kw in ("金融", "保險", "銀行", "證券", "壽險") else "utility"
                break
        out[r["stock_id"]] = reason
    return out


def fetch_latest_date(db: Any, table: str, *, market: str = "TW") -> Any | None:
    """回給定表的最新 date(market filter)。"""
    rows = db.query(
        f"SELECT MAX(date) AS d FROM {table} WHERE market = %s",
        [market],
    )
    if not rows:
        return None
    return rows[0].get("d")


def fetch_target_dates(
    db: Any, table: str, *, market: str = "TW", limit: int = 30,
) -> list[Any]:
    """回最近 N 個 distinct date(降序)。對齊 magic_formula._fetch_target_dates。"""
    rows = db.query(
        f"""
        SELECT DISTINCT date FROM {table}
         WHERE market = %s
         ORDER BY date DESC
         LIMIT %s
        """,
        [market, limit],
    )
    return [r["date"] for r in rows]


def assign_ranks(
    rows: list[dict[str, Any]],
    *,
    rank_col: str,
    metric_col: str,
    reverse: bool = True,
    top_n: int = 30,
    is_top_col: str = "is_top_n",
) -> None:
    """對 eligible rows 加 rank(1 = best)+ is_top_n 標記。In-place 修改。

    Args:
        rank_col:  寫入的 rank 欄名(e.g. "momentum_rank")
        metric_col: 排序依據欄名(e.g. "return_6m")
        reverse:   True = 高值好(rank 1 = 最高);False = 低值好(rank 1 = 最低)
        top_n:     top-N 標記閾值
        is_top_col: 寫入 is_top_n 的欄名
    """
    eligible = [r for r in rows if r.get(metric_col) is not None]
    eligible.sort(key=lambda r: r[metric_col], reverse=reverse)
    n = len(eligible)
    for i, r in enumerate(eligible, 1):
        r[rank_col] = i
        r["universe_size"] = n
    for r in eligible[:top_n]:
        r[is_top_col] = True


def compute_std(values: list[float]) -> float | None:
    """sample std(N-1)— None if < 2 values。"""
    valid = [v for v in values if v is not None and not math.isnan(v)]
    n = len(valid)
    if n < 2:
        return None
    mean = sum(valid) / n
    var = sum((v - mean) ** 2 for v in valid) / (n - 1)
    return math.sqrt(var)


def compute_returns_from_closes(closes: list[float]) -> list[float]:
    """連續 log returns(本實作用 simple pct returns 對齊 Ang 2009 標準慣例)。"""
    out: list[float] = []
    for i in range(1, len(closes)):
        prev = closes[i - 1]
        cur = closes[i]
        if prev and prev > 0:
            out.append((cur - prev) / prev)
    return out


def fetch_close_series(
    db: Any, *, stock_id: str, end_date: Any, lookback_days: int, market: str = "TW",
) -> list[dict[str, Any]]:
    """從 price_daily_fwd 撈某股最近 N 日(降序),每 row 含 date / close。"""
    rows = db.query(
        """
        SELECT date, close::float8 AS close
          FROM price_daily_fwd
         WHERE market = %s AND stock_id = %s AND date <= %s
         ORDER BY date DESC
         LIMIT %s
        """,
        [market, stock_id, end_date, lookback_days],
    )
    return rows


def empty_row(stock_id: str, date: Any, *, excluded_reason: str | None = None,
              extras: dict[str, Any] | None = None) -> dict[str, Any]:
    """組裝 ranked 表 base row(metric 欄為 None)。"""
    row: dict[str, Any] = {
        "market": "TW",
        "stock_id": stock_id,
        "date": date,
        "universe_size": None,
        "is_top_n": False,
        "excluded_reason": excluded_reason,
    }
    if extras:
        row.update(extras)
    return row
