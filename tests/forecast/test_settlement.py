"""Tests for src/forecast/settlement.py — resolve_pending."""

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

from forecast.settlement import resolve_pending, _settlement_date  # noqa: E402


def test_settlement_date_arithmetic():
    assert _settlement_date(date(2024, 1, 1), 21) == date(2024, 1, 22)
    assert _settlement_date(date(2024, 1, 1), 126) == date(2024, 5, 6)


def _pending_row(*, id, stock_id, forecast_date, horizon, lower, upper, c=0.80):
    return {
        "id": id,
        "stock_id": stock_id,
        "forecast_date": forecast_date,
        "horizon_days": horizon,
        "lower": lower,
        "upper": upper,
        "point": (lower + upper) / 2,
        "confidence": c,
        "calibrated": False,
        "source_core": "baseline",
        "regime_tag": None,
        "params_hash": "abc",
    }


class TestResolvePending:
    def test_settles_hit_row(self):
        # forecast on 2024-01-01 with horizon=21 settles on 2024-01-22.
        # Realized = 100 ∈ [90, 110] → hit=True
        pending = [_pending_row(id=1, stock_id="2330",
                                forecast_date=date(2024, 1, 1),
                                horizon=21, lower=90, upper=110)]
        updates: list[dict] = []

        with patch("forecast.settlement.fetch_unresolved", return_value=pending), \
             patch("forecast.settlement.asof_close_series",
                   return_value=[{"date": date(2024, 1, 22),
                                  "asof_adj_close": 100.0}]), \
             patch("forecast.settlement.update_settlement",
                   side_effect=lambda conn, **kw: updates.append(kw)):
            summary = resolve_pending(conn=None, asof=date(2024, 1, 30))

        assert summary["settled"] == 1
        assert summary["missing_realized"] == 0
        assert updates[0]["hit"] is True
        assert updates[0]["realized_price"] == 100.0
        # pinball with realized in band, 0.80 conf, [90,110]:
        # alpha=0.2, tau_lo=0.1, tau_hi=0.9
        # lo loss = (100-90)*0.1 = 1, hi loss = (110-100)*0.1 = 1
        # total = 0.5*2 = 1.0
        assert updates[0]["pinball_loss"] == pytest.approx(1.0)

    def test_settles_miss_row(self):
        pending = [_pending_row(id=2, stock_id="2330",
                                forecast_date=date(2024, 1, 1),
                                horizon=21, lower=200, upper=300)]
        updates: list[dict] = []

        with patch("forecast.settlement.fetch_unresolved", return_value=pending), \
             patch("forecast.settlement.asof_close_series",
                   return_value=[{"date": date(2024, 1, 22),
                                  "asof_adj_close": 100.0}]), \
             patch("forecast.settlement.update_settlement",
                   side_effect=lambda conn, **kw: updates.append(kw)):
            resolve_pending(conn=None, asof=date(2024, 1, 30))

        assert updates[0]["hit"] is False

    def test_missing_realized_skipped(self):
        pending = [_pending_row(id=3, stock_id="2330",
                                forecast_date=date(2024, 1, 1),
                                horizon=21, lower=90, upper=110)]
        with patch("forecast.settlement.fetch_unresolved", return_value=pending), \
             patch("forecast.settlement.asof_close_series", return_value=[]), \
             patch("forecast.settlement.update_settlement") as upd:
            summary = resolve_pending(conn=None, asof=date(2024, 1, 30))
        assert summary["settled"] == 0
        assert summary["missing_realized"] == 1
        upd.assert_not_called()
