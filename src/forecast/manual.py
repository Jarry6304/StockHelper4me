"""Manual scenario entry — discretionary forecast row(human judgment)。

對齊 user v0.3 區間預測 spec phase 6 + plan 文件 phase 6。

Strict spec rule(§「強制規則」):
  - 手動情景只前向 log,**禁回測**
  - calibrated=False(discretionary,不可宣稱統計覆蓋率)
  - source_core='manual'

Forecast_log 仍然會被 settlement 結算 pinball + hit(scorer 可看 regime-conditional
表現),但 fusion gate **不**接受 manual 為 eligible core(spec rule)。
"""

from __future__ import annotations

import hashlib
from datetime import date
from typing import Any

from forecast._db import upsert_forecast


__all__ = ["write_manual_forecast"]


def write_manual_forecast(
    conn,
    *,
    stock_id: str,
    forecast_date: date,
    horizon_days: int,
    lower: float,
    upper: float,
    confidence: float = 0.70,
    point: float | None = None,
    regime: str | None = None,
    note: str | None = None,
) -> dict[str, Any]:
    """Write a single discretionary forecast row.

    Args:
        regime: 自訂 regime_tag(e.g. "5wave_completion", "support_test")。
                Used by scorer group_by='regime_tag' for after-the-fact
                analysis of which scenarios you nailed.
        note: 短註解,寫進 params_hash 後綴方便追溯。

    Returns:
        Status dict with the upserted row's key fields.
    """
    if lower >= upper:
        raise ValueError(f"lower ({lower}) must be < upper ({upper})")
    if not (0.0 < confidence < 1.0):
        raise ValueError(f"confidence must be in (0,1), got {confidence}")
    if horizon_days <= 0:
        raise ValueError(f"horizon_days must be > 0, got {horizon_days}")

    # Hash includes note for traceability — even if re-running with different
    # judgment for same (stock, date, horizon, source), params_hash differs.
    seed = f"{stock_id}|{forecast_date}|{horizon_days}|{regime or ''}|{note or ''}"
    params_hash = "manual|" + hashlib.sha256(seed.encode()).hexdigest()[:12]

    if point is None:
        point = (lower + upper) / 2.0

    upsert_forecast(
        conn,
        {
            "stock_id": stock_id,
            "forecast_date": forecast_date,
            "horizon_days": horizon_days,
            "lower": round(lower, 4),
            "upper": round(upper, 4),
            "point": round(point, 4),
            "confidence": confidence,
            "calibrated": False,
            "source_core": "manual",
            "regime_tag": regime,
            "params_hash": params_hash,
        },
    )
    return {
        "status": "written",
        "stock_id": stock_id,
        "forecast_date": str(forecast_date),
        "horizon_days": horizon_days,
        "lower": lower,
        "upper": upper,
        "regime": regime,
    }
