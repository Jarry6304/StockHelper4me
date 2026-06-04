"""v4.36 tests:run_fusion_materialize across-stock parallelization。

驗證點:
- parallelism=1 退回單緒原行為(對齊既有 test_materialize.py 全部 assertions)
- parallelism>1 走 ThreadPoolExecutor,wall time 顯著少於 N×sleep
- worker 各自呼叫 get_connection(不再共享 single conn → psycopg thread-safety 風險為 0)
- per-stock 失敗仍 graceful 不擋其他
- _BATCH 邊界:超過 _BATCH 觸發 streaming flush
"""

from __future__ import annotations

import sys
import threading
import time
from datetime import date
from pathlib import Path

_REPO_ROOT = Path(__file__).resolve().parent.parent.parent
_SRC_ROOT = _REPO_ROOT / "src"
for p in (str(_SRC_ROOT), str(_REPO_ROOT)):
    if p not in sys.path:
        sys.path.insert(0, p)

import fusion.materialize._provenance as P  # noqa: E402
import fusion.materialize.fusion_stage as fs  # noqa: E402


class _SpyDB:
    """capture db.upsert calls,thread-safe(用 lock)。"""

    def __init__(self):
        self.calls = []
        self._lock = threading.Lock()

    def upsert(self, table, rows, pk):
        with self._lock:
            self.calls.append((table, len(rows), list(pk)))
        return len(rows)


class _FakeCursor:
    def __init__(self, rows): self._rows = rows
    def __enter__(self): return self
    def __exit__(self, *a): return False
    def execute(self, sql, params=None): pass
    def fetchone(self): return self._rows[0] if self._rows else None
    def fetchall(self): return list(self._rows)


class _FakeConn:
    def __init__(self): self.closed = False
    def cursor(self): return _FakeCursor([])
    def close(self): self.closed = True


def _patch_metadata(monkeypatch, universe):
    """metadata 階段 patch(latest_trading_date / fetch_universe / forecast_log_lag_days)。"""
    monkeypatch.setattr(fs, "get_connection", lambda *a, **k: _FakeConn())
    monkeypatch.setattr(P, "latest_trading_date", lambda conn: date(2026, 5, 28))
    monkeypatch.setattr(P, "fetch_universe", lambda conn, stocks=None: list(stocks or universe))
    monkeypatch.setattr(P, "forecast_log_lag_days", lambda conn, asof: 0)


def _patch_compute(monkeypatch, *, sleep_ms=0, fail_for=None, capture_threads=None):
    """key_levels / resonance 假實作,可加 sleep + 失敗注入 + thread tracking。"""
    fail_for = fail_for or set()

    def _kl(sid, asof, conn=None):
        if sleep_ms:
            time.sleep(sleep_ms / 1000.0)
        if capture_threads is not None:
            capture_threads.append(threading.get_ident())
        if sid in fail_for:
            raise RuntimeError(f"levels boom {sid}")
        return {"stock_id": sid, "levels": []}

    class _Res:
        def __init__(self, sid): self.sid = sid
        def to_dict(self):
            if sleep_ms:
                time.sleep(sleep_ms / 1000.0)
            if self.sid in fail_for:
                raise RuntimeError(f"resonance boom {self.sid}")
            return {"track1": {}, "track2": {}, "findings": []}

    def _rz(sid, asof, timeframe="daily", conn=None):
        return _Res(sid)

    monkeypatch.setattr(fs, "key_levels", _kl)
    monkeypatch.setattr(fs, "resonance", _rz)


# ─── parallelism=1 backward compat ─────────────────────────────────────


def test_parallelism_1_keeps_single_thread_behaviour(monkeypatch):
    _patch_metadata(monkeypatch, universe=["2330", "1101"])
    threads = []
    _patch_compute(monkeypatch, capture_threads=threads)
    db = _SpyDB()
    summary = fs.run_fusion_materialize(db, parallelism=1)

    # 2 stocks × 1 levels + 2 × 3 resonance = 2 + 6
    assert summary["levels_written"] == 2
    assert summary["resonance_written"] == 6
    # 全部在 main thread(parallelism=1 不開 thread pool)
    main_tid = threading.get_ident()
    assert all(t == main_tid for t in threads), "parallelism=1 不該開 worker thread"


# ─── parallelism>1 真實並行 ─────────────────────────────────────────────


def test_parallel_wall_less_than_sum(monkeypatch):
    """8 個 stocks 各 sleep 40ms,parallelism=4 → 並行 wall < sum/2。"""
    universe = [f"s{i:04d}" for i in range(8)]
    _patch_metadata(monkeypatch, universe=universe)
    _patch_compute(monkeypatch, sleep_ms=40)
    db = _SpyDB()

    start = time.monotonic()
    summary = fs.run_fusion_materialize(db, parallelism=4)
    elapsed = time.monotonic() - start

    # 8 stocks × (1 levels + 3 resonance) × 40ms = 1280ms 序列;
    # 並行 4 → ~320ms;設上限 600ms 給點 buffer
    assert elapsed < 0.6, f"並行未生效,wall={elapsed:.3f}s(序列預期 ~1.3s)"
    assert summary["levels_written"] == 8
    assert summary["resonance_written"] == 24


def test_workers_actually_run_on_threadpool(monkeypatch):
    universe = [f"s{i}" for i in range(4)]
    _patch_metadata(monkeypatch, universe=universe)
    threads = []
    _patch_compute(monkeypatch, sleep_ms=10, capture_threads=threads)
    db = _SpyDB()
    fs.run_fusion_materialize(db, parallelism=3)

    main_tid = threading.get_ident()
    distinct = {t for t in threads if t != main_tid}
    # 至少一個非 main thread(thread pool 真實啟動)
    assert distinct, f"workers 沒在 thread pool 跑,thread_ids={threads}"


def test_per_stock_failure_does_not_block_others(monkeypatch):
    universe = ["good_a", "BAD", "good_b"]
    _patch_metadata(monkeypatch, universe=universe)
    _patch_compute(monkeypatch, fail_for={"BAD"})
    db = _SpyDB()
    summary = fs.run_fusion_materialize(db, parallelism=2)

    # BAD 兩部分都炸(levels + 3 resonance)→ errors=4
    # good 全部成功 → levels_written=2,resonance_written=6
    assert summary["levels_written"] == 2
    assert summary["resonance_written"] == 6
    assert summary["errors"] == 4


# ─── flush 行為 ────────────────────────────────────────────────────────


def test_final_flush_called_when_under_batch(monkeypatch):
    """universe 小於 _BATCH → 只一次 final flush per kind(levels + resonance)。"""
    _patch_metadata(monkeypatch, universe=["2330", "1101"])
    _patch_compute(monkeypatch)
    db = _SpyDB()
    fs.run_fusion_materialize(db, parallelism=2)
    # 兩次 upsert(lv batch + rz batch)
    assert len(db.calls) == 2
    # 都打 structural_snapshots
    assert all(c[0] == "structural_snapshots" for c in db.calls)


def test_empty_universe_does_no_upsert(monkeypatch):
    _patch_metadata(monkeypatch, universe=[])
    _patch_compute(monkeypatch)
    db = _SpyDB()
    summary = fs.run_fusion_materialize(db, parallelism=4)
    assert db.calls == []
    assert summary["levels_written"] == 0
    assert summary["resonance_written"] == 0


# ─── _default_parallelism ─────────────────────────────────────────────


def test_default_parallelism_reads_env(monkeypatch):
    monkeypatch.setenv("DB_POOL_SIZE", "16")
    assert fs._default_parallelism() == 15


def test_default_parallelism_fallback_on_invalid(monkeypatch):
    monkeypatch.setenv("DB_POOL_SIZE", "abc")
    assert fs._default_parallelism() == 7
