"""Golden L3 fusion 物化單元測試(mock db + conn + fusion 函式,不依賴真實 PG)。"""

from datetime import date
from unittest.mock import MagicMock

import fusion.materialize._provenance as P
import fusion.materialize.climate_stage as cs
import fusion.materialize.fusion_stage as fs
from fusion.materialize.climate_stage import run_climate_materialize
from fusion.materialize.fusion_stage import run_fusion_materialize
from fusion.materialize.read import fetch_fusion_doc


class FakeDB:
    """擷取 db.upsert 呼叫,回 len(rows)。"""

    def __init__(self):
        self.calls: list[tuple[str, list[dict], list[str]]] = []

    def upsert(self, table, rows, pk):
        self.calls.append((table, [dict(r) for r in rows], list(pk)))
        return len(rows)


class FakeCursor:
    def __init__(self, rows):
        self._rows = rows
        self.sql = None
        self.params = None

    def __enter__(self):
        return self

    def __exit__(self, *a):
        return False

    def execute(self, sql, params=None):
        self.sql = sql
        self.params = params

    def fetchone(self):
        return self._rows[0] if self._rows else None

    def fetchall(self):
        return list(self._rows)


class FakeConn:
    def __init__(self, rows=None):
        self._rows = rows or []
        self.closed = False

    def cursor(self):
        return FakeCursor(self._rows)

    def close(self):
        self.closed = True


# ── params_hash / 常數 canonical ───────────────────────────────────────────
def test_params_hashes_canonical():
    assert P.levels_params_hash() == "lv|top20|lb120"
    assert P.resonance_params_hash() == "rz|h63|c0.80|tol0.02|H21-63-126"
    assert P.climate_params_hash() == "cl|lb60"


def test_sentinels_and_derived_from():
    assert P.LEVELS_CORE == "levels_fusion"
    assert P.RESONANCE_CORE == "resonance_fusion"
    assert P.CLIMATE_CORE == "climate_fusion"
    assert P.LEVELS_TIMEFRAME == "_all_"
    assert P.CLIMATE_STOCK_ID == "_market_"
    assert P.RESONANCE_TIMEFRAMES == ("daily", "weekly", "monthly")
    # derived_from 純 core 名 CSV(嚴守規格)— 不含 date/version
    assert "@" not in P.LEVELS_DERIVED_FROM
    assert P.LEVELS_DERIVED_FROM == "support_resistance_core,trendline_core,neely_core"


def test_build_row_keys():
    row = P.build_row(
        stock_id="2330", snapshot_date=date(2026, 5, 28), timeframe="_all_",
        core_name="levels_fusion", source_version="levels_v1",
        params_hash="lv|top20|lb120", snapshot={"a": 1},
        derived_from_core=P.LEVELS_DERIVED_FROM,
    )
    assert set(row) == {
        "stock_id", "snapshot_date", "timeframe", "core_name",
        "source_version", "params_hash", "snapshot", "derived_from_core",
    }


# ── fusion_stage: levels + resonance ───────────────────────────────────────
def _patch_fusion(monkeypatch, *, universe, lag=0):
    monkeypatch.setattr(fs, "get_connection", lambda *a, **k: FakeConn())
    monkeypatch.setattr(P, "latest_trading_date", lambda conn: date(2026, 5, 28))
    monkeypatch.setattr(P, "fetch_universe", lambda conn, stocks=None: list(stocks or universe))
    monkeypatch.setattr(P, "forecast_log_lag_days", lambda conn, as_of: lag)

    class _Res:
        def to_dict(self):
            return {"track1": {}, "track2": {}, "findings": []}

    monkeypatch.setattr(
        fs, "key_levels",
        lambda sid, asof, conn=None: {"stock_id": sid, "levels": [], "level_count": 0},
    )
    monkeypatch.setattr(
        fs, "resonance",
        lambda sid, asof, timeframe="daily", conn=None: _Res(),
    )


def test_fusion_materialize_levels_and_resonance(monkeypatch):
    _patch_fusion(monkeypatch, universe=["2330", "1101"])
    db = FakeDB()
    s = run_fusion_materialize(db, as_of=date(2026, 5, 28))
    assert s["levels_written"] == 2            # 2 stocks × 1 (_all_)
    assert s["resonance_written"] == 6         # 2 stocks × 3 tf
    assert s["errors"] == 0
    assert s["as_of"] == "2026-05-28"
    # 兩批 upsert 都打 structural_snapshots
    assert [c[0] for c in db.calls] == ["structural_snapshots", "structural_snapshots"]
    lv = db.calls[0][1][0]
    assert lv["core_name"] == "levels_fusion" and lv["timeframe"] == "_all_"
    assert lv["source_version"] == "levels_v1"
    assert lv["params_hash"] == P.levels_params_hash()
    assert lv["derived_from_core"] == P.LEVELS_DERIVED_FROM
    assert db.calls[0][2] == P.PK_COLS
    rz_tfs = {r["timeframe"] for r in db.calls[1][1]}
    assert rz_tfs == {"daily", "weekly", "monthly"}
    assert db.calls[1][1][0]["core_name"] == "resonance_fusion"


def test_fusion_materialize_only_levels(monkeypatch):
    _patch_fusion(monkeypatch, universe=["2330"])
    db = FakeDB()
    s = run_fusion_materialize(db, only={"levels"})
    assert s["levels_written"] == 1
    assert s["resonance_written"] == 0


def test_fusion_materialize_backfill_skips_existing(monkeypatch):
    _patch_fusion(monkeypatch, universe=["2330"])
    monkeypatch.setattr(P, "fusion_row_exists", lambda *a, **k: True)
    db = FakeDB()
    s = run_fusion_materialize(db, backfill=True)
    assert s["levels_written"] == 0
    assert s["resonance_written"] == 0
    assert s["skipped"] == 4          # 1 levels + 3 resonance tf
    assert db.calls == []


def test_fusion_materialize_per_stock_graceful(monkeypatch):
    _patch_fusion(monkeypatch, universe=["2330", "BAD"])

    def _kl(sid, asof, conn=None):
        if sid == "BAD":
            raise RuntimeError("boom")
        return {"stock_id": sid, "levels": []}

    monkeypatch.setattr(fs, "key_levels", _kl)
    monkeypatch.setattr(fs, "resonance",
                        lambda sid, asof, timeframe="daily", conn=None:
                        type("R", (), {"to_dict": lambda self: {}})())
    db = FakeDB()
    s = run_fusion_materialize(db, only={"levels"})
    assert s["levels_written"] == 1   # 只 2330 成功
    assert s["errors"] == 1           # BAD 計入 errors,不中斷


def test_fusion_materialize_forecast_stale_warning(monkeypatch):
    _patch_fusion(monkeypatch, universe=["2330"], lag=None)  # forecast_log 無 external
    db = FakeDB()
    s = run_fusion_materialize(db)
    assert any("forecast_log 無 external" in w for w in s["warnings"])


def test_fusion_materialize_no_price_data(monkeypatch):
    monkeypatch.setattr(fs, "get_connection", lambda *a, **k: FakeConn())
    monkeypatch.setattr(P, "latest_trading_date", lambda conn: None)
    db = FakeDB()
    s = run_fusion_materialize(db)
    assert s["levels_written"] == 0 and s["resonance_written"] == 0
    assert any("price_daily 無資料" in w for w in s["warnings"])


# ── climate_stage(marketwide,獨立 aggregator)──────────────────────────────
def test_climate_materialize(monkeypatch):
    monkeypatch.setattr(cs, "get_connection", lambda *a, **k: FakeConn())
    monkeypatch.setattr(P, "latest_trading_date", lambda conn: date(2026, 5, 28))
    import mcp_server._climate as climate_mod
    monkeypatch.setattr(
        climate_mod, "compute_market_context",
        lambda asof, lookback_days=60, conn=None: {
            "climate_score": 1.2, "overall_climate": "bullish",
        },
    )
    db = FakeDB()
    s = run_climate_materialize(db)
    assert s["written"] == 1
    assert s["overall_climate"] == "bullish"
    row = db.calls[0][1][0]
    assert row["stock_id"] == "_market_"
    assert row["core_name"] == "climate_fusion"
    assert row["timeframe"] == "_all_"
    assert row["source_version"] == "climate_v1"


# ── read.fetch_fusion_doc ───────────────────────────────────────────────────
def test_fetch_fusion_doc_returns_row():
    conn = FakeConn([{"snapshot": {"levels": []}, "snapshot_date": date(2026, 5, 28),
                      "timeframe": "_all_", "source_version": "levels_v1",
                      "params_hash": "lv|top20|lb120"}])
    out = fetch_fusion_doc(conn, stock_id="2330", as_of=date(2026, 5, 28),
                           core_name="levels_fusion")
    assert out is not None and out["snapshot"] == {"levels": []}


def test_fetch_fusion_doc_none_when_missing():
    conn = FakeConn([])
    out = fetch_fusion_doc(conn, stock_id="2330", as_of=date(2026, 5, 28),
                           core_name="resonance_fusion", timeframe="daily")
    assert out is None


def test_fetch_fusion_doc_adds_timeframe_filter():
    conn = FakeConn([{"snapshot": {}}])
    cur_holder = {}
    real_cursor = conn.cursor

    def _cursor():
        c = real_cursor()
        cur_holder["c"] = c
        return c

    conn.cursor = _cursor
    fetch_fusion_doc(conn, stock_id="2330", as_of=date(2026, 5, 28),
                     core_name="resonance_fusion", timeframe="weekly")
    assert "AND timeframe = %s" in cur_holder["c"].sql
    assert cur_holder["c"].params[-1] == "weekly"
