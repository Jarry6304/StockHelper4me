"""Tests for silver/_common incremental window(Phase 7a 不再 full rebuild)。

7a 非 full_rebuild 時 orchestrator set 窗口:fetch_bronze 只讀 date >= read_since,
upsert_silver 只寫 date >= write_since。READ 窗 > WRITE 窗 → warmup 充足。
"""

from __future__ import annotations

import sys
from datetime import date
from pathlib import Path

import pytest

_REPO_ROOT = Path(__file__).resolve().parent.parent.parent
_SRC_ROOT = _REPO_ROOT / "src"
for p in (str(_SRC_ROOT), str(_REPO_ROOT)):
    if p not in sys.path:
        sys.path.insert(0, p)

from silver._common import (  # noqa: E402
    clear_incremental_window,
    fetch_bronze,
    incremental_read_since,
    set_incremental_window,
    upsert_silver,
)


class _CaptureDB:
    """mock DBWriter:記下 query / upsert 呼叫。"""

    def __init__(self):
        self.queries: list = []
        self.upserts: list = []

    def query(self, sql, params=None):
        self.queries.append((sql, params))
        return []

    def upsert(self, table, batch, pk_cols):
        self.upserts.append((table, list(batch), pk_cols))
        return len(batch)


@pytest.fixture(autouse=True)
def _clean_window():
    """每個 test 前後清窗口,避免 module state 洩漏到別的 test。"""
    clear_incremental_window()
    yield
    clear_incremental_window()


class TestIncrementalWindow:
    def test_no_window_fetch_bronze_has_no_date_filter(self):
        db = _CaptureDB()
        fetch_bronze(db, "price_daily")
        sql, _ = db.queries[0]
        assert "date >=" not in sql

    def test_fetch_bronze_applies_read_since(self):
        db = _CaptureDB()
        set_incremental_window(date(2026, 1, 1), date(2026, 5, 1))
        fetch_bronze(db, "price_daily", stock_ids=["2330"])
        sql, params = db.queries[0]
        assert "date >= %s" in sql
        assert date(2026, 1, 1) in params
        assert "stock_id IN" in sql  # stock_ids filter 與日期過濾並存

    def test_upsert_silver_filters_to_write_window(self):
        db = _CaptureDB()
        set_incremental_window(date(2026, 1, 1), date(2026, 5, 1))
        rows = [
            {"market": "TW", "stock_id": "2330", "date": date(2026, 4, 15)},
            {"market": "TW", "stock_id": "2330", "date": date(2026, 5, 10)},
            {"market": "TW", "stock_id": "2330", "date": date(2026, 5, 20)},
        ]
        written = upsert_silver(db, "t_derived", rows, ["market", "stock_id", "date"])
        assert written == 2  # 04-15 在 write 窗外被濾掉
        upserted = {r["date"] for _, batch, _ in db.upserts for r in batch}
        assert upserted == {date(2026, 5, 10), date(2026, 5, 20)}

    def test_upsert_silver_no_window_writes_all(self):
        db = _CaptureDB()
        rows = [
            {"market": "TW", "stock_id": "2330", "date": date(2020, 1, 1)},
            {"market": "TW", "stock_id": "2330", "date": date(2026, 5, 20)},
        ]
        written = upsert_silver(db, "t_derived", rows, ["market", "stock_id", "date"])
        assert written == 2

    def test_upsert_silver_keeps_rows_without_date(self):
        """row 無 date(防衛)→ 不被窗口濾掉。"""
        db = _CaptureDB()
        set_incremental_window(None, date(2026, 5, 1))
        rows = [{"market": "TW", "key": "x"}]
        written = upsert_silver(db, "t_derived", rows, ["market", "key"])
        assert written == 1

    def test_clear_resets_window(self):
        set_incremental_window(date(2026, 1, 1), date(2026, 5, 1))
        assert incremental_read_since() == date(2026, 1, 1)
        clear_incremental_window()
        assert incremental_read_since() is None
