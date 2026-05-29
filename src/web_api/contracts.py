"""v4.32 Golden L3 — Python fusion 輸出的 wire-shape pydantic 契約(→ TS codegen 來源)。

⚠️ 這些 model 鏡射的是 **`.to_dict()` 序列化形狀**(date→ISO 字串、int-key dict→str-key
object),**不是** dataclass 的 type hints(那些 date / int-key 會生錯 TS)。對齊
m3Spec/read-api.md Track B。

來源對應:
- LevelsFusion      ← src/fusion/key_levels.py::key_levels() 回傳 dict(物化 levels_fusion)
- ResonanceFusion   ← src/fusion/dual_track/_shared.py::DualTrackResult.to_dict()(物化 resonance_fusion)
- ClimateFusion     ← mcp_server/_climate.py::compute_market_context() 回傳 dict(物化 climate_fusion)

codegen:`pydantic2ts --module web_api.contracts --output frontend/src/contracts/fusion.ts`
"""

from __future__ import annotations

from pydantic import BaseModel


# ── levels_fusion(key_levels)────────────────────────────────────────────────
class Level(BaseModel):
    price: float
    low: float
    high: float
    sources: list[str]
    strength: int
    member_count: int


class LevelsFusion(BaseModel):
    stock_id: str
    as_of: str
    source_point_count: int
    level_count_total: int
    level_count: int
    levels: list[Level]


# ── resonance_fusion(DualTrackResult.to_dict)────────────────────────────────
class FibLine(BaseModel):
    price: float
    low: float
    high: float
    label: str | None
    source_ratio: float | None


class Track1View(BaseModel):
    stock_id: str
    as_of: str
    snapshot_date: str | None
    has_snapshot: bool
    pattern_type: str | None
    power_rating: str | None
    direction: str
    effective_degree: str | None
    wave_count: int
    fib_lines: list[FibLine]
    invalidation_price: float | None
    invalidated: bool
    fallback_to_flat_union: bool
    notes: list[str]


class Track2Band(BaseModel):
    horizon_days: int
    confidence: float
    lower: float
    upper: float
    point: float
    source_core: str
    width_ratio: float | None
    is_overly_wide: bool


class Track2View(BaseModel):
    stock_id: str
    as_of: str
    current_price: float | None
    primary_horizon: int
    primary_confidence: float
    primary_band: Track2Band | None
    # to_dict 把 dict[int, Track2Band] 序列化成 str-key object(JSON 鍵恆為字串)
    horizons: dict[str, Track2Band]
    notes: list[str]


class FibLineResonance(BaseModel):
    fib_line: FibLine
    level: str
    band_covers: bool
    median_close: bool
    cross_stock_boost: bool
    t1_horizon: int | None
    # T2 剖面:str-key(21/63/126)→ 共振等級
    t2_profile: dict[str, str]
    notes: list[str]


class ResonanceFusion(BaseModel):
    stock_id: str
    as_of: str
    track1: Track1View
    track2: Track2View
    is_top_30: bool
    is_top_30_source: str | None
    is_top_30_date: str | None
    findings: list[FibLineResonance]
    single_track_mode: bool
    notes: list[str]


# ── climate_fusion(compute_market_context)────────────────────────────────────
class ClimateComponent(BaseModel):
    # 7 env components 用 score + fact_count;risk_alert 用 score + 3 個聚合欄(皆 optional)
    score: float
    fact_count: int | None = None
    active_disposition_stocks: int | None = None
    escalations_60d: int | None = None
    announced_14d: int | None = None


class ClimateFusion(BaseModel):
    as_of: str
    overall_climate: str
    climate_score: float
    components: dict[str, ClimateComponent]
    systemic_risks: list[str]
    narrative: str
