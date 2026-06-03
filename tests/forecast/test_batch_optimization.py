"""Forecast 校準流水線批次優化 — 批次版 vs 逐筆版等價測 + settle dedup 測。

涵蓋:
  1. `conformalize_batch`(批次路徑:trading-day 迭代 + 記憶體 bisect 切片 +
     `upsert_forecast_batch`)對相同底層資料,寫出與 `conformalize_one`(逐筆路徑:
     per-asof SQL `forecast_date < asof ORDER BY DESC LIMIT window`)**位元相同**的
     calibrated rows。
  2. `resolve_pending` 的 (stock_id, settle_date) memo cache:同 (stock, settle_date)
     跨多 confidence 只讀一次 PIT,且 hit/pinball/realized 與逐筆版一致。
  3. `upsert_forecast_batch` / `update_settlement_batch` 的 UNNEST SQL 組裝(純字串)。

均為純函式 / mock,沙箱無 DB 可跑。
"""

from __future__ import annotations

import sys
from contextlib import contextmanager
from datetime import date, timedelta
from pathlib import Path
from unittest.mock import MagicMock, patch

_REPO_ROOT = Path(__file__).resolve().parent.parent.parent
_SRC_ROOT = _REPO_ROOT / "src"
for _p in (str(_SRC_ROOT), str(_REPO_ROOT)):
    if _p not in sys.path:
        sys.path.insert(0, _p)

from forecast import calibration  # noqa: E402
from forecast.calibration import conformalize_batch, conformalize_one  # noqa: E402
from forecast.settlement import resolve_pending  # noqa: E402


class _FakeConn:
    """no-op transaction context manager(批次版 `with conn.transaction():`)。"""

    @contextmanager
    def transaction(self):
        yield


# ─── 1. conformalize_batch vs conformalize_one 等價 ───────────────────────────


def _settled(fdate, lower, upper, realized, conf=0.80):
    return {"lower": lower, "upper": upper, "realized_price": realized,
            "forecast_date": fdate, "confidence": conf}


def _raw(fdate, lower=90.0, upper=110.0, point=100.0, conf=0.80):
    return {"forecast_date": fdate, "lower": lower, "upper": upper,
            "point": point, "confidence": conf, "params_hash": "raw"}


def _reference_calibration_set(settled, asof, window):
    """逐筆版 `_fetch_calibration_set` 的獨立 SQL 語意參考實作:
    forecast_date < asof,ORDER BY forecast_date DESC,LIMIT window。"""
    eligible = [r for r in settled if r["forecast_date"] < asof]
    eligible.sort(key=lambda r: r["forecast_date"], reverse=True)
    return eligible[:window]


def _run_one_path(*, trading_days, raw_by_date, settled, window, min_cal):
    """跑逐筆 conformalize_one 蒐集寫出的 rows。"""
    written: list[dict] = []
    with patch.object(calibration, "_fetch_raw_forecast",
                      side_effect=lambda conn, sid, asof, h, c, core: raw_by_date.get(asof)), \
         patch.object(calibration, "_fetch_calibration_set",
                      side_effect=lambda conn, sid, asof, h, c, core, w: _reference_calibration_set(settled, asof, w)), \
         patch.object(calibration, "upsert_forecast",
                      side_effect=lambda conn, row: written.append(row)):
        for asof in trading_days:
            conformalize_one(
                None, stock_id="2330", asof=asof, horizon_days=21,
                confidence=0.80, calibration_window=window,
                min_calibration_size=min_cal,
            )
    return written


def _run_batch_path(*, trading_days, raw_by_date, settled, start, end, window, min_cal):
    """跑批次 conformalize_batch 蒐集寫出的 rows。"""
    written: list[dict] = []
    with patch.object(calibration, "_fetch_trading_days", return_value=list(trading_days)), \
         patch.object(calibration, "_fetch_raw_forecasts_range", return_value=dict(raw_by_date)), \
         patch.object(calibration, "_fetch_calibration_history",
                      side_effect=lambda conn, sid, h, c, core, before: [r for r in settled if r["forecast_date"] < before]), \
         patch.object(calibration, "upsert_forecast_batch",
                      side_effect=lambda conn, rows: written.extend(rows)):
        conformalize_batch(
            _FakeConn(), stock_ids=["2330"], start=start, end=end,
            horizons=[21], confidences=[0.80],
            calibration_window=window, min_calibration_size=min_cal,
        )
    return written


def _key(row):
    return (row["forecast_date"], row["horizon_days"], row["confidence"])


class TestConformalizeBatchEquivalence:
    def test_batch_matches_one_bit_for_bit(self):
        base = date(2024, 1, 1)
        # 20 連續日當 trading days(等價測不在乎是否真為交易日)
        trading_days = [base + timedelta(days=i) for i in range(20)]
        # 早於 trading window 的 settled 校準史 + 部分 trading days 本身也 settle
        settled = [
            _settled(base - timedelta(days=30) + timedelta(days=i),
                     90.0, 110.0, 95.0 + (i % 7))  # 變動 realized → 變動 q
            for i in range(40)
        ]
        # 每個 trading day 有 raw,但故意挖掉第 5 天 → 觸發 no_raw
        raw_by_date = {d: _raw(d) for i, d in enumerate(trading_days) if i != 5}
        window, min_cal = 10, 5

        one_rows = _run_one_path(
            trading_days=trading_days, raw_by_date=raw_by_date,
            settled=settled, window=window, min_cal=min_cal,
        )
        batch_rows = _run_batch_path(
            trading_days=trading_days, raw_by_date=raw_by_date, settled=settled,
            start=trading_days[0], end=trading_days[-1],
            window=window, min_cal=min_cal,
        )

        assert len(one_rows) > 0  # 確實有寫
        assert sorted(one_rows, key=_key) == sorted(batch_rows, key=_key)

    def test_batch_respects_window_slice(self):
        """window 切片:批次 bisect 取的「最近 window 筆」與逐筆 SQL LIMIT 同集合
        → q(進 lower/upper)位元相同。用大 history + 小 window 放大切點差異。"""
        base = date(2024, 6, 1)
        trading_days = [base + timedelta(days=i) for i in range(10)]
        # 100 筆 settled,realized 隨日期單調變化 → 不同 window 切點 → 不同 q
        settled = [
            _settled(base - timedelta(days=120) + timedelta(days=i),
                     90.0, 110.0, 80.0 + i * 0.5)
            for i in range(100)
        ]
        raw_by_date = {d: _raw(d) for d in trading_days}
        window, min_cal = 7, 3

        one_rows = _run_one_path(
            trading_days=trading_days, raw_by_date=raw_by_date,
            settled=settled, window=window, min_cal=min_cal,
        )
        batch_rows = _run_batch_path(
            trading_days=trading_days, raw_by_date=raw_by_date, settled=settled,
            start=trading_days[0], end=trading_days[-1],
            window=window, min_cal=min_cal,
        )
        assert sorted(one_rows, key=_key) == sorted(batch_rows, key=_key)

    def test_empty_trading_days_writes_nothing(self):
        written: list[dict] = []
        with patch.object(calibration, "_fetch_trading_days", return_value=[]), \
             patch.object(calibration, "upsert_forecast_batch",
                          side_effect=lambda conn, rows: written.extend(rows)):
            totals = conformalize_batch(
                _FakeConn(), stock_ids=["2330"],
                start=date(2024, 1, 1), end=date(2024, 1, 10),
            )
        assert written == []
        assert totals == {}


# ─── 2. settle memo cache + 等價 ──────────────────────────────────────────────


def _pending(*, id, stock_id, forecast_date, horizon, lower, upper, c):
    return {"id": id, "stock_id": stock_id, "forecast_date": forecast_date,
            "horizon_days": horizon, "lower": lower, "upper": upper,
            "point": (lower + upper) / 2, "confidence": c,
            "calibrated": False, "source_core": "baseline",
            "regime_tag": None, "params_hash": "h"}


class TestSettleBatchAndDedup:
    def test_pit_read_deduped_across_confidences(self):
        """同 (stock, forecast_date, horizon) 跨 3 個 confidence → 同 settle_date →
        asof_close_series 只該被呼叫 1 次(memo cache)。"""
        fd = date(2024, 1, 1)
        pending = [
            _pending(id=i, stock_id="2330", forecast_date=fd, horizon=21,
                     lower=90, upper=110, c=c)
            for i, c in enumerate([0.50, 0.80, 0.95])
        ]
        updates: list[dict] = []
        pit = MagicMock(return_value=[{"date": date(2024, 1, 22),
                                       "asof_adj_close": 100.0}])
        with patch("forecast.settlement.fetch_unresolved", return_value=pending), \
             patch("forecast.settlement.asof_close_series", pit), \
             patch("forecast.settlement.update_settlement_batch",
                   side_effect=lambda conn, ups: updates.extend(ups)):
            summary = resolve_pending(conn=_FakeConn(), asof=date(2024, 2, 1))

        assert pit.call_count == 1          # 3 row 共用 1 次 PIT 讀
        assert summary["settled"] == 3
        assert len(updates) == 3
        assert all(u["realized_price"] == 100.0 for u in updates)
        # 不同 confidence 的 hit 都 True(100 ∈ [90,110]),pinball 隨 conf 變
        assert all(u["hit"] is True for u in updates)

    def test_distinct_settle_dates_not_deduped(self):
        """不同 horizon → 不同 settle_date → PIT 各讀一次。"""
        fd = date(2024, 1, 1)
        pending = [
            _pending(id=1, stock_id="2330", forecast_date=fd, horizon=21,
                     lower=90, upper=110, c=0.80),
            _pending(id=2, stock_id="2330", forecast_date=fd, horizon=63,
                     lower=90, upper=110, c=0.80),
        ]
        pit = MagicMock(return_value=[{"date": date(2024, 3, 4),
                                       "asof_adj_close": 100.0}])
        with patch("forecast.settlement.fetch_unresolved", return_value=pending), \
             patch("forecast.settlement.asof_close_series", pit), \
             patch("forecast.settlement.update_settlement_batch",
                   side_effect=lambda conn, ups: None):
            resolve_pending(conn=_FakeConn(), asof=date(2024, 6, 1))
        assert pit.call_count == 2


# ─── 3. batch UNNEST SQL 組裝(純字串契約)────────────────────────────────────


class TestBatchSqlContract:
    def test_upsert_batch_columns_shape(self):
        from forecast._db import _forecast_batch_columns
        rows = [
            {"stock_id": "2330", "forecast_date": date(2024, 1, 1), "horizon_days": 21,
             "lower": 1.0, "upper": 2.0, "point": 1.5, "confidence": 0.8,
             "source_core": "kalman_cqr", "params_hash": "p"},
            {"stock_id": "1101", "forecast_date": date(2024, 1, 2), "horizon_days": 63,
             "confidence": 0.95, "source_core": "kalman_cqr"},
        ]
        cols = _forecast_batch_columns(rows)
        assert len(cols) == 13                       # 13 欄
        assert all(len(c) == 2 for c in cols)        # 每欄 2 列
        # per-row 預設對齊 upsert_forecast
        assert cols[7] == [False, False]             # calibrated 預設 False
        assert cols[8] == [False, False]             # internal_only 預設 False
        assert cols[12] == ["b1", "b1"]              # logic_version 預設 b1
        assert cols[3] == [1.0, None]                # lower(第二筆缺 → None)

    def test_upsert_batch_sql_has_conflict_guard(self):
        from forecast._db import _UPSERT_FORECAST_BATCH_SQL as sql
        assert "UNNEST(" in sql
        assert "ON CONFLICT (stock_id, forecast_date, horizon_days, source_core, confidence)" in sql
        # B1 logic_version CASE guard 與單筆版一致
        assert "CASE" in sql and "resolved_date IS NULL" in sql
        assert "EXCLUDED.logic_version" in sql
        assert "forecast_log.logic_version" in sql

    def test_update_settlement_batch_sql(self):
        from forecast._db import _UPDATE_SETTLEMENT_BATCH_SQL as sql
        assert "UPDATE forecast_log" in sql
        assert "UNNEST(" in sql
        assert "WHERE f.id = u.row_id" in sql
        for col in ("resolved_date", "realized_price", "hit", "pinball_loss"):
            assert col in sql

    def test_upsert_batch_empty_noop(self):
        from forecast._db import upsert_forecast_batch, update_settlement_batch
        assert upsert_forecast_batch(MagicMock(), []) == 0
        assert update_settlement_batch(MagicMock(), []) == 0
