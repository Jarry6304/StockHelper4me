"""Tests for _SegmentRunner universe filter(all_market 權證排除)。

price_daily / institutional_daily 走 all_market(1 請求/日回全市場含數萬權證),
universe_filter 把 rows 過濾到 stock_resolver 宇宙,行為對齊 per_stock。
"""

from __future__ import annotations

import asyncio
import sys
from pathlib import Path
from unittest.mock import MagicMock

_REPO_ROOT = Path(__file__).resolve().parent.parent.parent
_SRC_ROOT = _REPO_ROOT / "src"
for p in (str(_SRC_ROOT), str(_REPO_ROOT)):
    if p not in sys.path:
        sys.path.insert(0, p)


def _make_runner(universe, *, aggregation=None):
    """建一個 _SegmentRunner,field_mapper 是 MagicMock(由 caller 設 transform 回值)。"""
    from bronze.segment_runner import _SegmentRunner
    from config_loader import ApiConfig

    api = ApiConfig(
        name="price_daily",
        dataset="TaiwanStockPrice",
        param_mode="all_market",
        target_table="price_daily",
        phase=3,
        enabled=True,
        is_backer=True,
        segment_days=1,
        aggregation=aggregation,
        universe_filter=True,
    )
    field_mapper = MagicMock()
    runner = _SegmentRunner(
        api_config=api,
        db=MagicMock(),
        client=MagicMock(),
        field_mapper=field_mapper,
        sync_tracker=MagicMock(),
        get_trading_dates=lambda: set(),
        tracker=MagicMock(),
        sem=asyncio.Semaphore(1),
        dry_run=False,
        universe=universe,
    )
    return runner, field_mapper


class TestUniverseFilter:
    """_transform_and_aggregate 內的 universe 過濾。"""

    def test_drops_rows_outside_universe(self):
        """權證 stock_id 不在宇宙 → 被丟掉,只留宇宙內股票。"""
        runner, fm = _make_runner({"2330", "2317"})
        fm.transform.return_value = (
            [
                {"stock_id": "2330", "date": "2026-05-15", "close": 1000},
                {"stock_id": "2317", "date": "2026-05-15", "close": 100},
                {"stock_id": "030678", "date": "2026-05-15", "close": 1.2},  # 權證
                {"stock_id": "081234", "date": "2026-05-15", "close": 0.5},  # 權證
            ],
            False,
        )
        rows = runner._transform_and_aggregate(
            "__ALL__", "2026-05-15", "2026-05-15", [{}]
        )
        assert {r["stock_id"] for r in rows} == {"2330", "2317"}

    def test_none_universe_keeps_all_rows(self):
        """universe=None(非 universe_filter dataset)→ 不過濾。"""
        runner, fm = _make_runner(None)
        fm.transform.return_value = (
            [{"stock_id": "2330"}, {"stock_id": "030678"}],
            False,
        )
        rows = runner._transform_and_aggregate(
            "__ALL__", "2026-05-15", "2026-05-15", [{}]
        )
        assert len(rows) == 2

    def test_empty_universe_keeps_all_rows(self):
        """股票清單解析失敗 → universe 空 → 不過濾(避免靜默丟光全部資料)。"""
        runner, fm = _make_runner(set())
        fm.transform.return_value = (
            [{"stock_id": "2330"}, {"stock_id": "030678"}],
            False,
        )
        rows = runner._transform_and_aggregate(
            "__ALL__", "2026-05-15", "2026-05-15", [{}]
        )
        assert len(rows) == 2

    def test_filter_runs_before_aggregation(self, monkeypatch):
        """有 aggregation 時,過濾在聚合前發生 — 聚合器只看到宇宙內 rows。"""
        runner, fm = _make_runner({"2330"}, aggregation="pivot_institutional")
        fm.transform.return_value = (
            [
                {"stock_id": "2330", "name": "外資", "buy": 1, "sell": 0},
                {"stock_id": "030678", "name": "外資", "buy": 9, "sell": 9},  # 權證
            ],
            False,
        )
        captured = {}

        def fake_agg(name, rows, **kwargs):
            captured["rows"] = rows
            return rows

        monkeypatch.setattr("bronze.segment_runner.apply_aggregation", fake_agg)
        runner._transform_and_aggregate("__ALL__", "2026-05-15", "2026-05-15", [{}])
        assert {r["stock_id"] for r in captured["rows"]} == {"2330"}
