"""Tests for src/pit/ohlcv.py — as-of-T OHLCV reconstruction.

Validates the Python mirror of rust_compute/silver_s1_adjustment/src/main.rs.
Critical contract: PIT must NOT apply AF for events with date > asof_t.
"""

from __future__ import annotations

import sys
from contextlib import contextmanager
from datetime import date
from pathlib import Path

import pytest

_REPO_ROOT = Path(__file__).resolve().parent.parent.parent
_SRC_ROOT = _REPO_ROOT / "src"
for p in (str(_SRC_ROOT), str(_REPO_ROOT)):
    if p not in sys.path:
        sys.path.insert(0, p)

from pit.ohlcv import _compute_event_af, asof_close_series, asof_ohlc  # noqa: E402


class _MockCursor:
    """Mimics psycopg dict_row cursor.  Consumes from a SHARED plan queue."""

    def __init__(self, plan_queue: list[list[dict]]):
        self._plan = plan_queue  # shared reference, popped at conn level
        self._current: list[dict] = []

    def __enter__(self):
        return self

    def __exit__(self, *exc):
        return False

    def execute(self, sql, params=None):
        if self._plan:
            self._current = self._plan.pop(0)
        else:
            self._current = []
        return self

    def fetchall(self):
        rows = self._current
        self._current = []
        return rows


class _MockConn:
    """One conn = one shared plan queue across all its cursors.

    Tests order canned data matching the order asof_ohlc issues queries:
    1st cur.execute → raw_rows, 2nd cur.execute → event_rows.
    """

    def __init__(self, plan):
        # Mutable shared queue — cursors pop from this single list.
        self._plan: list[list[dict]] = list(plan)

    def cursor(self):
        return _MockCursor(self._plan)


def _raw_row(d, close, open_=None, high=None, low=None, volume=1000):
    return {
        "date": d,
        "open": float(open_) if open_ is not None else float(close),
        "high": float(high) if high is not None else float(close),
        "low": float(low) if low is not None else float(close),
        "close": float(close),
        "volume": volume,
    }


def _event_row(d, event_type, *, bp=None, rp=None, cash=0.0, stock=0.0, vf=1.0, detail=None):
    return {
        "date": d,
        "event_type": event_type,
        "before_price": bp,
        "reference_price": rp,
        "cash_dividend": cash,
        "stock_dividend": stock,
        "volume_factor": vf,
        "detail": detail,
    }


# ─── _compute_event_af unit tests ─────────────────────────────────────────────


class TestComputeEventAf:
    def test_priority_1_api_exact(self):
        # 50 → 49 reference → AF = 50/49 ≈ 1.0204 (1 dollar cash dividend)
        ev = _event_row(date(2024, 6, 1), "dividend", bp=50.0, rp=49.0, cash=1.0)
        af, vf = _compute_event_af(ev, raw_prev_close=50.0)
        assert af == pytest.approx(50.0 / 49.0)
        assert vf == 1.0

    def test_priority_1_unreliable_pure_stock_dividend_falls_through(self):
        # FinMind quirk: pure stock dividend → bp == rp (no real cut), should
        # fall back to priority 2 formula
        ev = _event_row(date(2024, 6, 1), "dividend", bp=50.0, rp=50.0, stock=1.0)
        af, vf = _compute_event_af(ev, raw_prev_close=50.0)
        # priority 2: p_after = 50 / (1 + 0.1) = 45.4545
        # af = 50 / 45.4545 = 1.1
        assert af == pytest.approx(1.1)

    def test_priority_2_dividend_fallback_pure_cash(self):
        ev = _event_row(date(2024, 6, 1), "dividend", cash=1.0)
        af, vf = _compute_event_af(ev, raw_prev_close=50.0)
        # p_after = (50 - 1) / 1 = 49 → af = 50/49
        assert af == pytest.approx(50.0 / 49.0)

    def test_priority_3_capital_increase(self):
        detail = {"subscription_price": 30.0, "subscription_rate_raw": 100.0}
        ev = _event_row(date(2024, 6, 1), "capital_increase", detail=detail)
        af, vf = _compute_event_af(ev, raw_prev_close=50.0)
        # r = 0.1; after_price = (50 + 30*0.1) / 1.1 = 53/1.1 ≈ 48.18
        # af = 50/48.18 ≈ 1.0377
        assert af == pytest.approx(50.0 / ((50.0 + 30.0 * 0.1) / 1.1))

    def test_unknown_returns_1(self):
        ev = _event_row(date(2024, 6, 1), "par_value_change", vf=2.0)
        af, vf = _compute_event_af(ev, raw_prev_close=50.0)
        assert af == 1.0
        assert vf == 2.0


# ─── asof_close_series integration tests ──────────────────────────────────────


class TestAsofCloseSeries:
    def test_no_events_returns_raw_closes(self):
        raws = [
            _raw_row(date(2024, 1, 5), 100),
            _raw_row(date(2024, 1, 6), 101),
            _raw_row(date(2024, 1, 7), 102),
        ]
        conn = _MockConn([raws, []])  # raw_rows then empty event_rows
        out = asof_close_series(conn, "2330", date(2024, 1, 7), lookback_days=30)
        assert len(out) == 3
        # No events → asof_adj_close == raw_close
        for row, raw in zip(out, raws):
            assert row["raw_close"] == raw["close"]
            assert row["asof_adj_close"] == pytest.approx(raw["close"])

    def test_single_event_lifts_earlier_dates(self):
        # 3 raw days; dividend event on day 2 with AF = 1.10 (10% cash on 50)
        # → row at day 1 should get × 1.10, days 2 and 3 unchanged
        # (rule: 先 push 再更新 multiplier — multiplier for row T = product of
        # AFs for events with date > T strictly)
        raws = [
            _raw_row(date(2024, 1, 5), 100),
            _raw_row(date(2024, 1, 6), 90),
            _raw_row(date(2024, 1, 7), 92),
        ]
        # API exact values: bp=100 (closes of day 1), rp ≈ 100/1.10 ≈ 90.91
        events = [_event_row(date(2024, 1, 6), "dividend",
                             bp=100.0, rp=100.0 / 1.10, cash=9.09)]
        conn = _MockConn([raws, events])
        out = asof_close_series(conn, "2330", date(2024, 1, 7), 30)
        # Day 1 (before event): close * 1.10 = 110
        assert out[0]["asof_adj_close"] == pytest.approx(110.0, rel=1e-3)
        # Day 2 (event day, raw is post-event): unchanged
        assert out[1]["asof_adj_close"] == pytest.approx(90.0)
        # Day 3 (after event): unchanged
        assert out[2]["asof_adj_close"] == pytest.approx(92.0)

    def test_event_after_asof_t_not_applied(self):
        # CRITICAL PIT contract: events with date > asof_t MUST NOT be applied.
        # The query in asof_ohlc filters events to date ≤ asof_t at SQL level;
        # this test simulates that by passing only events ≤ asof_t.
        raws = [
            _raw_row(date(2024, 1, 5), 100),
            _raw_row(date(2024, 1, 6), 101),
        ]
        # asof_t = 2024-01-06; future event on 2024-01-07 is NOT in our event_rows
        events_already_filtered = []
        conn = _MockConn([raws, events_already_filtered])
        out = asof_close_series(conn, "2330", date(2024, 1, 6), 30)
        # No adjustment applied
        assert out[0]["asof_adj_close"] == pytest.approx(100.0)
        assert out[1]["asof_adj_close"] == pytest.approx(101.0)

    def test_multi_event_cumprod(self):
        # 4 raw days, 2 dividend events
        raws = [
            _raw_row(date(2024, 1, 5), 100),
            _raw_row(date(2024, 1, 6), 90),
            _raw_row(date(2024, 1, 7), 91),
            _raw_row(date(2024, 1, 8), 81),
        ]
        # Event 1 on day 2: AF = 100/(100/1.10) = 1.10  (cash 9.09 on bp 100)
        # Event 2 on day 4: AF = 91/(91/1.10) = 1.10   (cash on bp 91)
        events = [
            _event_row(date(2024, 1, 6), "dividend",
                       bp=100.0, rp=100.0 / 1.10, cash=9.09),
            _event_row(date(2024, 1, 8), "dividend",
                       bp=91.0, rp=91.0 / 1.10, cash=8.27),
        ]
        conn = _MockConn([raws, events])
        out = asof_close_series(conn, "2330", date(2024, 1, 8), 30)
        # Day 1: × 1.10 × 1.10 = 1.21 → 100 × 1.21 = 121
        # Day 2 (event 1 day): × 1.10 → 90 × 1.10 = 99
        # Day 3 (between events): × 1.10 → 91 × 1.10 = 100.1
        # Day 4 (event 2 day): unchanged
        assert out[0]["asof_adj_close"] == pytest.approx(121.0, rel=1e-3)
        assert out[1]["asof_adj_close"] == pytest.approx(99.0, rel=1e-3)
        assert out[2]["asof_adj_close"] == pytest.approx(100.1, rel=1e-3)
        assert out[3]["asof_adj_close"] == pytest.approx(81.0, rel=1e-3)

    def test_empty_raw_returns_empty(self):
        conn = _MockConn([[], []])
        out = asof_close_series(conn, "2330", date(2024, 1, 8), 30)
        assert out == []


class TestAsofOhlc:
    def test_ohlc_all_four_adjusted_same_multiplier(self):
        # Same as single_event test but with OHLC variation per row
        raws = [
            _raw_row(date(2024, 1, 5), 100, open_=99, high=102, low=98),
            _raw_row(date(2024, 1, 6), 90,  open_=99, high=102, low=88),
        ]
        events = [_event_row(date(2024, 1, 6), "dividend",
                             bp=100.0, rp=100.0 / 1.10, cash=9.09)]
        conn = _MockConn([raws, events])
        out = asof_ohlc(conn, "2330", date(2024, 1, 6), 30)
        # Day 1 should get × 1.10 on all OHLC
        d1 = out[0]
        assert d1["asof_adj_close"] == pytest.approx(110.0, rel=1e-3)
        assert d1["asof_adj_open"] == pytest.approx(99 * 1.10, rel=1e-3)
        assert d1["asof_adj_high"] == pytest.approx(102 * 1.10, rel=1e-3)
        assert d1["asof_adj_low"] == pytest.approx(98 * 1.10, rel=1e-3)
        # Day 2 unchanged
        assert out[1]["asof_adj_close"] == pytest.approx(90.0)
        assert out[1]["asof_adj_open"] == pytest.approx(99.0)
