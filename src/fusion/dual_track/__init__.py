"""Fusion · 雙軌共振決策層(dual_track)。

對齊 m3Spec/dual_track_resonance.md v1.0。

三層平面:
- 事實層(零改動,僅 forecast_log 加 internal_only 欄,見 alembic f2g3h4i5j6k7)
- 讀法層 — 本套件:
    - track1(結構)讀 structural_snapshots → 離散 fib 線 / label / 失效價
    - track2(統計)讀 forecast_log filtered → 涵蓋帶 + 中位數 + 多 horizon
- 關係層 — 本套件 resonance:
    - A-3 失效閘門(前置、軌道一資格)
    - A-1 三級共振判定(逐 fib 線:分歧 / 基礎 / 強)
    - cross_stock 旁路升振(is_top_30,並聯不擋路、命中升振、未命中不扣分)
    - T1/T2 時間反向標註(借軌道二精確時間描述軌道一價格命中)

公開 API:
    `resonance(stock_id, as_of, ...)` — 一次回傳完整 DualTrackResult。
"""

from fusion.dual_track._shared import (
    FibLine,
    FibLineResonance,
    Track1View,
    Track2Band,
    Track2View,
    DualTrackResult,
)
from fusion.dual_track.resonance import resonance

__all__ = [
    "FibLine",
    "FibLineResonance",
    "Track1View",
    "Track2Band",
    "Track2View",
    "DualTrackResult",
    "resonance",
]
