"""dual_track · 軌道二(統計)讀法。

對齊 m3Spec/dual_track_resonance.md §三 + §七:
- 讀 forecast_log filtered(internal_only = FALSE 預設過濾,擋掉 neely_fib 對齊影子)
- 每個 horizon × confidence 取一個帶(lower / upper / point)
- 來源 source_core 偏好 fusion;若 fusion 缺則 fallback 校準 cores(kalman_cqr 等)
- 計算 width_ratio(寬/現價)+ is_overly_wide 防呆(對齊 §三)

v4.28+ 性能改善(2026-05-27):
- 新加 `fetch_bands_batch` 取代 read_track2 內 3 × 6 = 18 N+1 query pattern
- 單 SQL 用 `ANY(%s::int[])` + `ANY(%s::text[])` 批次取 horizons × sources
- Python 端依 source_preference rank 取每 horizon 首選
- `fetch_band` 保留為單一 horizon 的 backward-compat shim
"""

from __future__ import annotations

from datetime import date
from typing import Any

from fusion.dual_track._shared import (
    ALL_HORIZONS,
    BAND_WIDTH_THRESHOLD,
    PRIMARY_CONFIDENCE,
    PRIMARY_HORIZON_DAYS,
    Track2Band,
    Track2View,
)


__all__ = ["read_track2", "fetch_band", "fetch_bands_batch"]


# Source-core preference order(對齊 fusion 哲學 — 多源組合 > 單源)
# fusion 為首選(對齊 M8 + M7 spec);若 fusion 缺則退單 core。
_SOURCE_PREFERENCE: tuple[str, ...] = (
    "fusion",
    "kalman_cqr",
    "log_channel_cqr",
    "chip_forecast_core_cqr",
    "macro_forecast_core_cqr",
    "fundamental_forecast_core_cqr",
)


def _row_to_band(
    row: dict[str, Any],
    *,
    horizon_days: int,
    confidence: float,
    current_price: float | None,
) -> Track2Band:
    """共用 row → Track2Band(width_ratio / is_overly_wide 防呆邏輯單一化)。"""
    lower = float(row["lower"])
    upper = float(row["upper"])
    point = row.get("point")
    point_f = float(point) if point is not None else (lower + upper) / 2.0
    width = upper - lower
    width_ratio: float | None
    is_overly_wide: bool
    if current_price is not None and float(current_price) > 0:
        width_ratio = round(width / float(current_price), 6)
        is_overly_wide = width_ratio > BAND_WIDTH_THRESHOLD
    else:
        width_ratio = None
        is_overly_wide = False
    return Track2Band(
        horizon_days=horizon_days,
        confidence=confidence,
        lower=lower,
        upper=upper,
        point=point_f,
        source_core=str(row["source_core"]),
        width_ratio=width_ratio,
        is_overly_wide=is_overly_wide,
    )


def fetch_bands_batch(
    conn,
    *,
    stock_id: str,
    forecast_date: date,
    horizons: tuple[int, ...],
    confidence: float,
    current_price: float | None,
    source_preference: tuple[str, ...] | None = None,
) -> dict[int, Track2Band]:
    """單 query 批次取多 horizon band,Python 端依 source_preference rank 取首選。

    v4.28+ 替代 read_track2 內 3 × 6 = 18 query N+1 pattern。

    Deterministic via uq_forecast_log_lookup(stock_id, forecast_date, horizon_days,
    source_core, confidence) — 同 (stock × date × horizon × source × conf) 唯一 row;
    ABS(confidence - %s) < 1e-6 tolerance 不破唯一性(production confidence
    都是 0.50/0.80/0.95 三固定值)。
    """
    if not horizons:
        return {}
    pref = source_preference if source_preference is not None else _SOURCE_PREFERENCE
    if not pref:
        return {}
    pref_rank = {src: idx for idx, src in enumerate(pref)}

    sql = """
        SELECT horizon_days, source_core, lower, upper, point
          FROM forecast_log
         WHERE stock_id      = %s
           AND forecast_date = %s
           AND horizon_days  = ANY(%s::int[])
           AND source_core   = ANY(%s::text[])
           AND ABS(confidence - %s) < 1e-6
           AND internal_only = FALSE
           AND lower IS NOT NULL
           AND upper IS NOT NULL
    """
    with conn.cursor() as cur:
        cur.execute(sql, (
            stock_id, forecast_date,
            list(horizons), list(pref),
            confidence,
        ))
        rows = cur.fetchall()

    # Python 端依 source_preference 對每 horizon 取首選 row(最小 pref_rank 勝)。
    best_by_horizon: dict[int, tuple[int, dict[str, Any]]] = {}
    for row in rows:
        h = int(row["horizon_days"])
        src = str(row["source_core"])
        rank = pref_rank.get(src, len(pref))
        prev = best_by_horizon.get(h)
        if prev is None or rank < prev[0]:
            best_by_horizon[h] = (rank, row)

    out: dict[int, Track2Band] = {}
    for h, (_, row) in best_by_horizon.items():
        out[h] = _row_to_band(
            row,
            horizon_days=h,
            confidence=confidence,
            current_price=current_price,
        )
    return out


def fetch_band(
    conn,
    *,
    stock_id: str,
    forecast_date: date,
    horizon_days: int,
    confidence: float,
    current_price: float | None,
    source_preference: tuple[str, ...] | None = None,
) -> Track2Band | None:
    """取單一 (horizon, confidence) 的軌道二涵蓋帶。

    v4.28+ 改為 fetch_bands_batch 的 backward-compat shim(單 horizon 包裝);
    既有 caller 簽章 / 行為不變。

    Args:
        current_price: 用來算 width_ratio(寬/現價);None → width_ratio=None
                       且 is_overly_wide=False(無法判防呆)。

    Returns:
        Track2Band(lower / upper / point 必齊),否則 None(該 horizon/conf 無資料)
    """
    out = fetch_bands_batch(
        conn,
        stock_id=stock_id,
        forecast_date=forecast_date,
        horizons=(horizon_days,),
        confidence=confidence,
        current_price=current_price,
        source_preference=source_preference,
    )
    return out.get(horizon_days)


def read_track2(
    conn,
    *,
    stock_id: str,
    as_of: date,
    current_price: float | None,
    primary_horizon: int = PRIMARY_HORIZON_DAYS,
    primary_confidence: float = PRIMARY_CONFIDENCE,
    horizons: tuple[int, ...] = ALL_HORIZONS,
    source_preference: tuple[str, ...] | None = None,
) -> Track2View:
    """讀 forecast_log filtered → Track2View(多 horizon 涵蓋帶)。

    對齊 §五 T2:同時取 21 / 63 / 126(或 horizons 自訂)各自 band。
    主判定走 primary_horizon(預設 63)+ primary_confidence(預設 0.80)。

    v4.28+ 改用 fetch_bands_batch 單 query 取代既有 N × M = 18 query N+1 pattern;
    `read_track2` API 簽章 / Track2View 行為不變。

    Note:
        as_of 對 forecast_log 的語意是 forecast_date(預測「發出日」),取現有
        資料中 forecast_date 等於 as_of 的 row。若 as_of 沒有預測 row 會回缺。
    """
    bands = fetch_bands_batch(
        conn,
        stock_id=stock_id,
        forecast_date=as_of,
        horizons=horizons,
        confidence=primary_confidence,
        current_price=current_price,
        source_preference=source_preference,
    )

    primary = bands.get(primary_horizon)

    notes: list[str] = []
    if not bands:
        notes.append(
            f"no forecast_log rows for stock={stock_id} forecast_date={as_of} "
            f"confidence={primary_confidence} (try running backtest / fuse first)"
        )
    elif primary is None:
        notes.append(
            f"primary horizon {primary_horizon} not available; available: {sorted(bands.keys())}"
        )

    return Track2View(
        stock_id=stock_id,
        as_of=as_of,
        current_price=current_price,
        primary_horizon=primary_horizon,
        primary_confidence=primary_confidence,
        primary_band=primary,
        horizons=bands,
        notes=notes,
    )
