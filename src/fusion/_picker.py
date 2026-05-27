"""
fusion/_picker.py
=================
NEoWave forest scenario picker 共用 helpers — 跨模組 single source of truth。

對齊 v4.26 follow-up:`track1.py`(v4.25.x canonical)+ `_forecast.py`(v3.35
較舊)兩處 drift consolidation。本檔抽出 **truly identical** 的 helper(power
rating / pattern type / wave count / date 解析),既有 caller import 此處取代
local 副本。

**B1 consolidation(本次)**:degree quantity table + write-side invalidation
predicate 從 `track1.py` / `neely_emitter.py` / `_forecast.py` 三處發散收斂到
本檔 single source。Rust canonical:`rust_compute/cores/wave/neely_core/src/`
  - `output.rs::Degree` enum 11 variants(SubMicro → GrandSupercycle)
  - `degree/mod.rs::classify_degree` 6-level bracket(永不回 Minuette/Micro/SubMicro)
  - `triggers/mod.rs` Up pattern → PriceBreakBelow / Down pattern → PriceBreakAbove

**留 B3 統一**:track1.py 的 `scenario_is_invalidated`(讀取面,v4.25.x semantics
含 neutral 走 ALL kinds)不在本 PR 動;本 PR 只把 canonical write-side predicate
鎖在本檔給 `neely_emitter` 用。
"""

from __future__ import annotations

from datetime import date, datetime
from typing import Any


__all__ = [
    # Date
    "coerce_date",
    # Power rating
    "power_rating_label", "power_rating_strength", "power_rating_sign",
    "direction_from_power",
    # Pattern / wave
    "pattern_type_label", "wave_count_from_label",
    # Degree (B1 canonical)
    "DEGREE_ORDER", "DEGREE_RANK",
    "degree_rank", "classify_degree_by_years", "effective_degree",
    # Invalidation (B1 canonical, write-side)
    "canonical_is_invalidated",
]


# ────────────────────────────────────────────────────────────
# Date parsing
# ────────────────────────────────────────────────────────────


def coerce_date(s: Any) -> date | None:
    """ISO 字串 / date 物件 → date object。invalid → None。

    對齊 track1.py 的 _coerce_date(slice ISO[:10])+ _forecast.py 的
    _parse_iso_date(datetime.fromisoformat)— 兩處邏輯等價,本實作取
    track1 風格(切首 10 字元支持「2026-05-25T00:00:00」)+ 安全 fallback。
    """
    if isinstance(s, date):
        return s
    if isinstance(s, str):
        try:
            return date.fromisoformat(s[:10])
        except ValueError:
            try:
                return datetime.fromisoformat(s).date()
            except ValueError:
                return None
    return None


# ────────────────────────────────────────────────────────────
# Power rating helpers
# ────────────────────────────────────────────────────────────


def power_rating_label(rating: Any) -> str:
    """正規化 PowerRating → variant 字串。

    NEoWave PowerRating enum 在 JSON 序列化:
      - unit variant → 裸字串 "Bullish"
      - dict variant → {"Bullish": null} 對齊 serde tagged enum
    """
    if isinstance(rating, dict):
        return next(iter(rating.keys()), "Neutral")
    if isinstance(rating, str):
        return rating
    return "Neutral"


def power_rating_strength(rating: Any) -> int:
    """PowerRating → 強度級別 0-3。

    StrongBullish / StrongBearish → 3
    Bullish / Bearish             → 2
    SlightBullish / SlightBearish → 1
    Neutral / 其他                 → 0
    """
    if not rating:
        return 0
    if isinstance(rating, dict):
        rating = next(iter(rating.keys()), None)
    if not isinstance(rating, str):
        return 0
    return {
        "StrongBullish": 3, "StrongBearish": 3,
        "Bullish":       2, "Bearish":       2,
        "SlightBullish": 1, "SlightBearish": 1,
        "Neutral":       0,
    }.get(rating, 0)


def power_rating_sign(rating: Any) -> int:
    """+1 bull / -1 bear / 0 neutral。對齊 _forecast.py `_power_rating_sign`。"""
    if isinstance(rating, dict):
        rating = next(iter(rating.keys()), None)
    if not isinstance(rating, str):
        return 0
    if rating.endswith("Bullish"):
        return +1
    if rating.endswith("Bearish"):
        return -1
    return 0


def direction_from_power(rating: Any) -> str:
    """PowerRating → "bullish" / "bearish" / "neutral"。對齊 track1.py。"""
    label = power_rating_label(rating)
    if label.endswith("Bullish"):
        return "bullish"
    if label.endswith("Bearish"):
        return "bearish"
    return "neutral"


# ────────────────────────────────────────────────────────────
# Pattern type / wave count helpers
# ────────────────────────────────────────────────────────────


def pattern_type_label(pattern_type: Any) -> str | None:
    """NeelyPatternType → variant 字串首 key。

    JSON shape:
      - 純字串 "Impulse"(unit variant)
      - dict {"Diagonal": {"Leading": null}}(nested variant)
    """
    if isinstance(pattern_type, dict):
        return next(iter(pattern_type.keys()), None)
    if isinstance(pattern_type, str):
        return pattern_type
    return None


def wave_count_from_label(label: str | None) -> int:
    """從 structure_label 字串 regex 抽 `(\\d+)-wave`。回 0 if 無 match。

    例:"5-wave from mw27 to mw31" → 5
    """
    if not label:
        return 0
    import re
    m = re.search(r"(\d+)-wave", label)
    return int(m.group(1)) if m else 0


# ────────────────────────────────────────────────────────────
# B1 canonical degree(對齊 Rust output.rs::Degree + degree/mod.rs::classify_degree)
# ────────────────────────────────────────────────────────────

# 對齊 rust_compute/cores/wave/neely_core/src/output.rs::Degree
# 11 variants 由小至大列序(SubMicro=1 → GrandSupercycle=11)。
DEGREE_ORDER: tuple[str, ...] = (
    "SubMicro",
    "Micro",
    "SubMinuette",
    "Minuette",
    "Minute",
    "Minor",
    "Intermediate",
    "Primary",
    "Cycle",
    "Supercycle",
    "GrandSupercycle",
)

# 每 variant 1-11 唯一 rank;defensive 0 給 unknown / NULL。
DEGREE_RANK: dict[str, int] = {name: i + 1 for i, name in enumerate(DEGREE_ORDER)}


def degree_rank(label: str | None) -> int:
    """Degree label → 整數 rank 1-11;unknown / None → 0(防禦,與 unknown 不撞)。

    對齊 B1 spec:
      - Minor != Intermediate(rank 6 vs 7),修 track1 舊 bug Minor=Intermediate=4
      - SubMicro / Micro rank > 0(修 track1 缺漏落 0 與 unknown 撞)
    """
    if not label:
        return 0
    return DEGREE_RANK.get(label, 0)


def classify_degree_by_years(years: float) -> str:
    """資料時間跨度(年)→ Degree label。

    對齊 `rust_compute/cores/wave/neely_core/src/degree/mod.rs::classify_degree`:
      - < 1 yr   → "SubMinuette"
      - 1-3 yr   → "Minute"     (producer 此區間取 Minute,不產 Minuette)
      - 3-10 yr  → "Minor"      (producer 此區間取 Minor,不產 Intermediate)
      - 10-30 yr → "Primary"
      - 30-100 yr→ "Cycle"
      - ≥ 100 yr → "Supercycle"

    **永不回**: Minuette / Micro / SubMicro(producer 死碼,enum 保留但 classify 不發)。
    """
    if years < 1.0:
        return "SubMinuette"
    if years < 3.0:
        return "Minute"
    if years < 10.0:
        return "Minor"
    if years < 30.0:
        return "Primary"
    if years < 100.0:
        return "Cycle"
    return "Supercycle"


def _scenario_span_years(scenario: dict) -> float | None:
    """從 scenario.wave_tree.start / end 推算年跨度。回 None 若缺資料 / 反向 / 0 跨度。

    對齊 Rust degree/mod.rs:span_days = (last - first).num_days().max(0);
    span_years = span_days / 365.25(同除數)。
    """
    wt = scenario.get("wave_tree") or {}
    start = coerce_date(wt.get("start"))
    end = coerce_date(wt.get("end"))
    if start is None or end is None:
        return None
    delta_days = (end - start).days
    if delta_days <= 0:
        return None
    return delta_days / 365.25


def effective_degree(scenario: dict) -> str | None:
    """scenario.wave_tree → 跨度年 → classify_degree_by_years。

    缺 wave_tree / start / end → None(caller fallback degree_rank=0)。
    """
    years = _scenario_span_years(scenario)
    if years is None:
        return None
    return classify_degree_by_years(years)


# ────────────────────────────────────────────────────────────
# B1 canonical invalidation predicate(write-side;B3 提前)
# ────────────────────────────────────────────────────────────


def canonical_is_invalidated(scenario: dict, current_price: float | None) -> bool:
    """寫入面 invalidation predicate — direction-aware,禁 direction-blind fallback。

    對齊:
      - Rust `triggers/mod.rs`:Up pattern → PriceBreakBelow / Down pattern → PriceBreakAbove
      - skill b1-degree-consolidation:
          - bullish 只看 PriceBreakBelow(current < threshold → invalidated)
          - bearish 只看 PriceBreakAbove(current > threshold → invalidated)
          - **neutral 不濾**(永遠 False,scenario 為資訊性、非方向性 bet)
      - 只看 `OnTriggerAction == "InvalidateScenario"`;WeakenScenario / PromoteAlternative 不算

    Args:
        scenario: forest scenario dict(讀 power_rating 推 direction、讀 invalidation_triggers)
        current_price: 當下 close。**None → 永遠 False**(不靜默放行失效,
            由 caller 確保有價;若無價則不啟用 filter 步驟)

    Returns:
        True if scenario 在 current_price 下已失效(寫入面應 filter 掉)
    """
    if current_price is None:
        return False

    direction = direction_from_power(scenario.get("power_rating"))
    if direction == "neutral":
        return False

    try:
        cp = float(current_price)
    except (TypeError, ValueError):
        return False

    triggers = scenario.get("invalidation_triggers") or []
    for t in triggers:
        action = t.get("on_trigger")
        if isinstance(action, dict):
            action = next(iter(action.keys()), None)
        if action != "InvalidateScenario":
            continue

        trigger_type = t.get("trigger_type")
        if not isinstance(trigger_type, dict):
            continue

        if direction == "bullish" and "PriceBreakBelow" in trigger_type:
            try:
                threshold = float(trigger_type["PriceBreakBelow"])
            except (TypeError, ValueError):
                continue
            if cp < threshold:
                return True

        elif direction == "bearish" and "PriceBreakAbove" in trigger_type:
            try:
                threshold = float(trigger_type["PriceBreakAbove"])
            except (TypeError, ValueError):
                continue
            if cp > threshold:
                return True

    return False
