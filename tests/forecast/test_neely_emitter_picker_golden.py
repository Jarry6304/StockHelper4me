"""B1:neely_emitter write-side picker golden + contract tests.

對齊 b1-degree-consolidation skill「驗收標準」 golden test 案例:
- 不變-1 / 不變-2:contract pass(B1 後 primary 選擇對齊舊行為)
- sub-year lump:**xfail strict**(degree lump → power/rules tiebreak,primary 翻)
- 寫入面 filter:**xfail strict**(舊版無 filter 誤選失效;新版過濾後翻)
- 寫入面 + stale:contract pass(stale gate skip 寫入)
- neutral 不濾(via canonical_is_invalidated)— 已在 test_picker_invalidation.py 覆蓋
"""

from __future__ import annotations

import sys
from datetime import date, timedelta
from pathlib import Path
from unittest.mock import patch

import pytest

_REPO_ROOT = Path(__file__).resolve().parent.parent.parent
_SRC_ROOT = _REPO_ROOT / "src"
for _p in (str(_SRC_ROOT), str(_REPO_ROOT)):
    if _p not in sys.path:
        sys.path.insert(0, _p)

from forecast.neely_emitter import (  # noqa: E402
    _pick_primary,
    emit_neely_fib,
)


# ────────────────────────────────────────────────────────────
# Scenario fixtures
# ────────────────────────────────────────────────────────────


def _scenario(
    *,
    sid: str,
    span_days: int,
    power: str = "Bullish",
    rules_passed: int = 5,
    invalidation_below: float | None = None,
    invalidation_above: float | None = None,
    fib_zones: list | None = None,
) -> dict:
    """Build forest scenario dict with unique sid as `structure_label` for primary identification."""
    base = date(2024, 1, 1)
    triggers = []
    if invalidation_below is not None:
        triggers.append({
            "on_trigger": "InvalidateScenario",
            "trigger_type": {"PriceBreakBelow": invalidation_below},
        })
    if invalidation_above is not None:
        triggers.append({
            "on_trigger": "InvalidateScenario",
            "trigger_type": {"PriceBreakAbove": invalidation_above},
        })
    return {
        "structure_label": sid,
        "pattern_type": "Impulse",
        "power_rating": power,
        "rules_passed_count": rules_passed,
        "wave_tree": {
            "start": base.isoformat(),
            "end": (base + timedelta(days=span_days)).isoformat(),
        },
        "invalidation_triggers": triggers,
        "expected_fib_zones": fib_zones or [
            {"label": "0.5", "low": 90.0, "high": 100.0},
        ],
    }


# ────────────────────────────────────────────────────────────
# Contract:degree-aware sort 不破舊「不變」case
# ────────────────────────────────────────────────────────────


class TestContractInvariantPrimary:
    def test_invariant_1_long_minor_vs_short_bull(self):
        """8y Minor bull(rank 6)+ 0.6y bull(rank 3 SubMinuette)→ Minor 勝。"""
        long_minor = _scenario(sid="LONG_MINOR", span_days=8 * 365)  # ~8y → Minor
        short_bull = _scenario(sid="SHORT_BULL", span_days=int(0.6 * 365))  # ~0.6y → SubMinuette
        primary = _pick_primary([long_minor, short_bull])
        assert primary is not None
        assert primary["structure_label"] == "LONG_MINOR"

    def test_invariant_2_primary_vs_minor(self):
        """16y Primary + 4y Minor → Primary 勝。"""
        primary_scn = _scenario(sid="PRIMARY", span_days=16 * 365)  # ~16y → Primary
        minor_scn = _scenario(sid="MINOR", span_days=4 * 365)  # ~4y → Minor
        primary = _pick_primary([primary_scn, minor_scn])
        assert primary is not None
        assert primary["structure_label"] == "PRIMARY"


# ────────────────────────────────────────────────────────────
# xfail strict:行為變更(已知,接受;XPASS → CI 紅)
# ────────────────────────────────────────────────────────────


class TestXfailBehaviorChanges:
    @pytest.mark.xfail(
        strict=True,
        reason=(
            "B1 行為變更 (a):sub-year 平手 forest(degree 都 lump 到 SubMinuette)"
            "→ 落 power_rating_strength + rules_passed 二級 tiebreak。舊版 track1/"
            "neely_emitter degree 拆 Subminuette(0.3y 切點)vs Minuette(1y 切點)"
            "讓 0.2y 與 0.5y 不同 rank → 0.5y 永勝;新 canonical 統一 < 1y 全 "
            "SubMinuette → 0.2y StrongBullish(power=3)勝 0.5y Bullish(power=2)。"
        ),
    )
    def test_sub_year_lump_flips_primary_to_higher_power(self):
        # 都 < 1y → 都 SubMinuette → degree rank 同 → 落 power tiebreak
        old_winner = _scenario(
            sid="OLD_WINNER", span_days=int(0.5 * 365), power="Bullish",  # power=2
        )
        new_winner = _scenario(
            sid="NEW_WINNER", span_days=int(0.2 * 365), power="StrongBullish",  # power=3
        )
        # 期望 B1 後 NEW_WINNER 勝(power=3 > power=2);記做 xfail 表示「期望
        # primary 是 NEW_WINNER」— 若哪天又翻回 OLD_WINNER,xfail strict XPASS = 紅
        primary = _pick_primary([old_winner, new_winner])
        assert primary is not None
        # 寫成「斷言 primary == OLD_WINNER」+ xfail strict,意即「我們預期
        # 此斷言失敗」— 即 B1 後 primary != OLD_WINNER(實際是 NEW_WINNER)
        assert primary["structure_label"] == "OLD_WINNER"

    @pytest.mark.xfail(
        strict=True,
        reason=(
            "B1 行為變更 (b):寫入面 filter — 2y Bullish(Minute degree)已失效"
            "(PriceBreakBelow=100,current=95)+ 0.5y Bullish 安全。舊版 _pick_primary"
            "無 current_price filter → 永遠選 degree 高的 2y;新版 canonical_is_invalidated"
            "filter 後只剩 0.5y。"
        ),
    )
    def test_invalidated_high_degree_filtered_out(self):
        invalidated_high = _scenario(
            sid="HIGH_INVALID",
            span_days=2 * 365,  # Minute(rank 5)
            power="Bullish",
            invalidation_below=100.0,
        )
        safe_low = _scenario(
            sid="LOW_SAFE",
            span_days=int(0.5 * 365),  # SubMinuette(rank 3)
            power="Bullish",
            invalidation_below=80.0,  # current=95 > 80,safe
        )
        primary = _pick_primary([invalidated_high, safe_low], current_price=95.0)
        # 期望 B1 後 LOW_SAFE 勝(HIGH_INVALID 被 filter)
        # 斷言 primary == HIGH_INVALID + xfail strict → 期望失敗(即 primary 是 LOW_SAFE)
        assert primary is not None
        assert primary["structure_label"] == "HIGH_INVALID"


# ────────────────────────────────────────────────────────────
# Contract:emit_neely_fib stale gate + current_price filter
# ────────────────────────────────────────────────────────────


class TestStaleGate:
    def test_stale_snapshot_skips_write(self):
        """snapshot 8 days old + threshold=7 → status='stale_snapshot' + 不 upsert。"""
        primary = _scenario(sid="STALE", span_days=400, fib_zones=[
            {"label": "0.5", "low": 90.0, "high": 100.0},
        ])
        snap = {
            "snapshot_date": date(2026, 5, 18),  # 8 days before asof=2026-05-26
            "snapshot": {"scenario_forest": [primary]},
        }
        upserts = []
        with patch("forecast.neely_emitter._fetch_latest_neely_snapshot", return_value=snap), \
             patch("forecast.neely_emitter.upsert_forecast",
                   side_effect=lambda conn, row: upserts.append(row)):
            res = emit_neely_fib(
                None, "2330", date(2026, 5, 26),
                stale_threshold_days=7,
                current_price=95.0,
            )
        assert res["status"] == "stale_snapshot"
        assert res["skipped"] is True
        assert res["age_days"] == 8
        assert res["stale_threshold_days"] == 7
        assert res["zones_emitted"] == 0
        # 最重要:不寫 forecast_log
        assert upserts == []

    def test_fresh_snapshot_writes_normally(self):
        """snapshot 3 days old < threshold=7 → 正常寫入。"""
        primary = _scenario(sid="FRESH", span_days=400, fib_zones=[
            {"label": "0.5", "low": 90.0, "high": 100.0},
        ])
        snap = {
            "snapshot_date": date(2026, 5, 23),  # 3 days before asof
            "snapshot": {"scenario_forest": [primary]},
        }
        upserts = []
        with patch("forecast.neely_emitter._fetch_latest_neely_snapshot", return_value=snap), \
             patch("forecast.neely_emitter.upsert_forecast",
                   side_effect=lambda conn, row: upserts.append(row)):
            res = emit_neely_fib(
                None, "2330", date(2026, 5, 26),
                stale_threshold_days=7,
                current_price=95.0,
            )
        assert res["status"] == "written"
        assert len(upserts) == 1

    def test_stale_gate_disabled_when_threshold_zero(self):
        """stale_threshold_days=0 → gate disabled(intra-day 用例)。"""
        primary = _scenario(sid="ANCIENT", span_days=400, fib_zones=[
            {"label": "0.5", "low": 90.0, "high": 100.0},
        ])
        snap = {
            "snapshot_date": date(2025, 1, 1),  # very old
            "snapshot": {"scenario_forest": [primary]},
        }
        upserts = []
        with patch("forecast.neely_emitter._fetch_latest_neely_snapshot", return_value=snap), \
             patch("forecast.neely_emitter.upsert_forecast",
                   side_effect=lambda conn, row: upserts.append(row)):
            res = emit_neely_fib(
                None, "2330", date(2026, 5, 26),
                stale_threshold_days=0,
                current_price=95.0,
            )
        assert res["status"] == "written"
        assert len(upserts) == 1


class TestCurrentPriceInPickerFlow:
    def test_all_invalidated_returns_status(self):
        """全部 scenario 失效 → status='all_invalidated',不 upsert。"""
        s1 = _scenario(
            sid="S1", span_days=400, power="Bullish", invalidation_below=100.0,
            fib_zones=[{"label": "0.5", "low": 90.0, "high": 100.0}],
        )
        s2 = _scenario(
            sid="S2", span_days=300, power="Bullish", invalidation_below=110.0,
            fib_zones=[{"label": "0.5", "low": 90.0, "high": 100.0}],
        )
        snap = {
            "snapshot_date": date(2026, 5, 25),
            "snapshot": {"scenario_forest": [s1, s2]},
        }
        upserts = []
        with patch("forecast.neely_emitter._fetch_latest_neely_snapshot", return_value=snap), \
             patch("forecast.neely_emitter.upsert_forecast",
                   side_effect=lambda conn, row: upserts.append(row)):
            res = emit_neely_fib(
                None, "2330", date(2026, 5, 26),
                current_price=95.0,  # < both thresholds (100, 110)
            )
        assert res["status"] == "all_invalidated"
        assert res["zones_emitted"] == 0
        assert upserts == []

    def test_current_price_none_skips_filter(self):
        """current_price=None → 跳過 filter,沿用既有行為(可能選 invalidated scenario)。"""
        s_invalid = _scenario(
            sid="INVALID", span_days=2 * 365, power="Bullish",  # Minute
            invalidation_below=100.0,
            fib_zones=[{"label": "0.5", "low": 90.0, "high": 100.0}],
        )
        s_safe = _scenario(
            sid="SAFE", span_days=int(0.5 * 365), power="Bullish",  # SubMinuette
            invalidation_below=80.0,
            fib_zones=[{"label": "0.5", "low": 90.0, "high": 100.0}],
        )
        snap = {
            "snapshot_date": date(2026, 5, 25),
            "snapshot": {"scenario_forest": [s_invalid, s_safe]},
        }
        upserts = []
        with patch("forecast.neely_emitter._fetch_latest_neely_snapshot", return_value=snap), \
             patch("forecast.neely_emitter.upsert_forecast",
                   side_effect=lambda conn, row: upserts.append(row)):
            res = emit_neely_fib(
                None, "2330", date(2026, 5, 26),
                current_price=None,  # filter skipped
            )
        # 沒 filter → INVALID(Minute degree)勝 SAFE(SubMinuette)
        assert res["status"] == "written"
        assert len(upserts) == 1
