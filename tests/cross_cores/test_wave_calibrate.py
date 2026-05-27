"""Tests for src/cross_cores/wave_impulse_calibrate.py — 2A hygiene calibration.

對齊 b1 plan §2A:
- build_threshold_combos cartesian 正確
- build_date_series 日序產生
- aggregate_hygiene_metrics 對 sample rows 算 phase / RR / excluded 統計
- calibrate_hygiene 對 (date × combo) 矩陣展開 + 聚合
- compute_screen_at_date 透傳 thresholds(production 行為 0 變,thresholds=DEFAULT)
"""

from __future__ import annotations

import sys
from datetime import date, timedelta
from pathlib import Path
from unittest.mock import MagicMock, patch

import pytest

_REPO_ROOT = Path(__file__).resolve().parent.parent.parent
_SRC_ROOT = _REPO_ROOT / "src"
for _p in (str(_SRC_ROOT), str(_REPO_ROOT)):
    if _p not in sys.path:
        sys.path.insert(0, _p)

from cross_cores.wave_impulse_calibrate import (  # noqa: E402
    DEFAULT_SWEEP_RANGES,
    RR_OUTLIER_THRESHOLD,
    aggregate_hygiene_metrics,
    build_date_series,
    build_threshold_combos,
    calibrate_hygiene,
    samples_to_csv_rows,
)
from cross_cores.wave_impulse_screen import (  # noqa: E402
    DEFAULT_THRESHOLDS,
    PHASE_CORRECTION_DONE_DOWN,
    PHASE_CORRECTION_DONE_UP,
    PHASE_CORRECTION_ONGOING,
    PHASE_IMPULSE_COMPLETE,
    PHASE_OTHER,
    ScreenThresholds,
    compute_screen_at_date,
)


# ────────────────────────────────────────────────────────────
# build_threshold_combos
# ────────────────────────────────────────────────────────────


class TestBuildThresholdCombos:
    def test_no_overrides_returns_single_default(self):
        combos = build_threshold_combos(None)
        assert len(combos) == 1
        assert combos[0] == DEFAULT_THRESHOLDS

    def test_empty_dict_returns_single_default(self):
        combos = build_threshold_combos({})
        assert len(combos) == 1
        assert combos[0] == DEFAULT_THRESHOLDS

    def test_single_axis_sweep_produces_list(self):
        combos = build_threshold_combos({"rr_min": [1.0, 1.5, 2.0]})
        assert len(combos) == 3
        rr_values = [c.rr_min for c in combos]
        assert sorted(rr_values) == [1.0, 1.5, 2.0]
        # 其他 axis 應該等於 DEFAULT
        for c in combos:
            assert c.recent_days == DEFAULT_THRESHOLDS.recent_days
            assert c.max_upside_multiple == DEFAULT_THRESHOLDS.max_upside_multiple

    def test_two_axis_sweep_cartesian(self):
        combos = build_threshold_combos({
            "rr_min": [1.0, 2.0],
            "recent_days": [7, 14],
        })
        assert len(combos) == 4
        # 確認 cartesian
        pairs = {(c.rr_min, c.recent_days) for c in combos}
        assert pairs == {(1.0, 7), (1.0, 14), (2.0, 7), (2.0, 14)}

    def test_full_5_axis_sweep_cartesian(self):
        # 2 × 2 × 2 × 2 × 2 = 32 combos
        combos = build_threshold_combos({
            "recent_days": [7, 14],
            "rr_min": [1.0, 2.0],
            "max_upside_multiple": [1.5, 2.0],
            "correction_bottom_buffer": [0.02, 0.05],
            "min_upside_pct": [0.02, 0.05],
        })
        assert len(combos) == 32
        # 全部 combo 都該 unique
        assert len({(c.recent_days, c.rr_min, c.max_upside_multiple,
                     c.correction_bottom_buffer, c.min_upside_pct)
                    for c in combos}) == 32

    def test_default_sweep_ranges_well_formed(self):
        # 對 DEFAULT_SWEEP_RANGES 全 axis 跑一遍應該成功
        combos = build_threshold_combos(DEFAULT_SWEEP_RANGES)
        # 3 × 4 × 3 × 3 × 3 = 324
        assert len(combos) == 3 * 4 * 3 * 3 * 3
        # 都是 ScreenThresholds instance
        assert all(isinstance(c, ScreenThresholds) for c in combos)


# ────────────────────────────────────────────────────────────
# build_date_series
# ────────────────────────────────────────────────────────────


class TestBuildDateSeries:
    def test_single_day_range(self):
        d = date(2026, 5, 1)
        out = build_date_series(d, d, step_days=7)
        assert out == [d]

    def test_step_7_inclusive_end(self):
        out = build_date_series(date(2026, 5, 1), date(2026, 5, 22), step_days=7)
        assert out == [
            date(2026, 5, 1), date(2026, 5, 8),
            date(2026, 5, 15), date(2026, 5, 22),
        ]

    def test_step_1_daily(self):
        out = build_date_series(date(2026, 5, 1), date(2026, 5, 3), step_days=1)
        assert out == [date(2026, 5, 1), date(2026, 5, 2), date(2026, 5, 3)]

    def test_start_after_end_returns_empty(self):
        out = build_date_series(date(2026, 5, 10), date(2026, 5, 1), step_days=7)
        assert out == []

    def test_zero_step_raises(self):
        with pytest.raises(ValueError):
            build_date_series(date(2026, 5, 1), date(2026, 5, 10), step_days=0)

    def test_negative_step_raises(self):
        with pytest.raises(ValueError):
            build_date_series(date(2026, 5, 1), date(2026, 5, 10), step_days=-1)


# ────────────────────────────────────────────────────────────
# aggregate_hygiene_metrics
# ────────────────────────────────────────────────────────────


def _row(
    phase: str | None = None,
    is_candidate: bool = False,
    rr_ratio: float | None = None,
    excluded_reason: str | None = None,
    is_top_n: bool = False,
    cross_tf_aligned: bool = False,
) -> dict:
    return {
        "stock_id": "TEST", "date": date(2026, 5, 26), "timeframe": "daily",
        "phase": phase, "is_candidate": is_candidate, "rr_ratio": rr_ratio,
        "excluded_reason": excluded_reason, "is_top_n": is_top_n,
        "cross_tf_aligned": cross_tf_aligned,
    }


class TestAggregateHygieneMetrics:
    def test_empty_rows(self):
        m = aggregate_hygiene_metrics(
            [], as_of=date(2026, 5, 26), thresholds=DEFAULT_THRESHOLDS,
        )
        assert m["total_rows"] == 0
        assert m["candidates"] == 0
        assert m["rr_p50"] is None
        assert m["rr_p95"] is None
        assert m["rr_max"] is None
        assert m["excluded_top"] == []

    def test_thresholds_serialized_in_output(self):
        thresholds = ScreenThresholds(
            recent_days=21, rr_min=2.5, max_upside_multiple=1.5,
            correction_bottom_buffer=0.05, min_upside_pct=0.05,
        )
        m = aggregate_hygiene_metrics(
            [_row()], as_of=date(2026, 5, 26), thresholds=thresholds,
        )
        assert m["thresholds"]["recent_days"] == 21
        assert m["thresholds"]["rr_min"] == 2.5
        assert m["thresholds"]["max_upside_multiple"] == 1.5

    def test_phase_breakdown(self):
        rows = [
            _row(phase=PHASE_CORRECTION_DONE_DOWN, is_candidate=True, rr_ratio=2.0),
            _row(phase=PHASE_CORRECTION_DONE_DOWN, is_candidate=True, rr_ratio=3.0),
            _row(phase=PHASE_CORRECTION_DONE_UP, excluded_reason="bearish_setup"),
            _row(phase=PHASE_CORRECTION_ONGOING, excluded_reason="correction_stale"),
            _row(phase=PHASE_IMPULSE_COMPLETE, excluded_reason="impulse_complete_observe"),
            _row(phase=PHASE_OTHER, excluded_reason="non_corrective_pattern"),
            _row(phase=None, excluded_reason="no_snapshot"),
        ]
        m = aggregate_hygiene_metrics(rows, as_of=date(2026, 5, 26), thresholds=DEFAULT_THRESHOLDS)
        assert m["total_rows"] == 7
        assert m["candidates"] == 2
        assert m["phase_correction_down"] == 2
        assert m["phase_correction_up"] == 1
        assert m["phase_correction_ongoing"] == 1
        assert m["phase_impulse_complete"] == 1
        assert m["phase_other"] == 1
        assert m["phase_none"] == 1

    def test_rr_percentiles_and_outlier(self):
        rr_values = [1.5, 2.0, 3.0, 5.0, 10.0, 25.0]  # 25 > RR_OUTLIER_THRESHOLD(20)
        rows = [_row(
            phase=PHASE_CORRECTION_DONE_DOWN, is_candidate=True, rr_ratio=rr,
        ) for rr in rr_values]
        m = aggregate_hygiene_metrics(rows, as_of=date(2026, 5, 26), thresholds=DEFAULT_THRESHOLDS)
        assert m["rr_count"] == 6
        assert m["rr_max"] == 25.0
        assert m["rr_outlier_count"] == 1  # 25 > 20
        # p50 of [1.5, 2, 3, 5, 10, 25] = (3 + 5) / 2 = 4.0 with inclusive linear
        # Actually for 6 elements, idx=(6-1)*0.5=2.5 → between index 2 (3.0) and 3 (5.0) → 4.0
        assert m["rr_p50"] == pytest.approx(4.0)

    def test_rr_only_candidates(self):
        """非 candidate row 的 rr_ratio 不進 RR distribution."""
        rows = [
            _row(phase=PHASE_CORRECTION_DONE_DOWN, is_candidate=True, rr_ratio=2.0),
            _row(phase=PHASE_CORRECTION_DONE_DOWN, is_candidate=False,
                 excluded_reason="rr_below_threshold", rr_ratio=0.5),
        ]
        m = aggregate_hygiene_metrics(rows, as_of=date(2026, 5, 26), thresholds=DEFAULT_THRESHOLDS)
        assert m["rr_count"] == 1
        assert m["rr_max"] == 2.0

    def test_excluded_top_5_by_count(self):
        rows = (
            [_row(excluded_reason="no_snapshot")] * 100
            + [_row(excluded_reason="empty_forest")] * 50
            + [_row(excluded_reason="rr_below_threshold")] * 30
            + [_row(excluded_reason="no_target")] * 10
            + [_row(excluded_reason="upside_too_small")] * 5
            + [_row(excluded_reason="rare_reason")] * 1  # 不該進 top 5
        )
        m = aggregate_hygiene_metrics(rows, as_of=date(2026, 5, 26), thresholds=DEFAULT_THRESHOLDS)
        assert len(m["excluded_top"]) == 5
        top_reasons = [e["reason"] for e in m["excluded_top"]]
        assert "rare_reason" not in top_reasons
        # 排序 by count DESC
        assert m["excluded_top"][0]["reason"] == "no_snapshot"
        assert m["excluded_top"][0]["count"] == 100

    def test_cross_tf_and_top_n_counts(self):
        rows = [
            _row(is_candidate=True, cross_tf_aligned=True, is_top_n=True),
            _row(is_candidate=True, cross_tf_aligned=True, is_top_n=False),
            _row(is_candidate=True, cross_tf_aligned=False, is_top_n=True),
            _row(is_candidate=False, cross_tf_aligned=True, is_top_n=False),  # not counted
        ]
        m = aggregate_hygiene_metrics(rows, as_of=date(2026, 5, 26), thresholds=DEFAULT_THRESHOLDS)
        assert m["candidates"] == 3
        assert m["candidates_top_n"] == 2
        assert m["candidates_cross_tf_aligned"] == 2


# ────────────────────────────────────────────────────────────
# calibrate_hygiene + compute_screen_at_date wiring(integration via mock db)
# ────────────────────────────────────────────────────────────


def _mock_db_with_snapshot(scenarios_per_tf: dict[str, list[dict]] | None = None):
    """Build mock db.query that responds correctly to wave_impulse_screen SQL."""
    if scenarios_per_tf is None:
        scenarios_per_tf = {}

    snapshot_date = date(2026, 5, 25)
    snapshot_rows = []
    for tf, scenarios in scenarios_per_tf.items():
        snapshot_rows.append({
            "stock_id": "2330", "snapshot_date": snapshot_date,
            "timeframe": tf,
            "snapshot": {"scenario_forest": scenarios},
        })

    def _query(sql, params=None):
        s = sql.lower()
        if "stock_info_ref" in s:
            return [{"stock_id": "2330", "industry_category": "半導體",
                     "delisting_date": None}]
        if "price_daily_fwd" in s:
            return [{"stock_id": "2330", "close": 100.0}]
        if "structural_snapshots" in s:
            return snapshot_rows
        return []

    db = MagicMock()
    db.query = MagicMock(side_effect=_query)
    return db


class TestCalibrateHygiene:
    def test_empty_asof_returns_empty(self):
        db = _mock_db_with_snapshot()
        out = calibrate_hygiene(db, asof_dates=[])
        assert out == []

    def test_single_date_default_threshold(self):
        db = _mock_db_with_snapshot({"daily": [], "weekly": [], "monthly": []})
        out = calibrate_hygiene(
            db, asof_dates=[date(2026, 5, 26)],
        )
        assert len(out) == 1
        sample = out[0]
        assert sample["as_of"] == "2026-05-26"
        # default thresholds 對齊
        assert sample["thresholds"]["rr_min"] == DEFAULT_THRESHOLDS.rr_min

    def test_no_snapshots_at_or_before_asof(self):
        # mock db query 對 structural_snapshots 回 [] → calibrate 應加 skipped sample
        db = MagicMock()
        def _query(sql, _params=None):
            return []
        db.query = MagicMock(side_effect=_query)

        out = calibrate_hygiene(db, asof_dates=[date(2025, 1, 1)])
        assert len(out) == 1
        assert out[0]["total_rows"] == 0
        assert out[0]["skipped_reason"] == "no_snapshots_at_or_before_asof"

    def test_matrix_dates_x_combos(self):
        db = _mock_db_with_snapshot({"daily": [], "weekly": [], "monthly": []})
        combos = build_threshold_combos({"rr_min": [1.0, 2.0]})
        dates = build_date_series(
            date(2026, 5, 26), date(2026, 5, 26) + timedelta(days=7), step_days=7,
        )
        out = calibrate_hygiene(db, asof_dates=dates, threshold_combos=combos)
        # 2 dates × 2 combos = 4 samples
        assert len(out) == 4
        # rr_min values present
        rr_mins = sorted({s["thresholds"]["rr_min"] for s in out})
        assert rr_mins == [1.0, 2.0]


# ────────────────────────────────────────────────────────────
# samples_to_csv_rows
# ────────────────────────────────────────────────────────────


class TestSamplesToCsvRows:
    def test_flattens_thresholds(self):
        sample = aggregate_hygiene_metrics(
            [], as_of=date(2026, 5, 26),
            thresholds=ScreenThresholds(rr_min=2.5, recent_days=21),
        )
        flat = samples_to_csv_rows([sample])
        assert len(flat) == 1
        row = flat[0]
        # thresholds 展平為 thr_xxx 欄
        assert "thresholds" not in row
        assert row["thr_rr_min"] == 2.5
        assert row["thr_recent_days"] == 21

    def test_excluded_top_as_json_string(self):
        rows = [_row(excluded_reason="no_target")] * 3
        sample = aggregate_hygiene_metrics(
            rows, as_of=date(2026, 5, 26), thresholds=DEFAULT_THRESHOLDS,
        )
        flat = samples_to_csv_rows([sample])
        assert len(flat) == 1
        # excluded_top → excluded_top_json string
        assert "excluded_top" not in flat[0]
        assert "excluded_top_json" in flat[0]
        # JSON-parseable
        import json
        decoded = json.loads(flat[0]["excluded_top_json"])
        assert decoded[0]["reason"] == "no_target"
        assert decoded[0]["count"] == 3


# ────────────────────────────────────────────────────────────
# compute_screen_at_date 透傳 thresholds + as_of 行為
# ────────────────────────────────────────────────────────────


class TestComputeScreenAtDateContract:
    def test_default_thresholds_no_target_date_matches_run_behavior(self):
        """compute_screen_at_date(db) 不傳 target_date → 推導最新 snapshot 為 target。
        對齊 production run() 行為。"""
        db = _mock_db_with_snapshot({"daily": [], "weekly": [], "monthly": []})
        rows, target = compute_screen_at_date(db)
        # 最新 snapshot = 2026-05-25(mock 設定)
        assert target == date(2026, 5, 25)

    def test_with_target_date_uses_it(self):
        db = _mock_db_with_snapshot({"daily": [], "weekly": [], "monthly": []})
        rows, target = compute_screen_at_date(db, target_date=date(2026, 5, 20))
        assert target == date(2026, 5, 20)

    def test_no_snapshots_returns_empty(self):
        db = MagicMock()
        db.query = MagicMock(return_value=[])
        rows, target = compute_screen_at_date(db)
        assert rows == []
