"""
cross_cores/persistent_momentum.py
==================================
Toolkit A A1:Persistent Momentum (Chen-Chou-Hsieh 2023)。

過去 6M(~126 日)期間連續 ≥ 2M(~42 日)位於 top decile,skip 1M(~21 日),
持有 6M。同時計算 6M cumulative return rank。

Vol-managed overlay(Barroso-Santa-Clara 2015):若 6M 內 realized vol > 歷史
均值 × 1.5,detail.vol_managed_scale = 0.5;否則 1.0。

Refs:
  - Chen, H.-Y., Chou, P.-H., & Hsieh, C.-H. (2023). "Revisiting the momentum
    effect in Taiwan: The role of persistency." *Journal of Financial Markets*.
  - Barroso, P., & Santa-Clara, P. (2015). "Momentum has its moments." *JFE* 116, 111-120.
"""

from __future__ import annotations

import logging
import time
from typing import Any

from silver._common import upsert_silver

from cross_cores._shared import (
    assign_ranks,
    compute_returns_from_closes,
    compute_std,
    empty_row,
    fetch_close_series,
    fetch_latest_date,
    fetch_universe_filter,
)

logger = logging.getLogger("collector.cross_cores.persistent_momentum")

NAME            = "persistent_momentum"
OUTPUT_TABLE    = "persistent_momentum_ranked_derived"
UPSTREAM_TABLES = ["price_daily_fwd", "stock_info_ref"]

WINDOW_6M  = 126
WINDOW_12M = 252
WINDOW_1M  = 21
TOP_N = 30

# Barroso-Santa-Clara 2015 vol-managed threshold
VOL_MANAGED_THRESHOLD = 1.5    # 6M vol > 歷史均值 × 1.5 → scale 50%


def _persistent_months_count(closes: list[float]) -> int:
    """簡化版:6M 中每月底 close 對 6M 前 close ratio,持續 ≥ 0 的月數。
    closes:desc 排序最新→舊,需 ≥ WINDOW_6M。
    """
    # 切 6 個月 chunk:每 21 日 1 月,共 6 chunk
    months_positive = 0
    for m in range(6):
        end_idx = m * 21
        start_idx = end_idx + 21
        if start_idx >= len(closes):
            break
        end_close = closes[end_idx]
        start_close = closes[start_idx]
        if start_close and start_close > 0 and end_close > start_close:
            months_positive += 1
    return months_positive


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
    rows: list[dict[str, Any]] = []
    all_6m_vols: list[float] = []
    pending: list[tuple[dict[str, Any], float]] = []   # (row, 6m_vol)

    for sid, excluded in universe.items():
        if stock_ids and sid not in stock_ids:
            continue
        if excluded is not None:
            rows.append(empty_row(sid, target_date, excluded_reason=excluded,
                                  extras={"return_6m": None, "return_12m_1m": None,
                                          "persistent_months": None, "momentum_rank": None}))
            continue

        closes_rows = fetch_close_series(
            db, stock_id=sid, end_date=target_date, lookback_days=WINDOW_12M + 5,
        )
        if len(closes_rows) < WINDOW_6M + 5:
            rows.append(empty_row(sid, target_date, excluded_reason="insufficient_history",
                                  extras={"return_6m": None, "return_12m_1m": None,
                                          "persistent_months": None, "momentum_rank": None}))
            continue

        # closes desc 排序;closes[0] = 最新
        closes = [r["close"] for r in closes_rows if r.get("close") is not None]

        # 6M return
        try:
            cur = closes[0]
            past_6m = closes[WINDOW_6M - 1] if len(closes) >= WINDOW_6M else None
            return_6m = (cur - past_6m) / past_6m if past_6m and past_6m > 0 else None
        except (IndexError, TypeError, ZeroDivisionError):
            return_6m = None

        # 12-1 return
        try:
            close_skip1m = closes[WINDOW_1M] if len(closes) > WINDOW_1M else None
            close_12m = closes[WINDOW_12M - 1] if len(closes) >= WINDOW_12M else None
            return_12m_1m = (close_skip1m - close_12m) / close_12m \
                if close_12m and close_12m > 0 and close_skip1m is not None else None
        except (IndexError, TypeError, ZeroDivisionError):
            return_12m_1m = None

        persistent_months = _persistent_months_count(closes)

        # 6M realized vol(for vol-managed overlay)
        returns_6m = compute_returns_from_closes(list(reversed(closes[:WINDOW_6M + 1])))
        vol_6m = compute_std(returns_6m) or 0.0
        if vol_6m > 0:
            all_6m_vols.append(vol_6m)

        row = {
            "market": "TW", "stock_id": sid, "date": target_date,
            "return_6m": return_6m, "return_12m_1m": return_12m_1m,
            "persistent_months": persistent_months,
            "momentum_rank": None,
            "universe_size": None, "is_top_n": False, "excluded_reason": None,
        }
        pending.append((row, vol_6m))

    # Barroso-Santa-Clara overlay:對 cross-section 算 vol 均值,標 high-vol stocks
    if all_6m_vols:
        mean_vol = sum(all_6m_vols) / len(all_6m_vols)
        for row, vol in pending:
            scale = 0.5 if vol > mean_vol * VOL_MANAGED_THRESHOLD else 1.0
            row["detail"] = {
                "vol_managed_scale": scale,
                "realized_vol_6m": vol,
                "cross_mean_vol": mean_vol,
            }
            rows.append(row)
    else:
        for row, _ in pending:
            rows.append(row)

    # rank by return_6m(高的好)— 對齊 Chen-Chou-Hsieh 2023
    assign_ranks(rows, rank_col="momentum_rank", metric_col="return_6m",
                 reverse=True, top_n=TOP_N)
    written = upsert_silver(db, OUTPUT_TABLE, rows,
                            pk_cols=["market", "stock_id", "date"])
    elapsed_ms = int((time.monotonic() - start) * 1000)
    logger.info(f"[{NAME}] rows={len(rows)} written={written} ({elapsed_ms}ms)")
    return {"name": NAME, "rows_read": len(rows), "rows_written": written,
            "elapsed_ms": elapsed_ms}
