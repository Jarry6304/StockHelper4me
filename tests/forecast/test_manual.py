"""Tests for src/forecast/manual.py — discretionary forecast row."""

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

from forecast.manual import write_manual_forecast  # noqa: E402


class TestManual:
    def test_writes_basic_row(self):
        written = []
        with patch("forecast.manual.upsert_forecast",
                   side_effect=lambda conn, row: written.append(row)):
            res = write_manual_forecast(
                None,
                stock_id="2330",
                forecast_date=date(2024, 6, 1),
                horizon_days=63,
                lower=1200.0,
                upper=1500.0,
                confidence=0.70,
                regime="5wave_completion",
                note="long anchor support test",
            )
        assert res["status"] == "written"
        assert len(written) == 1
        row = written[0]
        assert row["source_core"] == "manual"
        assert row["calibrated"] is False
        assert row["regime_tag"] == "5wave_completion"
        assert row["point"] == 1350.0  # default midpoint
        assert "manual|" in row["params_hash"]

    def test_explicit_point_override(self):
        written = []
        with patch("forecast.manual.upsert_forecast",
                   side_effect=lambda conn, row: written.append(row)):
            write_manual_forecast(
                None,
                stock_id="2330",
                forecast_date=date(2024, 6, 1),
                horizon_days=63,
                lower=1200.0, upper=1500.0,
                point=1400.0,  # override
            )
        assert written[0]["point"] == 1400.0

    def test_invalid_bounds_raise(self):
        with pytest.raises(ValueError):
            write_manual_forecast(
                None,
                stock_id="2330",
                forecast_date=date(2024, 6, 1),
                horizon_days=63,
                lower=1500.0, upper=1200.0,  # backwards
            )

    def test_invalid_confidence_raises(self):
        with pytest.raises(ValueError):
            write_manual_forecast(
                None,
                stock_id="2330",
                forecast_date=date(2024, 6, 1),
                horizon_days=63,
                lower=1200.0, upper=1500.0,
                confidence=1.5,
            )

    def test_invalid_horizon_raises(self):
        with pytest.raises(ValueError):
            write_manual_forecast(
                None,
                stock_id="2330",
                forecast_date=date(2024, 6, 1),
                horizon_days=0,
                lower=1200.0, upper=1500.0,
            )

    def test_params_hash_includes_note_for_traceability(self):
        written = []
        with patch("forecast.manual.upsert_forecast",
                   side_effect=lambda conn, row: written.append(row)):
            write_manual_forecast(
                None,
                stock_id="2330",
                forecast_date=date(2024, 6, 1),
                horizon_days=63,
                lower=1200.0, upper=1500.0,
                note="bullish breakout above 1450",
            )
            write_manual_forecast(
                None,
                stock_id="2330",
                forecast_date=date(2024, 6, 1),
                horizon_days=63,
                lower=1200.0, upper=1500.0,
                note="alternative bearish flat scenario",
            )
        # Same (stock, date, horizon) but different note → different hash
        assert written[0]["params_hash"] != written[1]["params_hash"]
