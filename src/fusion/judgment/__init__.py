"""fusion.judgment — 證據 → 判讀 → 錨定迴路的資料層(m3Spec/wave_judgment_loop.md)。

模組:
- anchor_key:日期樹錨定鍵(§6;PIT 身分,golden test 凍結格式)
- dossier:引擎證據卷宗 builder(§4;無 primary、無分數)
- db:wave_judgments 存取(§5;append-only PIT)
- validate:判讀 JSON 驗證(§2 階段 4)— Phase 4
- diff:J2 錨定 diff(§6)— Phase 5
"""

from .anchor_key import (
    anchor_key,
    forest_anchor_keys,
    is_strict_subtree,
    pattern_tag,
    scenario_anchor_key,
)
from .db import (
    fetch_active_judgment,
    fetch_active_judgments_batch,
    fetch_all_active_judgments,
    fetch_judgments,
    insert_judgment,
)
from .dossier import CANDIDATES_CAP, LIVE_EDGE_BARS, build_dossier
from .validate import JudgmentValidationError, validate_judgment

__all__ = [
    "JudgmentValidationError",
    "validate_judgment",
    "anchor_key",
    "scenario_anchor_key",
    "pattern_tag",
    "forest_anchor_keys",
    "is_strict_subtree",
    "build_dossier",
    "CANDIDATES_CAP",
    "LIVE_EDGE_BARS",
    "fetch_active_judgment",
    "fetch_active_judgments_batch",
    "fetch_all_active_judgments",
    "fetch_judgments",
    "insert_judgment",
]
