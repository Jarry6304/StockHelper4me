"""
silver/builders/commodity_macro.py
===================================
commodity_price_daily (Bronze) → commodity_price_daily_derived (Silver)。

對齊 m3Spec/environment_cores.md §十 拍版設計:
- 對每個 commodity GROUP BY,sort by date,計算 return_pct / return_z_score
  (60d rolling)/ momentum_state / streak_days
- momentum_state:'up' / 'down' / 'neutral'(return_pct sign + threshold);
  streak_days:連續同 state 天數
- PK 含 commodity 維度,未來擴 SILVER/OIL 自動跑(對齊 Bronze schema)

Reference:
- 60d rolling z-score:對齊 chip_core / institutional_core lookback_for_z=60
"""

from __future__ import annotations

import logging
import math
import time
from collections import defaultdict
from typing import Any

from .._common import fetch_bronze, upsert_silver


logger = logging.getLogger("collector.silver.builders.commodity_macro")


NAME          = "commodity_macro"
SILVER_TABLE  = "commodity_price_daily_derived"
BRONZE_TABLES = ["commodity_price_daily"]

LOOKBACK_DAYS = 60                  # 對齊 chip_core lookback_for_z
MOMENTUM_NEUTRAL_THRESHOLD = 0.05   # |return_pct| < 0.05% 視為 neutral


def _classify_momentum(return_pct: float | None) -> str:
    if return_pct is None:
        return "neutral"
    if return_pct > MOMENTUM_NEUTRAL_THRESHOLD:
        return "up"
    if return_pct < -MOMENTUM_NEUTRAL_THRESHOLD:
        return "down"
    return "neutral"


def _build_silver_rows(bronze_rows: list[dict[str, Any]]) -> list[dict[str, Any]]:
    """Per (market, commodity) 序列計算 return / z-score / momentum / streak。"""
    grouped: dict[tuple, list[dict[str, Any]]] = defaultdict(list)
    for row in bronze_rows:
        key = (row.get("market"), row.get("commodity"))
        grouped[key].append(row)

    out: list[dict[str, Any]] = []
    for (market, commodity), rows in grouped.items():
        rows.sort(key=lambda r: r["date"])
        returns: list[float | None] = []
        prev_price: float | None = None
        for r in rows:
            cur = r.get("price")
            cur_f = float(cur) if cur is not None else None
            if prev_price is not None and cur_f is not None and prev_price > 0:
                ret = (cur_f - prev_price) / prev_price * 100.0
            else:
                ret = None
            returns.append(ret)
            prev_price = cur_f if cur_f is not None else prev_price

        # rolling z-score(對 returns 算 60d window)
        streak = 0
        prev_state = "neutral"
        for i, r in enumerate(rows):
            ret = returns[i]
            # rolling z
            window_start = max(0, i - LOOKBACK_DAYS)
            window = [v for v in returns[window_start:i] if v is not None]
            z: float | None = None
            mean: float | None = None
            std: float | None = None
            if len(window) >= 10:
                mean = sum(window) / len(window)
                var = sum((v - mean) ** 2 for v in window) / len(window)
                std = math.sqrt(var)
                if std > 1e-9 and ret is not None:
                    z = (ret - mean) / std

            state = _classify_momentum(ret)
            if state == prev_state and state != "neutral":
                streak += 1
            elif state != "neutral":
                streak = 1
            else:
                streak = 0
            prev_state = state

            out.append({
                "market":         market,
                "commodity":      commodity,
                "date":           r["date"],
                "price":          float(r["price"]) if r.get("price") is not None else None,
                "return_pct":     ret,
                "return_z_score": z,
                "momentum_state": state,
                "streak_days":    streak,
                "detail": {
                    "lookback_days": LOOKBACK_DAYS,
                    "window_size":   len(window),
                    "mean":          mean,
                    "std":           std,
                },
            })
    return out


def run(
    db: Any,
    stock_ids: list[str] | None = None,  # commodity_macro 不接 stock_ids;對齊 SilverBuilder Protocol 簽名
    full_rebuild: bool = False,
) -> dict[str, Any]:
    start = time.monotonic()

    bronze = fetch_bronze(db, "commodity_price_daily", stock_ids=None)
    silver = _build_silver_rows(bronze)
    written = upsert_silver(
        db, SILVER_TABLE, silver,
        pk_cols=["market", "commodity", "date"],
    )

    elapsed_ms = int((time.monotonic() - start) * 1000)
    logger.info(f"[{NAME}] read={len(bronze)} → wrote={written}({elapsed_ms}ms)")
    return {
        "name":         NAME,
        "rows_read":    len(bronze),
        "rows_written": written,
        "elapsed_ms":   elapsed_ms,
    }
