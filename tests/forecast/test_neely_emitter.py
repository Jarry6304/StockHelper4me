"""Tests for src/forecast/neely_emitter.py — judgment forward emitter(§8 S3)。

v4.39 起寫路徑 = `emit_judgment_forecast`(active judgment 才發,
`source_core='judgment'`);舊 `_pick_primary` / `emit_neely_fib` 已刪除
(picker 序列凍結唯讀),對應舊 picker 測試同步移除。
"""

from __future__ import annotations

import sys
from datetime import date, timedelta
from pathlib import Path
from unittest.mock import patch

import pytest

_REPO_ROOT = Path(__file__).resolve().parent.parent.parent
_SRC_ROOT = _REPO_ROOT / "src"
for p in (str(_SRC_ROOT), str(_REPO_ROOT)):
    if p not in sys.path:
        sys.path.insert(0, p)

from forecast.neely_emitter import (  # noqa: E402
    emit_judgment_forecast,
    _effective_degree,
    _scenario_horizon_days,
)
from fusion.judgment import scenario_anchor_key  # noqa: E402


def _make_scenario(
    *,
    pattern="Impulse",
    power="Strong",
    rules_passed=5,
    span_days=200,
    fib_zones=None,
):
    base = date(2024, 1, 1)
    return {
        "pattern_type": pattern,
        "power_rating": power,
        "rules_passed_count": rules_passed,
        "wave_tree": {
            "label": f"{pattern} L5↑",
            "base_label": ":5",
            "start": base.isoformat(),
            "end": (base + timedelta(days=span_days)).isoformat(),
        },
        "expected_fib_zones": fib_zones or [],
    }


def _make_judgment(scenario, *, jid=41, judged_by="human:jarry", role="preferred"):
    """active judgment,accepted[preferred] 錨到 scenario 的 anchor_key。"""
    return {
        "id": jid,
        "judged_by": judged_by,
        "accepted": [{"role": role, "anchor_key": scenario_anchor_key(scenario)}],
    }


class TestEffectiveDegree:
    def test_short_span_is_subminuette(self):
        # B1 後:< 1 yr → "SubMinuette"(canonical 對齊 Rust output.rs::Degree
        # enum 大小寫;舊版「Subminuette」小寫 'm' 為 producer-side label drift)
        s = _make_scenario(span_days=30)  # ~0.08 year
        assert _effective_degree(s) == "SubMinuette"

    def test_year_span_is_minute(self):
        s = _make_scenario(span_days=400)  # ~1.1 year
        assert _effective_degree(s) == "Minute"

    def test_decade_span_is_primary(self):
        s = _make_scenario(span_days=4000)  # ~11 years
        assert _effective_degree(s) == "Primary"

    def test_no_wave_tree_returns_none(self):
        assert _effective_degree({}) is None


class TestHorizonMapping:
    def test_subminuette_maps_to_21(self):
        s = _make_scenario(span_days=50)
        assert _scenario_horizon_days(s) == 21

    def test_minute_maps_to_63(self):
        s = _make_scenario(span_days=400)
        assert _scenario_horizon_days(s) == 63

    def test_minor_maps_to_126(self):
        s = _make_scenario(span_days=2000)  # ~5.5 years
        assert _scenario_horizon_days(s) == 126


class TestEmitJudgmentForecast:
    def test_no_active_judgment_no_write(self):
        with patch("fusion.judgment.fetch_active_judgment", return_value=None), \
             patch("forecast.neely_emitter.upsert_forecast") as upd:
            res = emit_judgment_forecast(None, "2330", date(2024, 6, 1))
        assert res["status"] == "no_active_judgment"
        assert res["zones_emitted"] == 0
        upd.assert_not_called()

    def test_no_fit_judgment_has_no_anchor(self):
        # no_fit 判讀(accepted=[])→ 無錨可發,不 upsert
        judgment = {"id": 7, "judged_by": "human:jarry", "accepted": []}
        with patch("fusion.judgment.fetch_active_judgment", return_value=judgment), \
             patch("forecast.neely_emitter.upsert_forecast") as upd:
            res = emit_judgment_forecast(None, "2330", date(2024, 6, 1))
        assert res["status"] == "no_active_judgment"
        assert res["judgment_id"] == 7
        upd.assert_not_called()

    def test_no_snapshot_returns_status(self):
        judgment = _make_judgment(_make_scenario())
        with patch("fusion.judgment.fetch_active_judgment", return_value=judgment), \
             patch("forecast.neely_emitter._fetch_latest_neely_snapshot",
                   return_value=None), \
             patch("forecast.neely_emitter.upsert_forecast") as upd:
            res = emit_judgment_forecast(None, "2330", date(2024, 6, 1))
        assert res["status"] == "no_snapshot"
        upd.assert_not_called()

    def test_stale_snapshot_skips_write(self):
        """snapshot 8 days old + threshold=7 → status='stale_snapshot' + 不 upsert。"""
        scenario = _make_scenario(
            span_days=400,
            fib_zones=[{"label": "0.5", "low": 90.0, "high": 100.0}],
        )
        judgment = _make_judgment(scenario)
        snap = {
            "snapshot_date": date(2026, 5, 18),  # 8 days before asof=2026-05-26
            "snapshot": {"scenario_forest": [scenario]},
        }
        with patch("fusion.judgment.fetch_active_judgment", return_value=judgment), \
             patch("forecast.neely_emitter._fetch_latest_neely_snapshot",
                   return_value=snap), \
             patch("forecast.neely_emitter.upsert_forecast") as upd:
            res = emit_judgment_forecast(
                None, "2330", date(2026, 5, 26), stale_threshold_days=7,
            )
        assert res["status"] == "stale_snapshot"
        assert res["skipped"] is True
        assert res["age_days"] == 8
        upd.assert_not_called()

    def test_stale_gate_disabled_when_threshold_zero(self):
        scenario = _make_scenario(
            span_days=400,
            fib_zones=[{"label": "0.5", "low": 90.0, "high": 100.0}],
        )
        judgment = _make_judgment(scenario)
        snap = {
            "snapshot_date": date(2025, 1, 1),  # very old
            "snapshot": {"scenario_forest": [scenario]},
        }
        written = []
        with patch("fusion.judgment.fetch_active_judgment", return_value=judgment), \
             patch("forecast.neely_emitter._fetch_latest_neely_snapshot",
                   return_value=snap), \
             patch("forecast.neely_emitter.upsert_forecast",
                   side_effect=lambda conn, row: written.append(row)):
            res = emit_judgment_forecast(
                None, "2330", date(2026, 5, 26), stale_threshold_days=0,
            )
        assert res["status"] == "written"
        assert len(written) == 1

    def test_anchor_not_in_forest(self):
        # 判讀錨到的候選已不在最新 forest(J2 責任區,emitter 不代判)
        judged_scn = _make_scenario(span_days=400)
        other_scn = _make_scenario(span_days=200, pattern="Diagonal")
        judgment = _make_judgment(judged_scn)
        snap = {
            "snapshot_date": date(2024, 5, 30),
            "snapshot": {"scenario_forest": [other_scn]},
        }
        with patch("fusion.judgment.fetch_active_judgment", return_value=judgment), \
             patch("forecast.neely_emitter._fetch_latest_neely_snapshot",
                   return_value=snap), \
             patch("forecast.neely_emitter.upsert_forecast") as upd:
            res = emit_judgment_forecast(None, "2330", date(2024, 6, 1))
        assert res["status"] == "anchor_not_in_forest"
        assert res["judgment_id"] == 41
        upd.assert_not_called()

    def test_no_fib_zones(self):
        scenario = _make_scenario(span_days=400, fib_zones=[])
        judgment = _make_judgment(scenario)
        snap = {
            "snapshot_date": date(2024, 5, 30),
            "snapshot": {"scenario_forest": [scenario]},
        }
        with patch("fusion.judgment.fetch_active_judgment", return_value=judgment), \
             patch("forecast.neely_emitter._fetch_latest_neely_snapshot",
                   return_value=snap), \
             patch("forecast.neely_emitter.upsert_forecast") as upd:
            res = emit_judgment_forecast(None, "2330", date(2024, 6, 1))
        assert res["status"] == "no_fib_zones"
        upd.assert_not_called()

    def test_malformed_zones(self):
        scenario = _make_scenario(
            span_days=400,
            fib_zones=[{"label": "0.5", "low": None, "high": None}],
        )
        judgment = _make_judgment(scenario)
        snap = {
            "snapshot_date": date(2024, 5, 30),
            "snapshot": {"scenario_forest": [scenario]},
        }
        with patch("fusion.judgment.fetch_active_judgment", return_value=judgment), \
             patch("forecast.neely_emitter._fetch_latest_neely_snapshot",
                   return_value=snap), \
             patch("forecast.neely_emitter.upsert_forecast") as upd:
            res = emit_judgment_forecast(None, "2330", date(2024, 6, 1))
        assert res["status"] == "malformed_zones"
        upd.assert_not_called()

    def test_emits_envelope_row(self):
        scenario = _make_scenario(
            span_days=400,  # Minute → horizon 63
            fib_zones=[
                {"label": "0.382", "low": 90.0, "high": 95.0},
                {"label": "0.618", "low": 92.0, "high": 100.0},
                {"label": "1.000", "low": 105.0, "high": 115.0},
            ],
        )
        judgment = _make_judgment(scenario)
        snap = {
            "snapshot_date": date(2024, 5, 30),
            "snapshot": {"scenario_forest": [scenario]},
        }
        written: list[dict] = []
        with patch("fusion.judgment.fetch_active_judgment", return_value=judgment), \
             patch("forecast.neely_emitter._fetch_latest_neely_snapshot",
                   return_value=snap), \
             patch("forecast.neely_emitter.upsert_forecast",
                   side_effect=lambda conn, row: written.append(row)):
            res = emit_judgment_forecast(None, "2330", date(2024, 6, 1))
        assert res["status"] == "written"
        assert res["horizon_days"] == 63
        assert res["envelope"] == (90.0, 115.0)  # min/max across all zones
        assert res["primary_pattern"] == "Impulse"
        assert res["judgment_id"] == 41
        assert res["judged_by"] == "human:jarry"
        # One row, envelope encloses all zones
        assert len(written) == 1
        row = written[0]
        assert row["source_core"] == "judgment"  # M4 whitelist 值
        assert row["calibrated"] is False
        # 裁量軌:internal_only=True(一行外包絡,禁上畫面與 MCP)
        assert row["internal_only"] is True
        assert row["regime_tag"] == "Impulse"
        assert row["lower"] == 90.0
        assert row["upper"] == 115.0
        assert row["params_hash"].startswith("judgment|")
        assert "id=41" in row["params_hash"]
        assert "by=human:jarry" in row["params_hash"]

    def test_overwrite_horizon_param(self):
        scenario = _make_scenario(
            span_days=400,
            fib_zones=[{"label": "0.5", "low": 90.0, "high": 100.0}],
        )
        judgment = _make_judgment(scenario)
        snap = {
            "snapshot_date": date(2024, 5, 30),
            "snapshot": {"scenario_forest": [scenario]},
        }
        written = []
        with patch("fusion.judgment.fetch_active_judgment", return_value=judgment), \
             patch("forecast.neely_emitter._fetch_latest_neely_snapshot",
                   return_value=snap), \
             patch("forecast.neely_emitter.upsert_forecast",
                   side_effect=lambda conn, row: written.append(row)):
            res = emit_judgment_forecast(
                None, "2330", date(2024, 6, 1), overwrite_horizon=126,
            )
        assert res["horizon_days"] == 126
        assert written[0]["horizon_days"] == 126

    def test_alternate_role_not_used(self):
        # accepted 只有 alternate(無 preferred)→ 不發
        scenario = _make_scenario(
            span_days=400,
            fib_zones=[{"label": "0.5", "low": 90.0, "high": 100.0}],
        )
        judgment = _make_judgment(scenario, role="alternate")
        with patch("fusion.judgment.fetch_active_judgment", return_value=judgment), \
             patch("forecast.neely_emitter.upsert_forecast") as upd:
            res = emit_judgment_forecast(None, "2330", date(2024, 6, 1))
        assert res["status"] == "no_active_judgment"
        upd.assert_not_called()
