"""v4.32 Golden L3 — Python Web API 輸出的 wire-shape pydantic 契約(→ TS codegen 來源)。

⚠️ 這些 model 鏡射的是 **`.to_dict()` / jsonable_encoder 序列化形狀**(date→ISO 字串、
int-key dict→str-key object),**不是** dataclass 的 type hints(那些 date / int-key 會
生錯 TS)。對齊 m3Spec/read-api.md Track B。

來源對應:
- LevelsFusion      ← src/fusion/key_levels.py::key_levels() 回傳 dict(物化 levels_fusion)
- ResonanceFusion   ← src/fusion/dual_track/_shared.py::DualTrackResult.to_dict()(物化 resonance_fusion)
- ClimateFusion     ← mcp_server/_climate.py::compute_market_context() 回傳 dict(物化 climate_fusion)
- ScreenResponse    ← web_api/routers/screens.py:screen() 回傳 dict(row-assembly,#1 後 rank 正規化 + 三欄 denylist)
- ScreenRow*        ← 10 個 *_ranked_derived 表的 row 形狀(base + per-toolkit metric 擴充)

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


# ── screens(/screens/{toolkit}.rows)─────────────────────────────────────────
# 對齊 #1 後 row 形狀:per-toolkit rank 欄(原值保留)+ 正規化 `rank` 鍵 + LEFT JOIN
# stock_info_ref(name + industry)。`detail` / `is_dirty` / `dirty_at` 已被 _db
# denylist 砍掉,不出現在 wire。
#
# 設計:base + 10 個 per-toolkit subclass(`ScreenRow{Toolkit}`)。`ScreenResponse.rows`
# 用 `list[ScreenRowBase]`(共通 surface);前端依 `toolkit` narrow 到具體 subtype。
# 既有原 per-toolkit rank 欄(`combined_rank` / `momentum_rank` / ...)留在 wire 但
# 不列入型別(避免 10 個 toolkit 之間 rank 欄名不一致污染共通 base)。
class ScreenRowBase(BaseModel):
    stock_id: str
    market: str
    date: str
    stock_name: str | None = None
    industry_category: str | None = None
    universe_size: int | None = None
    excluded_reason: str | None = None
    rank: int | None = None  # 正規化自各 *_rank;對齊 #1 _db post-process
    is_top_n: bool


# Toolkit A:Monthly
class ScreenRowMagicFormula(ScreenRowBase):
    ebit_ttm: float | None = None
    market_cap: float | None = None
    total_debt: float | None = None
    cash: float | None = None
    enterprise_value: float | None = None
    invested_capital: float | None = None
    earnings_yield: float | None = None
    roic: float | None = None
    ey_rank: int | None = None
    roic_rank: int | None = None


class ScreenRowPersistentMomentum(ScreenRowBase):
    return_6m: float | None = None
    return_12m_1m: float | None = None
    persistent_months: int | None = None


class ScreenRowRevenueMomentum(ScreenRowBase):
    revenue_yoy_latest: float | None = None
    consecutive_positive: int | None = None


class ScreenRowInstitutionalConcert(ScreenRowBase):
    concert_days: int | None = None
    foreign_cumulative_20d: float | None = None
    shares_outstanding: float | None = None
    cumulative_pct: float | None = None


# Toolkit B:Quarterly
class ScreenRowFScore(ScreenRowBase):
    f_score: int | None = None
    profitability: int | None = None
    leverage: int | None = None
    efficiency: int | None = None


class ScreenRowLowVolatility(ScreenRowBase):
    std_252d: float | None = None


class ScreenRowIndustryAdjGp(ScreenRowBase):
    gross_profitability: float | None = None
    industry: str | None = None
    industry_median_gp: float | None = None
    industry_adj_gp: float | None = None


# Toolkit C:Annual
class ScreenRowLongTermLowVol(ScreenRowBase):
    std_36m: float | None = None


class ScreenRowDividendYield(ScreenRowBase):
    dividend_yield_pct: float | None = None
    return_12m_pct: float | None = None
    payout_years_5y: int | None = None


class ScreenRowMom12_1(ScreenRowBase):
    return_12m_1m: float | None = None


class ScreenResponse(BaseModel):
    toolkit: str
    ranking_date: str | None
    top_n: int
    offset: int
    rows: list[ScreenRowBase]


# ── /stocks?q= search(個股入口 autocomplete)─────────────────────────────────
class StockRef(BaseModel):
    stock_id: str
    stock_name: str | None = None
    industry_category: str | None = None


# ── /stocks/{id}/ohlc(price_daily_fwd 切片)─────────────────────────────────
# 命名 `PriceBar` / `PriceSeries` 避開既有 ts-rs `OhlcvBar` / `OhlcvSeries`(neely core
# 輸入,僅差一個 `v`)。語意=後復權「價格」(取自 price_daily_fwd),前端 K 線繪製用。
# NUMERIC → float(jsonable_encoder 處理 Decimal),BIGINT → int(volume)。
class PriceBar(BaseModel):
    date: str
    open: float | None = None
    high: float | None = None
    low: float | None = None
    close: float | None = None
    volume: int | None = None


class PriceSeries(BaseModel):
    stock_id: str
    rows: list[PriceBar]
