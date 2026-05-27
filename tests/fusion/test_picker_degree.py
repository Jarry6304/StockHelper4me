"""B1:fusion/_picker.py canonical degree consolidation tests.

對齊 b1-degree-consolidation skill「驗收標準」:
- DEGREE_ORDER 對齊 Rust output.rs::Degree 11 variants
- rank 嚴格遞增且唯一(回歸:鎖 track1 舊 bug Minor=Intermediate=4)
- SubMicro / Micro rank > 0(回歸:鎖 track1 缺漏落 0 與 unknown 撞)
- classify bracket 對齊 Rust degree/mod.rs::classify_degree
- classify 永不回 Minuette / Micro / SubMicro(producer 死碼)
"""

from __future__ import annotations

import sys
from datetime import date, timedelta
from pathlib import Path

import pytest

_REPO_ROOT = Path(__file__).resolve().parent.parent.parent
_SRC_ROOT = _REPO_ROOT / "src"
for _p in (str(_SRC_ROOT), str(_REPO_ROOT)):
    if _p not in sys.path:
        sys.path.insert(0, _p)

from fusion._picker import (  # noqa: E402
    DEGREE_ORDER,
    DEGREE_RANK,
    classify_degree_by_years,
    degree_rank,
    effective_degree,
)


# ────────────────────────────────────────────────────────────
# DEGREE_ORDER / DEGREE_RANK 結構
# ────────────────────────────────────────────────────────────


class TestDegreeOrder:
    def test_order_matches_rust_enum_11_variants(self):
        """對齊 rust_compute/cores/wave/neely_core/src/output.rs::Degree 由小至大列序。"""
        assert DEGREE_ORDER == (
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

    def test_order_length_is_11(self):
        assert len(DEGREE_ORDER) == 11

    def test_rank_ascending_unique(self):
        """rank 1..11 嚴格遞增 + unique。"""
        ranks = [DEGREE_RANK[d] for d in DEGREE_ORDER]
        assert ranks == [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]
        assert len(set(ranks)) == 11  # unique

    def test_minor_vs_intermediate_distinct_rank(self):
        """回歸:鎖 track1.py 舊 bug Minor=Intermediate=4(本 PR 修為 6 vs 7)。"""
        assert DEGREE_RANK["Minor"] != DEGREE_RANK["Intermediate"]
        assert DEGREE_RANK["Minor"] == 6
        assert DEGREE_RANK["Intermediate"] == 7

    def test_submicro_micro_rank_positive(self):
        """回歸:鎖 track1.py 缺 SubMicro/Micro 落 0 與 unknown 撞(本 PR 修為 1/2)。"""
        assert DEGREE_RANK["SubMicro"] > 0
        assert DEGREE_RANK["Micro"] > 0
        assert DEGREE_RANK["SubMicro"] == 1
        assert DEGREE_RANK["Micro"] == 2


# ────────────────────────────────────────────────────────────
# degree_rank() defensive fallback
# ────────────────────────────────────────────────────────────


class TestDegreeRank:
    def test_unknown_returns_zero(self):
        assert degree_rank("NotARealDegree") == 0

    def test_none_returns_zero(self):
        assert degree_rank(None) == 0

    def test_empty_string_returns_zero(self):
        assert degree_rank("") == 0

    def test_all_canonical_labels_resolve(self):
        for label in DEGREE_ORDER:
            assert degree_rank(label) == DEGREE_RANK[label]


# ────────────────────────────────────────────────────────────
# classify_degree_by_years bracket(對齊 Rust degree/mod.rs::classify_degree)
# ────────────────────────────────────────────────────────────


class TestClassifyBracketBoundaries:
    def test_just_under_one_year_is_subminuette(self):
        assert classify_degree_by_years(0.99) == "SubMinuette"

    def test_exactly_one_year_is_minute(self):
        assert classify_degree_by_years(1.0) == "Minute"

    def test_exactly_three_years_is_minor(self):
        assert classify_degree_by_years(3.0) == "Minor"

    def test_exactly_ten_years_is_primary(self):
        assert classify_degree_by_years(10.0) == "Primary"

    def test_exactly_thirty_years_is_cycle(self):
        assert classify_degree_by_years(30.0) == "Cycle"

    def test_exactly_one_hundred_years_is_supercycle(self):
        assert classify_degree_by_years(100.0) == "Supercycle"

    def test_below_threshold_consistent(self):
        # 內部點(非邊界)
        assert classify_degree_by_years(0.5) == "SubMinuette"
        assert classify_degree_by_years(2.0) == "Minute"
        assert classify_degree_by_years(5.5) == "Minor"
        assert classify_degree_by_years(20.0) == "Primary"
        assert classify_degree_by_years(50.0) == "Cycle"
        assert classify_degree_by_years(200.0) == "Supercycle"


# ────────────────────────────────────────────────────────────
# classify_degree_by_years 死碼:Minuette / Micro / SubMicro 永不出現
# ────────────────────────────────────────────────────────────


class TestClassifyDeadCodes:
    @pytest.mark.parametrize("years", [
        0.001, 0.05, 0.1, 0.3, 0.5, 0.7, 0.99,    # < 1y 區段
        1.0, 1.5, 2.0, 2.999,                       # 1-3y
        3.0, 5.0, 7.5, 9.999,                       # 3-10y
        10.0, 15.0, 25.0, 29.999,                   # 10-30y
        30.0, 50.0, 75.0, 99.999,                   # 30-100y
        100.0, 150.0, 500.0, 1000.0,                # ≥ 100y
    ])
    def test_classify_never_emits_dead_codes(self, years: float):
        """producer 死碼:NEoWave classify 永不回 Minuette / Micro / SubMicro。

        對齊 rust_compute/cores/wave/neely_core/src/degree/mod.rs:
        enum 保留 Minuette/Micro/SubMicro variant,但 classify_degree 在所有
        年份區間永遠不產(producer-side dead code)。
        """
        result = classify_degree_by_years(years)
        assert result not in ("Minuette", "Micro", "SubMicro")


# ────────────────────────────────────────────────────────────
# effective_degree(scenario)
# ────────────────────────────────────────────────────────────


def _make_scenario(span_days: int | None = None) -> dict:
    """build scenario dict with wave_tree.start / end (or omit for None test)。"""
    if span_days is None:
        return {}
    base = date(2024, 1, 1)
    return {
        "wave_tree": {
            "start": base.isoformat(),
            "end": (base + timedelta(days=span_days)).isoformat(),
        }
    }


class TestEffectiveDegree:
    def test_no_wave_tree_returns_none(self):
        assert effective_degree({}) is None

    def test_missing_end_returns_none(self):
        assert effective_degree({"wave_tree": {"start": "2024-01-01"}}) is None

    def test_zero_span_returns_none(self):
        scenario = {"wave_tree": {"start": "2024-01-01", "end": "2024-01-01"}}
        assert effective_degree(scenario) is None

    def test_reverse_span_returns_none(self):
        # end before start
        scenario = {"wave_tree": {"start": "2024-06-01", "end": "2024-01-01"}}
        assert effective_degree(scenario) is None

    def test_under_1y_returns_subminuette(self):
        s = _make_scenario(span_days=200)  # ~0.55y
        assert effective_degree(s) == "SubMinuette"

    def test_one_year_plus_returns_minute(self):
        s = _make_scenario(span_days=500)  # ~1.37y
        assert effective_degree(s) == "Minute"

    def test_decade_returns_primary(self):
        s = _make_scenario(span_days=4000)  # ~10.95y
        assert effective_degree(s) == "Primary"

    def test_century_returns_supercycle(self):
        s = _make_scenario(span_days=40000)  # ~109.5y
        assert effective_degree(s) == "Supercycle"
