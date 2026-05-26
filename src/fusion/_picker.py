"""
fusion/_picker.py
=================
NEoWave forest scenario picker 共用 helpers — 跨模組 single source of truth。

對齊 v4.26 follow-up:`track1.py`(v4.25.x canonical)+ `_forecast.py`(v3.35
較舊)兩處 drift consolidation。本檔抽出 **truly identical** 的 helper(power
rating / pattern type / wave count / date 解析),既有 caller import 此處取代
local 副本。

**Out of scope**(需要獨立 audit):
- `effective_degree(scenario)`:track1 7-level(Subminuette/Minuette/Minute/...)
  vs _forecast 6-level(SubMinuette/Minute/...);兩處 docstring 都聲稱對齊
  spec §13.3,但 bracket 不同。需 Rust `degree/mod.rs::classify_degree` 對齊
  audit 後再 consolidate。
- `DEGREE_RANK`:track1 scale 1-8 vs _forecast 1-11(後者含 Micro/SubMicro)。
  相對順序對,但 rank value 差別。
- `pick_primary(forest)`:track1 純 sort,_forecast 在 sort 前先做 invalidation
  filter。兩種設計都 valid,需 user 拍版 canonical pattern。
- `scenario_is_invalidated`:track1 direction-aware(v4.25.x 拍版)vs _forecast
  ANY-trigger(v4.25.x 揭露 production 誤判但 _forecast.py 尚未同步修)。
  semantic divergence,需獨立 PR audit + 修 _forecast.py。
"""

from __future__ import annotations

from datetime import date, datetime
from typing import Any


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
