"""v4.36 tests:conformalize_batch / fuse_batch across-stock parallel。

驗證點:
- parallelism=1 維持原行為(passed conn 走完所有 stock × date × h × c)
- parallelism>1 走 ThreadPoolExecutor + 每 worker 自開 get_connection()
- 結果加總正確(status counter 跨 worker aggregate)
- per-stock 失敗不擋其他
"""

from __future__ import annotations

import sys
import threading
import time
from datetime import date
from pathlib import Path
from unittest.mock import MagicMock, patch

_REPO_ROOT = Path(__file__).resolve().parent.parent.parent
_SRC_ROOT = _REPO_ROOT / "src"
for p in (str(_SRC_ROOT), str(_REPO_ROOT)):
    if p not in sys.path:
        sys.path.insert(0, p)

from forecast.calibration import conformalize_batch  # noqa: E402
from forecast.fusion import fuse_batch  # noqa: E402


# ─── conformalize_batch parallel ──────────────────────────────────────


def test_conformalize_batch_parallelism_1_uses_passed_conn():
    """parallelism=1 + conn passed → 走原單緒 path,workers 不開新 conn。"""
    called_conns: list = []

    def _fake_one(conn, **kw):
        called_conns.append(id(conn))
        return {"status": "written"}

    sentinel_conn = MagicMock(name="caller_conn")
    with patch("forecast.calibration.conformalize_one", side_effect=_fake_one):
        out = conformalize_batch(
            conn=sentinel_conn,
            stock_ids=["2330", "1101"],
            start=date(2026, 5, 1), end=date(2026, 5, 1),
            horizons=[21], confidences=[0.80],
            parallelism=1,
        )

    assert out == {"written": 2}
    # 2 stocks × 1 date × 1 h × 1 c = 2 calls, 全部用同一個 conn(caller's)
    assert len(called_conns) == 2
    assert len(set(called_conns)) == 1, "parallelism=1 應走 caller's conn"


def test_conformalize_batch_parallel_each_worker_own_conn():
    """parallelism>1 → 不用 caller conn,每 worker get_connection 一次。"""
    conn_factory_calls: list[int] = []
    thread_ids: list[int] = []

    class _FakeConn:
        def __init__(self): pass
        def close(self): pass
        def __enter__(self): return self
        def __exit__(self, *a): return False

    def _get_conn_spy():
        conn_factory_calls.append(threading.get_ident())
        return _FakeConn()

    def _fake_one(conn, **kw):
        thread_ids.append(threading.get_ident())
        time.sleep(0.005)  # 給 thread pool 時間切換
        return {"status": "written"}

    with patch("forecast._db.get_connection", _get_conn_spy), \
         patch("forecast.calibration.conformalize_one", side_effect=_fake_one):
        out = conformalize_batch(
            stock_ids=[f"s{i}" for i in range(6)],
            start=date(2026, 5, 1), end=date(2026, 5, 1),
            horizons=[21], confidences=[0.80],
            parallelism=4,
        )

    # 6 stocks → 6 conn factory calls(1 per worker / stock)
    assert len(conn_factory_calls) == 6
    main_tid = threading.get_ident()
    distinct_tids = {t for t in thread_ids if t != main_tid}
    assert distinct_tids, f"workers 沒在 thread pool 跑,tids={thread_ids}"
    # Counts: 6 stocks × 1 date × 1 h × 1 c = 6 calls
    assert sum(out.values()) == 6


def test_conformalize_batch_parallel_aggregates_status_counts():
    """parallelism>1 status counter 跨 worker aggregate 正確。"""
    counts_per_call: dict[str, int] = {"written": 0, "no_raw": 0}

    def _fake_one(conn, **kw):
        # 第 1, 3, 5 個 stock 都 written;第 2, 4 個 stock 都 no_raw
        sid = kw["stock_id"]
        idx = int(sid[1:])
        status = "written" if idx % 2 == 1 else "no_raw"
        counts_per_call[status] += 1
        return {"status": status}

    class _FakeConn:
        def close(self): pass

    with patch("forecast._db.get_connection", lambda: _FakeConn()), \
         patch("forecast.calibration.conformalize_one", side_effect=_fake_one):
        out = conformalize_batch(
            stock_ids=[f"s{i}" for i in range(1, 6)],  # 5 stocks
            start=date(2026, 5, 1), end=date(2026, 5, 1),
            horizons=[21], confidences=[0.80],
            parallelism=3,
        )

    # 3 written (1, 3, 5) + 2 no_raw (2, 4)
    assert out == {"written": 3, "no_raw": 2}


def test_conformalize_batch_worker_crash_isolated():
    """1 worker raise → 計 worker_error,其他 worker 仍跑完。"""
    def _fake_one(conn, **kw):
        if kw["stock_id"] == "BAD":
            raise RuntimeError("simulated DB error")
        return {"status": "written"}

    class _FakeConn:
        def close(self): pass

    with patch("forecast._db.get_connection", lambda: _FakeConn()), \
         patch("forecast.calibration.conformalize_one", side_effect=_fake_one):
        out = conformalize_batch(
            stock_ids=["good_a", "BAD", "good_b"],
            start=date(2026, 5, 1), end=date(2026, 5, 1),
            horizons=[21], confidences=[0.80],
            parallelism=3,
        )

    # 2 good → written;BAD worker crashes → worker_error
    assert out.get("written") == 2
    assert out.get("worker_error") == 1


def test_conformalize_batch_parallel_speedup():
    """6 stocks 各 30ms sleep,parallelism=3 wall 應 < 串列 sum 的一半。"""
    def _slow_one(conn, **kw):
        time.sleep(0.03)
        return {"status": "written"}

    class _FakeConn:
        def close(self): pass

    with patch("forecast._db.get_connection", lambda: _FakeConn()), \
         patch("forecast.calibration.conformalize_one", side_effect=_slow_one):
        start = time.monotonic()
        conformalize_batch(
            stock_ids=[f"s{i}" for i in range(6)],
            start=date(2026, 5, 1), end=date(2026, 5, 1),
            horizons=[21], confidences=[0.80],
            parallelism=3,
        )
        elapsed = time.monotonic() - start

    # 6 × 30ms = 180ms 序列;parallel 3 ≈ 60-90ms。設上限 130ms 給點 buffer。
    assert elapsed < 0.13, f"並行未生效:{elapsed:.3f}s(序列預期 ~0.18s)"


# ─── fuse_batch parallel ─────────────────────────────────────────────


def test_fuse_batch_parallelism_1_keeps_caller_conn():
    called_conns: list = []

    def _fake_one(conn, **kw):
        called_conns.append(id(conn))
        return {"status": "written"}

    sentinel_conn = MagicMock(name="caller_conn")
    with patch("forecast.fusion.fuse_one", side_effect=_fake_one):
        out = fuse_batch(
            conn=sentinel_conn,
            stock_ids=["2330", "1101"],
            forecast_dates=[date(2026, 5, 1)],
            horizons=[21], confidences=[0.80],
            parallelism=1,
        )

    assert out == {"written": 2}
    assert len(set(called_conns)) == 1  # 同一 conn


def test_fuse_batch_parallel_aggregates():
    """fuse_batch parallel:每 worker own conn,result 正確 aggregate。"""
    def _fake_one(conn, **kw):
        return {"status": "written"}

    class _FakeConn:
        def close(self): pass

    with patch("forecast._db.get_connection", lambda: _FakeConn()), \
         patch("forecast.fusion.fuse_one", side_effect=_fake_one):
        out = fuse_batch(
            stock_ids=[f"s{i}" for i in range(4)],
            forecast_dates=[date(2026, 5, 1), date(2026, 5, 2)],
            horizons=[21, 63], confidences=[0.80],
            parallelism=2,
        )

    # 4 stocks × 2 dates × 2 horizons × 1 conf = 16
    assert out == {"written": 16}


def test_fuse_batch_worker_isolation():
    def _fake_one(conn, **kw):
        if kw["stock_id"] == "boom":
            raise RuntimeError("DB connection lost")
        return {"status": "written"}

    class _FakeConn:
        def close(self): pass

    with patch("forecast._db.get_connection", lambda: _FakeConn()), \
         patch("forecast.fusion.fuse_one", side_effect=_fake_one):
        out = fuse_batch(
            stock_ids=["a", "boom", "b"],
            forecast_dates=[date(2026, 5, 1)],
            horizons=[21], confidences=[0.80],
            parallelism=3,
        )

    assert out.get("written") == 2
    assert out.get("worker_error") == 1


# ─── empty stock_ids ────────────────────────────────────────────────


def test_conformalize_batch_empty_stocks():
    out = conformalize_batch(
        stock_ids=[],
        start=date(2026, 5, 1), end=date(2026, 5, 1),
        parallelism=4,
    )
    assert out == {}


def test_fuse_batch_empty_stocks():
    out = fuse_batch(
        stock_ids=[],
        forecast_dates=[date(2026, 5, 1)],
        parallelism=4,
    )
    assert out == {}
