"""Tests for v4.36 _SegmentRunner._write/_merge_delist_date async + to_thread。

確認 sync DB 寫入不再封住 asyncio event loop:多個 concurrent task 在 fetch 後
能真正並行寫(透過 PostgresWriter pool 多 connection),而非被 event loop 序列化。
"""

from __future__ import annotations

import asyncio
import sys
import threading
import time
from datetime import date
from pathlib import Path
from unittest.mock import MagicMock

import pytest

_REPO_ROOT = Path(__file__).resolve().parent.parent.parent
_SRC_ROOT = _REPO_ROOT / "src"
for p in (str(_SRC_ROOT), str(_REPO_ROOT)):
    if p not in sys.path:
        sys.path.insert(0, p)


def _make_runner(*, merge_strategy=None, target_table="price_daily",
                 universe=None, db_upsert=None):
    from bronze.segment_runner import _SegmentRunner
    from config_loader import ApiConfig

    api = ApiConfig(
        name="price_daily",
        dataset="TaiwanStockPrice",
        param_mode="all_market",
        target_table=target_table,
        phase=3,
        enabled=True,
        is_backer=True,
        segment_days=1,
        merge_strategy=merge_strategy,
        universe_filter=False,
    )
    db = MagicMock()
    db._table_pks.return_value = ["market", "stock_id", "date"]
    if db_upsert is not None:
        db.upsert = db_upsert
    field_mapper = MagicMock()
    sync_tracker = MagicMock()
    return _SegmentRunner(
        api_config=api,
        db=db,
        client=MagicMock(),
        field_mapper=field_mapper,
        sync_tracker=sync_tracker,
        get_trading_dates=lambda: set(),
        tracker=MagicMock(),
        sem=asyncio.Semaphore(1),
        dry_run=False,
        universe=universe,
    ), db, sync_tracker


class TestWriteIsAsync:
    """_write 改 async + 走 asyncio.to_thread。"""

    def test_write_is_coroutine_function(self):
        from bronze.segment_runner import _SegmentRunner
        import inspect
        assert inspect.iscoroutinefunction(_SegmentRunner._write)

    @pytest.mark.asyncio
    async def test_write_calls_db_upsert_with_pks(self):
        """正常 path:_write 呼叫 db.upsert(table, rows, pks)。"""
        runner, db, _ = _make_runner()
        rows = [{"market": "TW", "stock_id": "2330", "date": "2026-05-15", "close": 1000}]
        ok = await runner._write("2330", "2026-05-15", "2026-05-15", rows)
        assert ok is True
        db.upsert.assert_called_once_with(
            "price_daily", rows, ["market", "stock_id", "date"]
        )

    @pytest.mark.asyncio
    async def test_write_empty_rows_is_noop(self):
        """空 rows 不該打 db.upsert。"""
        runner, db, _ = _make_runner()
        ok = await runner._write("2330", "2026-05-15", "2026-05-15", [])
        assert ok is True
        db.upsert.assert_not_called()

    @pytest.mark.asyncio
    async def test_write_db_failure_marks_failed(self):
        """db.upsert raise → mark_failed,回 False。"""
        def boom(*args, **kwargs):
            raise RuntimeError("PG connection refused")
        runner, db, tracker = _make_runner(db_upsert=boom)
        rows = [{"market": "TW", "stock_id": "2330", "date": "2026-05-15"}]
        ok = await runner._write("2330", "2026-05-15", "2026-05-15", rows)
        assert ok is False
        tracker.mark_failed.assert_called_once()


class TestMergeDelistDate:
    """update_delist_date merge_strategy 走 async wrapper。"""

    @pytest.mark.asyncio
    async def test_merge_delist_calls_db_update(self):
        runner, db, _ = _make_runner(
            merge_strategy="update_delist_date",
            target_table="stock_info_ref",
        )
        rows = [
            {"stock_id": "2330", "delisting_date": "2026-01-01"},
            {"stock_id": "1234", "date": "2026-02-01"},
        ]
        ok = await runner._write("__ALL__", "2026-01-01", "2026-02-01", rows)
        assert ok is True
        # 應該對每行 valid row call 一次 update
        assert db.update.call_count == 2

    @pytest.mark.asyncio
    async def test_merge_delist_skips_invalid_rows(self):
        runner, db, _ = _make_runner(
            merge_strategy="update_delist_date",
            target_table="stock_info_ref",
        )
        rows = [
            {"stock_id": "2330", "delisting_date": "2026-01-01"},  # valid
            {"stock_id": None, "delisting_date": "2026-01-02"},     # 缺 id
            {"stock_id": "1234", "delisting_date": None},           # 缺 date
        ]
        await runner._write("__ALL__", "2026-01-01", "2026-02-01", rows)
        # 只一筆 valid → 一次 update
        assert db.update.call_count == 1


class TestProgressStatus:
    """進度標記決策:今日空段不標記(2026-06-09 破洞防回歸)。

    empty 推進水位線(sync_tracker SKIP_STATUSES 含 empty)→ EOD 發布前抓到的
    今日空段一旦標 empty,該日永久跳過。今日空段回 None(不標),下次重抓;
    歷史日空段照標 empty(legit:該股無此類事件)。
    """

    def test_rows_present_completed(self):
        from bronze.segment_runner import _progress_status

        assert _progress_status([{"x": 1}], "2026-06-11", today=date(2026, 6, 11)) == "completed"

    def test_historical_empty_marks_empty(self):
        from bronze.segment_runner import _progress_status

        assert _progress_status([], "2026-06-10", today=date(2026, 6, 11)) == "empty"

    def test_today_empty_not_marked(self):
        from bronze.segment_runner import _progress_status

        assert _progress_status([], "2026-06-11", today=date(2026, 6, 11)) is None

    def test_future_end_empty_not_marked(self):
        from bronze.segment_runner import _progress_status

        assert _progress_status([], "2026-06-12", today=date(2026, 6, 11)) is None

    def test_bad_seg_end_falls_back_empty(self):
        from bronze.segment_runner import _progress_status

        assert _progress_status([], "not-a-date", today=date(2026, 6, 11)) == "empty"


class TestEventLoopNotBlocked:
    """event loop 不被 sync DB 封住:多 task 並發跑時 wall < sum。"""

    @pytest.mark.asyncio
    async def test_concurrent_writes_do_not_serialize_event_loop(self):
        """4 個 task,每個 db.upsert 同步 sleep 40ms。改 to_thread 後 wall
        應 < 4×40ms = 160ms;若仍 sync(會 block event loop)會 ≥ 160ms。"""
        def slow_upsert(*args, **kwargs):
            time.sleep(0.04)  # 模擬 DB I/O,不 yield event loop
            return 1

        runner, db, _ = _make_runner(db_upsert=slow_upsert)
        rows = [{"market": "TW", "stock_id": "2330", "date": "2026-05-15"}]

        start = time.monotonic()
        # 4 個 _write coroutine 並發跑
        await asyncio.gather(*(
            runner._write(f"s{i}", "2026-05-15", "2026-05-15", rows)
            for i in range(4)
        ))
        elapsed = time.monotonic() - start

        # 若 _write 走 to_thread,4 個 sleep 並發 ≈ 40-100ms;若 sync 則 ≥ 160ms
        assert elapsed < 0.13, (
            f"_write 封住 event loop:{elapsed:.3f}s(預期 <0.13s,sync 會 ≥ 0.16s)"
        )

    @pytest.mark.asyncio
    async def test_upsert_runs_on_worker_thread(self):
        """db.upsert 在 worker thread 上跑(asyncio.to_thread default executor)。"""
        captured_tid: list[int] = []
        def capture(*args, **kwargs):
            captured_tid.append(threading.get_ident())
            return 1

        runner, _, _ = _make_runner(db_upsert=capture)
        rows = [{"market": "TW", "stock_id": "2330", "date": "2026-05-15"}]
        main_tid = threading.get_ident()
        await runner._write("2330", "2026-05-15", "2026-05-15", rows)

        assert captured_tid, "db.upsert 沒被呼叫"
        assert captured_tid[0] != main_tid, (
            "db.upsert 仍在 main thread — to_thread 沒生效"
        )
