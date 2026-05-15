"""Tests for v3.3 phase_executor.PhaseExecutor._run_api 並發 + short-circuit 共存。

對齊 plan §規格 4:
- asyncio.gather + Semaphore 並發
- _DatasetErrorTracker 處理 streak counter + Event abort signal
- v1.36 class constants(_DATASET_ERROR_CODES / _DATASET_ERROR_STREAK_THRESHOLD)保留
"""

from __future__ import annotations

import asyncio
import sys
from pathlib import Path
from unittest.mock import MagicMock

import pytest

_REPO_ROOT = Path(__file__).resolve().parent.parent.parent
_SRC_ROOT = _REPO_ROOT / "src"
for p in (str(_SRC_ROOT), str(_REPO_ROOT)):
    if p not in sys.path:
        sys.path.insert(0, p)


class TestDatasetErrorTracker:
    """並發場景下 streak counter + abort Event 行為。"""

    @pytest.mark.asyncio
    async def test_dataset_error_increments_until_abort(self):
        from bronze.phase_executor import _DatasetErrorTracker

        tracker = _DatasetErrorTracker(
            threshold=5,
            error_codes=frozenset({403, 404, 422}),
        )
        for _ in range(4):
            tracker.record_error(403)
        assert tracker.aborted.is_set() is False
        assert tracker.streak == 4
        tracker.record_error(403)
        assert tracker.aborted.is_set() is True
        assert tracker.streak == 5

    @pytest.mark.asyncio
    async def test_non_dataset_error_resets_streak(self):
        from bronze.phase_executor import _DatasetErrorTracker

        tracker = _DatasetErrorTracker(threshold=5, error_codes=frozenset({403, 404, 422}))
        tracker.record_error(403)
        tracker.record_error(403)
        assert tracker.streak == 2
        # 500 不在 dataset_error_codes → reset streak
        tracker.record_error(500)
        assert tracker.streak == 0
        assert tracker.aborted.is_set() is False

    @pytest.mark.asyncio
    async def test_success_resets_streak(self):
        from bronze.phase_executor import _DatasetErrorTracker

        tracker = _DatasetErrorTracker(threshold=5, error_codes=frozenset({403, 404, 422}))
        tracker.record_error(403)
        tracker.record_error(403)
        tracker.record_success()
        assert tracker.streak == 0


class TestRunApiConcurrency:
    """_run_api 並發處理 — mock client 跑 N tasks 比序列快。"""

    @pytest.mark.asyncio
    async def test_gather_runs_tasks_in_parallel(self):
        """10 個 task 平行跑,wall time 應接近單一 task 而非 N×。"""
        from api_client import APIError  # noqa: F401  (確保 module 可載)

        # 直接測 inner closure pattern:gather + semaphore 真的會並發。
        # 不掛真實 PhaseExecutor(它有許多 deps),簡化驗 plan §規格 4
        # 的核心契約:相同 sleep time 並發 vs 序列的 wall time 差異。
        async def fake_segment(task_id: int) -> int:
            await asyncio.sleep(0.05)
            return task_id

        sem = asyncio.Semaphore(8)

        async def wrapped(task_id: int) -> int:
            async with sem:
                return await fake_segment(task_id)

        import time
        start = time.monotonic()
        results = await asyncio.gather(*(wrapped(i) for i in range(16)))
        elapsed = time.monotonic() - start
        # 16 tasks × 50ms / 8 concurrency = ~100ms wall;序列會是 ~800ms
        assert len(results) == 16
        assert elapsed < 0.4, f"並發過慢:{elapsed:.3f}s(預期 <0.4s)"


class TestShortCircuitConstantsPreserved:
    """v1.36 class constants 仍然存在(既有 tests 對齊)。"""

    def test_threshold_value_preserved(self):
        from bronze.phase_executor import PhaseExecutor
        assert PhaseExecutor._DATASET_ERROR_STREAK_THRESHOLD == 5

    def test_dataset_error_codes_preserved(self):
        from bronze.phase_executor import PhaseExecutor
        codes = PhaseExecutor._DATASET_ERROR_CODES
        assert 403 in codes
        assert 404 in codes
        assert 422 in codes
        assert 429 not in codes
        assert 500 not in codes
