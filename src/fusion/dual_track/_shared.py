"""dual_track 共用 dataclass。

對齊 m3Spec/dual_track_resonance.md §八「模組落地對應」。
"""

from __future__ import annotations

from dataclasses import dataclass, field
from datetime import date
from typing import Any


# ─── 軌道一 ──────────────────────────────────────────────────────────────────


@dataclass
class FibLine:
    """軌道一輸出 — 一條離散 fib 線。

    對齊 m3Spec/dual_track_resonance.md §三:「FibZone 為『單一 fib 比例 ± 容差』
    窄帶,線位取 source_ratio 對應價」(實作上取 zone low/high 中點作為線位)。
    """
    price: float                     # 線位(zone 中點)
    low: float                       # FibZone tolerance 下限
    high: float                      # FibZone tolerance 上限
    label: str | None                # e.g. "0.382" / "0.618" / "1.000"
    source_ratio: float | None       # zone.source_ratio 原值

    def to_dict(self) -> dict[str, Any]:
        return {
            "price": self.price,
            "low": self.low,
            "high": self.high,
            "label": self.label,
            "source_ratio": self.source_ratio,
        }


@dataclass
class Track1View:
    """軌道一完整投影 — neely primary scenario 摘要 + 離散 fib 線。"""
    stock_id: str
    as_of: date
    snapshot_date: date | None       # structural_snapshots 來源日期
    has_snapshot: bool               # 是否有 SS row(無 → 軌道一不可用)
    pattern_type: str | None         # scenario.pattern_type
    power_rating: str | None         # StrongBullish / Bullish / Neutral / ...
    direction: str                   # "bullish" / "bearish" / "neutral"
    effective_degree: str | None     # SubMinuette / Minute / Minor / ...
    wave_count: int                  # 從 structure_label "N-wave" parse
    fib_lines: list[FibLine] = field(default_factory=list)
    invalidation_price: float | None = None  # 失效價(若有)
    invalidated: bool = False        # 現價已跌破 invalidation?
    fallback_to_flat_union: bool = False  # primary 無 zones,用 flat_fib_zones
    notes: list[str] = field(default_factory=list)

    def to_dict(self) -> dict[str, Any]:
        return {
            "stock_id": self.stock_id,
            "as_of": self.as_of.isoformat(),
            "snapshot_date": self.snapshot_date.isoformat() if self.snapshot_date else None,
            "has_snapshot": self.has_snapshot,
            "pattern_type": self.pattern_type,
            "power_rating": self.power_rating,
            "direction": self.direction,
            "effective_degree": self.effective_degree,
            "wave_count": self.wave_count,
            "fib_lines": [f.to_dict() for f in self.fib_lines],
            "invalidation_price": self.invalidation_price,
            "invalidated": self.invalidated,
            "fallback_to_flat_union": self.fallback_to_flat_union,
            "notes": self.notes,
        }


# ─── 軌道二 ──────────────────────────────────────────────────────────────────


@dataclass
class Track2Band:
    """軌道二單一 (horizon, confidence) 涵蓋帶。

    對齊 m3Spec/dual_track_resonance.md §二「涵蓋帶(內部機率質量)」+ §十一
    「主判定走 63 / 涵蓋判定 confidence=0.80」+ §三「防呆:寬度/現價 > 閾值
    視為過寬」。
    """
    horizon_days: int
    confidence: float
    lower: float
    upper: float
    point: float                     # 中位數(forecast_log.point)
    source_core: str                 # fusion / kalman_cqr / log_channel_cqr / ...
    width_ratio: float | None        # (upper-lower) / current_price
    is_overly_wide: bool             # width_ratio > _BAND_WIDTH_THRESHOLD(防呆)

    def covers(self, price: float) -> bool:
        return self.lower <= price <= self.upper

    def to_dict(self) -> dict[str, Any]:
        return {
            "horizon_days": self.horizon_days,
            "confidence": self.confidence,
            "lower": self.lower,
            "upper": self.upper,
            "point": self.point,
            "source_core": self.source_core,
            "width_ratio": self.width_ratio,
            "is_overly_wide": self.is_overly_wide,
        }


@dataclass
class Track2View:
    """軌道二完整投影 — 多 horizon 對齊。"""
    stock_id: str
    as_of: date
    current_price: float | None      # 現價(從 price_daily 直撈)
    primary_horizon: int             # 主判定 horizon(預設 63)
    primary_confidence: float        # 主判定 confidence(預設 0.80)
    primary_band: Track2Band | None  # 主判定 band(None=無校準 core 對齊輸出)
    horizons: dict[int, Track2Band]  # 21 / 63 / 126 各自 band(可能缺)
    notes: list[str] = field(default_factory=list)

    def to_dict(self) -> dict[str, Any]:
        return {
            "stock_id": self.stock_id,
            "as_of": self.as_of.isoformat(),
            "current_price": self.current_price,
            "primary_horizon": self.primary_horizon,
            "primary_confidence": self.primary_confidence,
            "primary_band": self.primary_band.to_dict() if self.primary_band else None,
            "horizons": {h: b.to_dict() for h, b in self.horizons.items()},
            "notes": self.notes,
        }


# ─── 關係層 ──────────────────────────────────────────────────────────────────


@dataclass
class FibLineResonance:
    """一條 fib 線的共振判定。

    對齊 m3Spec/dual_track_resonance.md §三:三級共振 + cross_stock 升振 + T1/T2。
    """
    fib_line: FibLine
    level: str                       # "divergence" / "basic" / "strong"
    band_covers: bool                # ② track2 primary band 覆蓋 fib_line.price
    median_close: bool               # ③ track2 point 貼近 fib_line.price(within tolerance)
    cross_stock_boost: bool          # is_top_30 命中(只在 basic+ 升振)
    t1_horizon: int | None           # T1:命中時標 track2 horizon
    t2_profile: dict[int, str]       # T2:21/63/126 多 horizon 各自共振等級
    notes: list[str] = field(default_factory=list)

    def to_dict(self) -> dict[str, Any]:
        return {
            "fib_line": self.fib_line.to_dict(),
            "level": self.level,
            "band_covers": self.band_covers,
            "median_close": self.median_close,
            "cross_stock_boost": self.cross_stock_boost,
            "t1_horizon": self.t1_horizon,
            "t2_profile": self.t2_profile,
            "notes": self.notes,
        }


@dataclass
class DualTrackResult:
    """雙軌共振完整結果。"""
    stock_id: str
    as_of: date
    track1: Track1View
    track2: Track2View
    is_top_30: bool                  # cross_stock 升振狀態
    is_top_30_source: str | None     # 對應 builder ranked_derived 表名
    is_top_30_date: date | None      # ranked_derived 取的 ranking_date
    findings: list[FibLineResonance] = field(default_factory=list)
    single_track_mode: bool = False  # True = A-3 閘門觸發,軌道一退場
    notes: list[str] = field(default_factory=list)

    def to_dict(self) -> dict[str, Any]:
        return {
            "stock_id": self.stock_id,
            "as_of": self.as_of.isoformat(),
            "track1": self.track1.to_dict(),
            "track2": self.track2.to_dict(),
            "is_top_30": self.is_top_30,
            "is_top_30_source": self.is_top_30_source,
            "is_top_30_date": self.is_top_30_date.isoformat() if self.is_top_30_date else None,
            "findings": [f.to_dict() for f in self.findings],
            "single_track_mode": self.single_track_mode,
            "notes": self.notes,
        }


# ─── Constants(對齊 §十一 MVP 預設值)────────────────────────────────────


# 主判定 horizon / confidence
PRIMARY_HORIZON_DAYS: int = 63
PRIMARY_CONFIDENCE: float = 0.80

# 多 horizon T2 剖面(spec §五)
ALL_HORIZONS: tuple[int, ...] = (21, 63, 126)

# Track 2 帶寬防呆閾值(寬/現價 > 此值 → 視為過寬,抑制共振)
BAND_WIDTH_THRESHOLD: float = 0.30

# Track 2 中位數貼近容差(|median - fib_line| / 現價 < 此值 → 算貼近)
MEDIAN_CLOSE_TOLERANCE: float = 0.02

# Cross-stock 默認 ranked_derived 表(MVP)
DEFAULT_CROSS_STOCK_TABLE: str = "magic_formula_ranked_derived"

# Track 1 fib_lines 上限 + 1% 價位 cluster(對齊 fusion._shared.cluster_price_levels)
# production 案例:flat_fib_zones 可達 100+ 條,直 MCP 暴露會撐爆 context budget。
# 取 1% bucket cluster + max 30 cap,~30 條對 LLM 仍有意義且 payload < 30KB。
FIB_LINES_MAX_COUNT: int = 30
FIB_LINES_CLUSTER_PCT: float = 0.01
