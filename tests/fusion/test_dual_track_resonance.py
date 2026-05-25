"""Tests for src/fusion/dual_track/resonance.py — 關係層判定。

對齊 m3Spec/dual_track_resonance.md §三 + §四 + §五。
"""

from __future__ import annotations

import sys
from datetime import date
from pathlib import Path
from unittest.mock import patch

import pytest

_REPO_ROOT = Path(__file__).resolve().parent.parent.parent
_SRC_ROOT = _REPO_ROOT / "src"
for p in (str(_SRC_ROOT), str(_REPO_ROOT)):
    if p not in sys.path:
        sys.path.insert(0, p)

from fusion.dual_track._shared import (  # noqa: E402
    FibLine,
    Track1View,
    Track2Band,
    Track2View,
)
from fusion.dual_track.resonance import judge_fib_line, resonance  # noqa: E402


def _band(*, low, high, point, h=63, source="fusion", current_price=100.0):
    width = high - low
    wr = width / current_price if current_price > 0 else None
    return Track2Band(
        horizon_days=h, confidence=0.80,
        lower=low, upper=high, point=point,
        source_core=source,
        width_ratio=wr,
        is_overly_wide=(wr is not None and wr > 0.30),
    )


def _fib(price=100.0, label="0.618"):
    return FibLine(price=price, low=price - 1, high=price + 1, label=label,
                    source_ratio=0.618)


# ─── A-1 三級判定 + cross_stock 升振 ──────────────────────────────────────────


class TestJudgeFibLine:
    def test_divergence_no_band(self):
        f = _fib(price=100.0)
        res = judge_fib_line(
            fib_line=f, primary_band=None, current_price=100.0,
            is_top_30=False, all_bands={},
        )
        assert res.level == "divergence"
        assert res.cross_stock_boost is False
        assert res.t1_horizon is None

    def test_divergence_band_does_not_cover(self):
        f = _fib(price=100.0)
        band = _band(low=80.0, high=90.0, point=85.0)  # 不涵蓋 100
        res = judge_fib_line(
            fib_line=f, primary_band=band, current_price=100.0,
            is_top_30=False, all_bands={63: band},
        )
        assert res.level == "divergence"
        assert res.band_covers is False

    def test_divergence_band_overly_wide(self):
        """band 過寬(>30%)→ 抑制判定為 divergence(防呆)。"""
        f = _fib(price=100.0)
        band = _band(low=70.0, high=130.0, point=100.0)  # width 60/100=60% > 30%
        assert band.is_overly_wide is True
        res = judge_fib_line(
            fib_line=f, primary_band=band, current_price=100.0,
            is_top_30=False, all_bands={63: band},
        )
        assert res.level == "divergence"
        assert any("overly wide" in n for n in res.notes)

    def test_basic_covers_no_median_close(self):
        """band 涵蓋 + 中位距 fib 線遠 → basic。"""
        f = _fib(price=100.0)
        # band [90, 110] 涵蓋 100;point=110 距 100 為 10/100=10% > 2% tolerance
        band = _band(low=90.0, high=110.0, point=110.0)
        res = judge_fib_line(
            fib_line=f, primary_band=band, current_price=100.0,
            is_top_30=False, all_bands={63: band},
        )
        assert res.level == "basic"
        assert res.band_covers is True
        assert res.median_close is False

    def test_basic_covers_median_close_no_top30(self):
        """涵蓋 + 中位貼近 + is_top_30=False → basic(未命中不扣分,但不升強)。"""
        f = _fib(price=100.0)
        # point=100.5 距 100 為 0.5/100=0.5% < 2%
        band = _band(low=90.0, high=110.0, point=100.5)
        res = judge_fib_line(
            fib_line=f, primary_band=band, current_price=100.0,
            is_top_30=False, all_bands={63: band},
        )
        assert res.level == "basic"
        assert res.median_close is True
        assert res.cross_stock_boost is False

    def test_strong_covers_median_close_top30(self):
        """三條件齊備 → strong。"""
        f = _fib(price=100.0)
        band = _band(low=90.0, high=110.0, point=100.5)
        res = judge_fib_line(
            fib_line=f, primary_band=band, current_price=100.0,
            is_top_30=True, all_bands={63: band},
        )
        assert res.level == "strong"
        assert res.cross_stock_boost is True

    def test_top30_alone_does_not_upgrade_to_strong(self):
        """is_top_30=True 但 median 不貼近 → 仍是 basic(不仲裁分歧也不獨升強)。"""
        f = _fib(price=100.0)
        band = _band(low=90.0, high=110.0, point=108.0)  # 中位距 fib 線 8/100=8% >> 2%
        res = judge_fib_line(
            fib_line=f, primary_band=band, current_price=100.0,
            is_top_30=True, all_bands={63: band},
        )
        assert res.level == "basic"
        assert res.cross_stock_boost is False

    def test_top30_does_not_arbitrate_divergence(self):
        """is_top_30 不在分歧時介入(spec 明文)。"""
        f = _fib(price=100.0)
        band = _band(low=80.0, high=90.0, point=85.0)
        res = judge_fib_line(
            fib_line=f, primary_band=band, current_price=100.0,
            is_top_30=True, all_bands={63: band},
        )
        assert res.level == "divergence"
        assert res.cross_stock_boost is False


# ─── T1 / T2 標註 ────────────────────────────────────────────────────────────


class TestTimeAnnotation:
    def test_t1_picks_tightest_band(self):
        """三個 horizon 都涵蓋 → t1_horizon = width 最小者。"""
        f = _fib(price=100.0)
        b21 = _band(low=95.0, high=105.0, point=100.0, h=21)   # width 10
        b63 = _band(low=90.0, high=110.0, point=100.0, h=63)   # width 20
        b126 = _band(low=85.0, high=115.0, point=100.0, h=126) # width 30
        res = judge_fib_line(
            fib_line=f, primary_band=b63, current_price=100.0,
            is_top_30=False, all_bands={21: b21, 63: b63, 126: b126},
        )
        # primary level basic (covers + median close)
        assert res.level == "basic"
        # t1 應取最緊的 21
        assert res.t1_horizon == 21

    def test_t2_profile_per_horizon(self):
        f = _fib(price=100.0)
        b21 = _band(low=95.0, high=105.0, point=100.0, h=21)   # 涵蓋 + 中位貼近
        b63 = _band(low=80.0, high=90.0, point=85.0, h=63)    # 不涵蓋
        b126 = _band(low=85.0, high=115.0, point=110.0, h=126) # 涵蓋但中位遠
        res = judge_fib_line(
            fib_line=f, primary_band=b21, current_price=100.0,
            is_top_30=False, all_bands={21: b21, 63: b63, 126: b126},
        )
        assert res.t2_profile[21] == "basic_median_close"
        assert res.t2_profile[63] == "divergence"
        assert res.t2_profile[126] == "basic"


# ─── resonance() 整合 ────────────────────────────────────────────────────────


def _make_track1_view(
    *,
    has_snapshot=True, invalidated=False, fib_lines=None, direction="bullish",
    invalidation_price=None,
):
    return Track1View(
        stock_id="2330", as_of=date(2024, 6, 1),
        snapshot_date=date(2024, 5, 30) if has_snapshot else None,
        has_snapshot=has_snapshot,
        pattern_type="Impulse" if has_snapshot else None,
        power_rating="StrongBullish" if has_snapshot else None,
        direction=direction,
        effective_degree="Minute",
        wave_count=5,
        fib_lines=fib_lines or [],
        invalidation_price=invalidation_price,
        invalidated=invalidated,
    )


def _make_track2_view(*, primary_band=None, horizons=None, current_price=100.0):
    return Track2View(
        stock_id="2330", as_of=date(2024, 6, 1),
        current_price=current_price,
        primary_horizon=63, primary_confidence=0.80,
        primary_band=primary_band,
        horizons=horizons or ({63: primary_band} if primary_band else {}),
    )


class TestResonanceIntegration:
    def test_a3_gate_triggers_single_track_mode(self):
        t1 = _make_track1_view(
            fib_lines=[_fib(price=100.0)],
            invalidated=True,
            invalidation_price=80.0,
        )
        t2 = _make_track2_view(primary_band=_band(low=90, high=110, point=100))
        with patch("fusion.dual_track.resonance.read_track1", return_value=t1), \
             patch("fusion.dual_track.resonance.read_track2", return_value=t2), \
             patch("fusion.dual_track.resonance.fetch_is_top_30",
                   return_value=(False, None)), \
             patch("fusion.dual_track.resonance.fetch_latest_close",
                   return_value={"close": 75.0}), \
             patch("fusion.dual_track.resonance.get_connection"):
            res = resonance("2330", date(2024, 6, 1), conn=object())

        assert res.single_track_mode is True
        assert res.findings == []  # 不顯示共振
        assert any("A-3 invalidation gate" in n for n in res.notes)

    def test_no_snapshot_skips_judgement(self):
        t1 = _make_track1_view(has_snapshot=False)
        t2 = _make_track2_view()
        with patch("fusion.dual_track.resonance.read_track1", return_value=t1), \
             patch("fusion.dual_track.resonance.read_track2", return_value=t2), \
             patch("fusion.dual_track.resonance.fetch_is_top_30",
                   return_value=(False, None)), \
             patch("fusion.dual_track.resonance.fetch_latest_close",
                   return_value={"close": 100.0}), \
             patch("fusion.dual_track.resonance.get_connection"):
            res = resonance("2330", date(2024, 6, 1), conn=object())

        assert res.findings == []
        assert any("track1 unavailable" in n for n in res.notes)

    def test_full_path_basic_resonance(self):
        f1 = _fib(price=100.0)
        f2 = _fib(price=120.0)
        t1 = _make_track1_view(fib_lines=[f1, f2])
        b63 = _band(low=90.0, high=110.0, point=100.0, h=63)
        t2 = _make_track2_view(primary_band=b63, horizons={63: b63})
        with patch("fusion.dual_track.resonance.read_track1", return_value=t1), \
             patch("fusion.dual_track.resonance.read_track2", return_value=t2), \
             patch("fusion.dual_track.resonance.fetch_is_top_30",
                   return_value=(False, None)), \
             patch("fusion.dual_track.resonance.fetch_latest_close",
                   return_value={"close": 100.0}), \
             patch("fusion.dual_track.resonance.get_connection"):
            res = resonance("2330", date(2024, 6, 1), conn=object())

        assert res.single_track_mode is False
        assert len(res.findings) == 2
        # f1 (100) 在 band 內 + median 貼近 → basic(無 is_top_30)
        assert res.findings[0].level == "basic"
        # f2 (120) 在 band 外 → divergence
        assert res.findings[1].level == "divergence"

    def test_full_path_strong_with_top30(self):
        f = _fib(price=100.0)
        t1 = _make_track1_view(fib_lines=[f])
        b63 = _band(low=90.0, high=110.0, point=100.5, h=63)
        t2 = _make_track2_view(primary_band=b63, horizons={63: b63})
        with patch("fusion.dual_track.resonance.read_track1", return_value=t1), \
             patch("fusion.dual_track.resonance.read_track2", return_value=t2), \
             patch("fusion.dual_track.resonance.fetch_is_top_30",
                   return_value=(True, date(2024, 5, 31))), \
             patch("fusion.dual_track.resonance.fetch_latest_close",
                   return_value={"close": 100.0}), \
             patch("fusion.dual_track.resonance.get_connection"):
            res = resonance("2330", date(2024, 6, 1), conn=object())

        assert res.is_top_30 is True
        assert res.is_top_30_date == date(2024, 5, 31)
        assert res.findings[0].level == "strong"
        assert res.findings[0].cross_stock_boost is True

    def test_to_dict_serializable(self):
        """確保 DualTrackResult.to_dict() 完整可序列化(MCP layer 需要)。"""
        f = _fib(price=100.0)
        t1 = _make_track1_view(fib_lines=[f])
        b63 = _band(low=90.0, high=110.0, point=100.5, h=63)
        t2 = _make_track2_view(primary_band=b63, horizons={63: b63})
        with patch("fusion.dual_track.resonance.read_track1", return_value=t1), \
             patch("fusion.dual_track.resonance.read_track2", return_value=t2), \
             patch("fusion.dual_track.resonance.fetch_is_top_30",
                   return_value=(False, None)), \
             patch("fusion.dual_track.resonance.fetch_latest_close",
                   return_value={"close": 100.0}), \
             patch("fusion.dual_track.resonance.get_connection"):
            res = resonance("2330", date(2024, 6, 1), conn=object())

        d = res.to_dict()
        assert d["stock_id"] == "2330"
        assert d["as_of"] == "2024-06-01"
        assert "track1" in d and "track2" in d
        assert "findings" in d and len(d["findings"]) == 1
        # 純 Python 原生型別,可被 json.dumps
        import json
        json.dumps(d)  # 不 raise


# ─── public API surface ──────────────────────────────────────────────────────


class TestPublicSurface:
    def test_dual_track_exports(self):
        from fusion import dual_track

        assert hasattr(dual_track, "resonance")
        assert hasattr(dual_track, "FibLine")
        assert hasattr(dual_track, "DualTrackResult")
        assert hasattr(dual_track, "Track1View")
        assert hasattr(dual_track, "Track2View")
