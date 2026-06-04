"""Tests for v4.36 CrossStockOrchestrator parallel run。

v4.36 把 cross_cores Phase 8 從 sequential for-loop 改 asyncio.gather +
asyncio.to_thread + Semaphore(對齊 SilverOrchestrator pattern)。
"""

from __future__ import annotations

import asyncio
import sys
import threading
import time
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import MagicMock

import pytest

_REPO_ROOT = Path(__file__).resolve().parent.parent.parent
_SRC_ROOT = _REPO_ROOT / "src"
for p in (str(_SRC_ROOT), str(_REPO_ROOT)):
    if p not in sys.path:
        sys.path.insert(0, p)

from cross_cores.orchestrator import CrossStockOrchestrator  # noqa: E402


class _SleepyModule:
    """Cross-stock builder module fake — 同步 sleep N ms 模擬 CPU+I/O bound。"""

    def __init__(self, name: str, sleep_ms: int, *, output_table: str | None = None):
        self.NAME = name
        self.OUTPUT_TABLE = output_table or f"{name}_ranked_derived"
        self.sleep_ms = sleep_ms
        self.call_count = 0
        self.thread_ids: list[int] = []

    def run(self, db, *, full_rebuild=False, lookback_days=None):
        self.call_count += 1
        self.thread_ids.append(threading.get_ident())
        time.sleep(self.sleep_ms / 1000.0)
        return {
            "name": self.NAME, "rows_written": 100,
            "elapsed_ms": self.sleep_ms,
        }


class _FailingModule:
    def __init__(self, name, exc):
        self.NAME = name
        self.OUTPUT_TABLE = f"{name}_ranked_derived"
        self.exc = exc
        self.call_count = 0

    def run(self, db, *, full_rebuild=False, lookback_days=None):
        self.call_count += 1
        raise self.exc


def _make_db_with_pool(pool_max_size: int):
    db = MagicMock()
    db.pool = SimpleNamespace(max_size=pool_max_size)
    return db


# ─── _parallelism_limit(對齊 silver) ─────────────────────────────────


def test_parallelism_limit_reads_pool_max_size():
    db = _make_db_with_pool(8)
    orch = CrossStockOrchestrator(db=db)
    assert orch._parallelism_limit() == 7


def test_parallelism_limit_fallback_when_no_pool(monkeypatch):
    monkeypatch.delenv("DB_POOL_SIZE", raising=False)
    orch = CrossStockOrchestrator(db=MagicMock(spec=[]))
    assert orch._parallelism_limit() == 7


# ─── 真實並行 ────────────────────────────────────────────────────────


def test_run_in_parallel_wall_less_than_sum(monkeypatch):
    """6 個 builder 各 sleep 60ms 並行(pool=8 limit=7)→ wall < sum/3。"""
    fakes = {
        f"b{i}": _SleepyModule(f"b{i}", 60) for i in range(6)
    }
    monkeypatch.setattr("cross_cores.orchestrator.BUILDERS", fakes)

    db = _make_db_with_pool(8)
    orch = CrossStockOrchestrator(db=db)

    start = time.monotonic()
    result = asyncio.run(orch.run(builders=list(fakes.keys()), full_rebuild=True))
    elapsed = time.monotonic() - start

    # sum=360ms;並行(limit 7,sleep 60ms 全部一波)≈ 60-120ms wall
    assert elapsed < 0.3, f"並行過慢:{elapsed:.3f}s(預期 <0.3s,sum 為 0.36s)"
    for mod in fakes.values():
        assert mod.call_count == 1
    assert set(result["results"].keys()) == set(fakes.keys())


def test_semaphore_caps_concurrency(monkeypatch):
    """pool=3(limit=2),6 個 builder × 50ms → wall ≈ 3 波 × 50ms = 150ms。"""
    fakes = {f"b{i}": _SleepyModule(f"b{i}", 50) for i in range(6)}
    monkeypatch.setattr("cross_cores.orchestrator.BUILDERS", fakes)

    db = _make_db_with_pool(3)
    orch = CrossStockOrchestrator(db=db)

    start = time.monotonic()
    asyncio.run(orch.run(builders=list(fakes.keys()), full_rebuild=True))
    elapsed = time.monotonic() - start

    assert 0.10 < elapsed < 0.35, (
        f"Semaphore 沒擋住:{elapsed:.3f}s(預期 ~150ms;>=300ms 表全並行了)"
    )


# ─── 失敗隔離 ────────────────────────────────────────────────────────


def test_one_fail_does_not_block_others(monkeypatch):
    fakes = {
        "ok1": _SleepyModule("ok1", 10),
        "boom": _FailingModule("boom", RuntimeError("simulated")),
        "ok2": _SleepyModule("ok2", 10),
    }
    monkeypatch.setattr("cross_cores.orchestrator.BUILDERS", fakes)

    db = _make_db_with_pool(8)
    orch = CrossStockOrchestrator(db=db)

    result = asyncio.run(orch.run(builders=["ok1", "boom", "ok2"], full_rebuild=True))

    r = result["results"]
    assert r["ok1"]["status"] == "ok"
    assert r["boom"]["status"] == "failed"
    assert "simulated" in r["boom"]["reason"]
    assert r["ok2"]["status"] == "ok"
    assert fakes["ok1"].call_count == 1
    assert fakes["ok2"].call_count == 1


# ─── 預設 builders=None 仍跑全部 ─────────────────────────────────────


def test_run_with_default_runs_all_builders(monkeypatch):
    fakes = {
        "a": _SleepyModule("a", 5),
        "b": _SleepyModule("b", 5),
        "c": _SleepyModule("c", 5),
    }
    monkeypatch.setattr("cross_cores.orchestrator.BUILDERS", fakes)

    db = _make_db_with_pool(8)
    orch = CrossStockOrchestrator(db=db)

    result = asyncio.run(orch.run(full_rebuild=False))
    assert set(result["results"].keys()) == set(fakes.keys())


def test_unknown_builder_raises_before_dispatch(monkeypatch):
    fakes = {"a": _SleepyModule("a", 5)}
    monkeypatch.setattr("cross_cores.orchestrator.BUILDERS", fakes)

    db = _make_db_with_pool(8)
    orch = CrossStockOrchestrator(db=db)

    with pytest.raises(ValueError, match="未知 cross_cores builder.*typo"):
        asyncio.run(orch.run(builders=["typo"], full_rebuild=True))

    # 0 dispatch
    assert fakes["a"].call_count == 0


# ─── thread_id 多元化 ───────────────────────────────────────────────


def test_builders_run_on_worker_threads(monkeypatch):
    fakes = {f"b{i}": _SleepyModule(f"b{i}", 20) for i in range(4)}
    monkeypatch.setattr("cross_cores.orchestrator.BUILDERS", fakes)

    db = _make_db_with_pool(8)
    orch = CrossStockOrchestrator(db=db)
    main_tid = threading.get_ident()

    asyncio.run(orch.run(builders=list(fakes.keys()), full_rebuild=True))

    for mod in fakes.values():
        assert mod.thread_ids[0] != main_tid, (
            f"{mod.NAME} 在 main thread 跑 — to_thread 沒生效"
        )


# ─── lookback_days 傳遞仍正確 ─────────────────────────────────────────


def test_lookback_days_passed_through(monkeypatch):
    captured = {}

    class _Capture:
        NAME = "cap"
        OUTPUT_TABLE = "cap"
        def run(self, db, *, full_rebuild=False, lookback_days=None):
            captured["full_rebuild"] = full_rebuild
            captured["lookback_days"] = lookback_days
            return {"name": "cap", "rows_written": 0, "elapsed_ms": 0}

    fakes = {"cap": _Capture()}
    monkeypatch.setattr("cross_cores.orchestrator.BUILDERS", fakes)

    db = _make_db_with_pool(8)
    orch = CrossStockOrchestrator(db=db)

    asyncio.run(orch.run(builders=["cap"], full_rebuild=True, lookback_days=60))
    assert captured == {"full_rebuild": True, "lookback_days": 60}
