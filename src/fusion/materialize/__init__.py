"""Golden L3 — fusion 物化階段(levels / resonance / climate)。

把原本 read-time compute 的 fusion 輸出物化進 `structural_snapshots`(新 core_name),
令對外(MCP / Web API)只讀。對齊 m3Spec/golden-layers.md。

公開入口:
- `run_fusion_materialize(db, ...)` — levels_fusion + resonance_fusion(per-stock)
- `run_climate_materialize(db, ...)` — climate_fusion(marketwide,獨立 aggregator)
- `fetch_fusion_doc(conn, ...)` — 讀已物化的 fusion row(MCP / API 共用)
"""

from __future__ import annotations

from fusion.materialize.climate_stage import run_climate_materialize
from fusion.materialize.fusion_stage import run_fusion_materialize
from fusion.materialize.read import fetch_fusion_doc

__all__ = [
    "run_fusion_materialize",
    "run_climate_materialize",
    "fetch_fusion_doc",
]
