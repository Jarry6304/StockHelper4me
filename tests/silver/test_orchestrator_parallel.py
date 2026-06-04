"""Tests for v4.36 SilverOrchestrator parallel `_run_builders`(改 asyncio.gather
+ to_thread + Semaphore,對齊 PostgresWriter ConnectionPool 並行)。

驗證點:
- 多個 builder 並行而非串列(wall time 接近最慢者而非 sum)
- Semaphore 限縮並行度到 `_parallelism_limit()`(預設 = pool.max_size - 1)
- 異常 builder 不擋其他(對齊既有 cores_overview §7.5 dirty 契約)
- 並行下 result dict 鍵與 names 對齊(順序保留 + 內容無缺漏)
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

from silver.orchestrator import PHASE_GROUPS, SilverOrchestrator  # noqa: E402


class _SleepyBuilder:
    """Module fake:run() 同步 sleep N 毫秒,模擬 builder 內部同步 DB I/O。"""

    def __init__(self, name: str, sleep_ms: int):
        self.name = name
        self.sleep_ms = sleep_ms
        self.call_count = 0
        self.thread_ids: list[int] = []

    def run(self, db, stock_ids=None, full_rebuild=False):
        self.call_count += 1
        self.thread_ids.append(threading.get_ident())
        time.sleep(self.sleep_ms / 1000.0)
        return {
            "name": self.name, "rows_read": 0, "rows_written": 0,
            "elapsed_ms": self.sleep_ms,
        }


def _make_db_with_pool(pool_max_size: int):
    """造一個 MagicMock db 但帶有 .pool.max_size 屬性,讓 _parallelism_limit 看得到。"""
    db = MagicMock()
    db.pool = SimpleNamespace(max_size=pool_max_size)
    return db


# ─── _parallelism_limit ─────────────────────────────────────────────────────


def test_parallelism_limit_reads_pool_max_size():
    """db.pool.max_size=8 → limit=7(留 1 conn 給 query 路徑)。"""
    db = _make_db_with_pool(8)
    orch = SilverOrchestrator(db=db)
    assert orch._parallelism_limit() == 7


def test_parallelism_limit_fallback_when_no_pool(monkeypatch):
    """MagicMock 沒有 pool 屬性 → 走 env DB_POOL_SIZE,預設 7。"""
    monkeypatch.delenv("DB_POOL_SIZE", raising=False)
    orch = SilverOrchestrator(db=MagicMock(spec=[]))
    assert orch._parallelism_limit() == 7


def test_parallelism_limit_respects_env(monkeypatch):
    monkeypatch.setenv("DB_POOL_SIZE", "16")
    orch = SilverOrchestrator(db=MagicMock(spec=[]))
    assert orch._parallelism_limit() == 15


def test_parallelism_limit_never_below_one():
    """pool.max_size=1 → limit=1(避免 max(1, 0))。"""
    db = _make_db_with_pool(1)
    orch = SilverOrchestrator(db=db)
    assert orch._parallelism_limit() == 1


# ─── 真實並行:wall time < sum ─────────────────────────────────────────────


def test_run_builders_runs_in_parallel(monkeypatch):
    """6 個 builder 各 sleep 60ms,pool=8 → 並行度 7,wall time 應接近 60ms 而非 360ms。"""
    fakes = {
        name: _SleepyBuilder(name, 60)
        for name in PHASE_GROUPS["7a"][:6]
    }
    monkeypatch.setattr("silver.orchestrator.BUILDERS", fakes)

    db = _make_db_with_pool(8)
    orch = SilverOrchestrator(db=db)

    start = time.monotonic()
    result = asyncio.run(orch.run(
        phases=["7a"], full_rebuild=True,
        builders=list(fakes.keys()),
    ))
    elapsed = time.monotonic() - start

    # 6 builder × 60ms sleep / concurrency 7 ≈ 60-150ms wall;串列會是 ~360ms+
    assert elapsed < 0.3, f"並行過慢:{elapsed:.3f}s(預期 <0.3s,串列會 ~0.36s+)"
    # 全 builder 都跑過
    for name, mod in fakes.items():
        assert mod.call_count == 1
    # result dict 完整
    phase_result = result["results"]["7a"]
    assert set(phase_result.keys()) == set(fakes.keys())
    assert all(r["status"] == "ok" for r in phase_result.values())


def test_run_builders_semaphore_limits_concurrency(monkeypatch):
    """pool=3(limit=2),6 個 builder 各 sleep 50ms → wall ≈ 3×50ms = 150ms。"""
    fakes = {
        name: _SleepyBuilder(name, 50)
        for name in PHASE_GROUPS["7a"][:6]
    }
    monkeypatch.setattr("silver.orchestrator.BUILDERS", fakes)

    db = _make_db_with_pool(3)  # pool=3 → limit=2
    orch = SilverOrchestrator(db=db)

    start = time.monotonic()
    asyncio.run(orch.run(
        phases=["7a"], full_rebuild=True,
        builders=list(fakes.keys()),
    ))
    elapsed = time.monotonic() - start

    # 6 builder / concurrency 2 = 3 波,各 50ms → 150ms;允許 worker thread
    # spawn 開銷,但仍應 < 0.3s(串列會是 ~0.3s 也接近 — 用更嚴緊上下限)
    assert 0.10 < elapsed < 0.35, (
        f"Semaphore 沒擋住:{elapsed:.3f}s(預期 ~150ms,>=300ms 表 6 個全並行)"
    )


# ─── 異常隔離 ────────────────────────────────────────────────────────────


class _FailingBuilder:
    def __init__(self, name, exc):
        self.name = name
        self.exc = exc
        self.call_count = 0

    def run(self, db, stock_ids=None, full_rebuild=False):
        self.call_count += 1
        raise self.exc


def test_one_builder_fail_does_not_block_others(monkeypatch):
    """1 個 builder raise → 標 failed,其他繼續跑(對齊 §7.5 dirty 契約)。"""
    fakes = {
        PHASE_GROUPS["7a"][0]: _SleepyBuilder(PHASE_GROUPS["7a"][0], 20),
        PHASE_GROUPS["7a"][1]: _FailingBuilder(
            PHASE_GROUPS["7a"][1], RuntimeError("simulated DB error")
        ),
        PHASE_GROUPS["7a"][2]: _SleepyBuilder(PHASE_GROUPS["7a"][2], 20),
    }
    monkeypatch.setattr("silver.orchestrator.BUILDERS", fakes)

    db = _make_db_with_pool(8)
    orch = SilverOrchestrator(db=db)

    result = asyncio.run(orch.run(
        phases=["7a"], full_rebuild=True, builders=list(fakes.keys()),
    ))

    phase_result = result["results"]["7a"]
    assert phase_result[PHASE_GROUPS["7a"][0]]["status"] == "ok"
    assert phase_result[PHASE_GROUPS["7a"][1]]["status"] == "failed"
    assert "simulated DB error" in phase_result[PHASE_GROUPS["7a"][1]]["reason"]
    assert phase_result[PHASE_GROUPS["7a"][2]]["status"] == "ok"
    # 全 3 builder 都被 invoke 過
    assert fakes[PHASE_GROUPS["7a"][0]].call_count == 1
    assert fakes[PHASE_GROUPS["7a"][1]].call_count == 1
    assert fakes[PHASE_GROUPS["7a"][2]].call_count == 1


def test_not_implemented_skipped_does_not_block(monkeypatch):
    """NotImplementedError → status=skipped,不擋其他。"""
    fakes = {
        PHASE_GROUPS["7a"][0]: _FailingBuilder(
            PHASE_GROUPS["7a"][0], NotImplementedError("stub")
        ),
        PHASE_GROUPS["7a"][1]: _SleepyBuilder(PHASE_GROUPS["7a"][1], 10),
    }
    monkeypatch.setattr("silver.orchestrator.BUILDERS", fakes)

    db = _make_db_with_pool(8)
    orch = SilverOrchestrator(db=db)

    result = asyncio.run(orch.run(
        phases=["7a"], full_rebuild=True, builders=list(fakes.keys()),
    ))

    phase_result = result["results"]["7a"]
    assert phase_result[PHASE_GROUPS["7a"][0]]["status"] == "skipped"
    assert phase_result[PHASE_GROUPS["7a"][1]]["status"] == "ok"


# ─── 7a incremental 路徑下並行仍對 ─────────────────────────────────────


def test_incremental_path_also_parallel(monkeypatch):
    """v4.15 incremental window 路徑下仍走並行 _run_builders。"""
    fakes = {
        name: _SleepyBuilder(name, 30)
        for name in PHASE_GROUPS["7a"][:4]
    }
    monkeypatch.setattr("silver.orchestrator.BUILDERS", fakes)

    db = _make_db_with_pool(8)
    orch = SilverOrchestrator(db=db)

    start = time.monotonic()
    asyncio.run(orch.run(
        phases=["7a"], full_rebuild=False,  # incremental
        builders=list(fakes.keys()),
    ))
    elapsed = time.monotonic() - start

    # 4 × 30ms / concurrency 7 ≈ 30-90ms;串列 ~120ms
    assert elapsed < 0.2, (
        f"incremental 路徑沒並行?{elapsed:.3f}s(預期 <0.2s)"
    )
    for mod in fakes.values():
        assert mod.call_count == 1


# ─── thread_id 多元化(證明真的走 thread pool) ──────────────────────────


def test_builders_run_on_worker_threads_not_main(monkeypatch):
    """asyncio.to_thread 派 sync run() 到 worker thread,thread_id 不能都是 main thread。"""
    fakes = {
        name: _SleepyBuilder(name, 30)
        for name in PHASE_GROUPS["7a"][:4]
    }
    monkeypatch.setattr("silver.orchestrator.BUILDERS", fakes)

    db = _make_db_with_pool(8)
    orch = SilverOrchestrator(db=db)
    main_tid = threading.get_ident()

    asyncio.run(orch.run(
        phases=["7a"], full_rebuild=True, builders=list(fakes.keys()),
    ))

    # 每個 builder 都該在非 main thread 上跑(asyncio.to_thread default executor)
    for mod in fakes.values():
        assert mod.thread_ids, f"{mod.name} 沒被呼叫"
        assert mod.thread_ids[0] != main_tid, (
            f"{mod.name} 在 main thread 跑 — to_thread 沒生效"
        )
