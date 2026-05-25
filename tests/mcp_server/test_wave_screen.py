"""Tests for scan_wave_impulse MCP tool (commit 7/7)。

對齊 plan §9 MCP tests + test_screens.py monkeypatch 風格(不打 PG,
mock get_connection + cursor):
  1. top_stocks_present (payload schema)
  2. observe_section_separate (W5 mature 進 observe_stocks)
  3. timeframe_passthrough ("weekly" 正確 forward to SQL)
  4. narrative_caveat_present (narrative + 3 段 caveat)
"""

from __future__ import annotations

import sys
from datetime import date
from pathlib import Path
from unittest.mock import MagicMock, patch

import pytest  # noqa: F401

_REPO_ROOT = Path(__file__).resolve().parent.parent.parent
for p in (str(_REPO_ROOT / "src"), str(_REPO_ROOT)):
    if p not in sys.path:
        sys.path.insert(0, p)


# ════════════════════════════════════════════════════════════
# Helper:mock get_connection + cursor
# ════════════════════════════════════════════════════════════


def _make_mock_conn(*, ranking_date=date(2026, 5, 25),
                    top_rows=None, observe_rows=None):
    """Build mock conn 模擬 _fetch_wave_impulse_rows 內部三段查詢。

    Args:
        ranking_date: 第 1 段 MAX(date) 回傳值;None → 模擬無資料
        top_rows:     第 2 段 SELECT top_n rows
        observe_rows: 第 3 段 SELECT observe rows
    """
    top_rows = top_rows or []
    observe_rows = observe_rows or []
    conn = MagicMock()

    # cursor return 序列
    fetchone_seq = [{"d": ranking_date} if ranking_date else {"d": None}]
    fetchall_seq = [top_rows, observe_rows]
    fa_iter = iter(fetchall_seq)
    fo_iter = iter(fetchone_seq)

    cursor = MagicMock()
    cursor.fetchone = MagicMock(side_effect=lambda: next(fo_iter, None))
    cursor.fetchall = MagicMock(side_effect=lambda: next(fa_iter, []))
    cursor.execute = MagicMock()

    cursor_cm = MagicMock()
    cursor_cm.__enter__ = MagicMock(return_value=cursor)
    cursor_cm.__exit__ = MagicMock(return_value=False)
    conn.cursor = MagicMock(return_value=cursor_cm)
    conn.close = MagicMock()
    return conn, cursor


def _row(stock_id="2330", *, phase="W2_DONE", is_candidate=True, is_top_n=True,
         rank=1, rr=2.5, cross_tf=False, direction="bullish"):
    """Build production-shaped wave_impulse_screen_derived row。"""
    return {
        "market": "TW",
        "stock_id": stock_id,
        "stock_name": f"Stock{stock_id}",
        "industry_category": "半導體",
        "date": date(2026, 5, 25),
        "timeframe": "daily",
        "phase": phase,
        "wave_number": 2 if phase == "W2_DONE" else 3 if phase == "W3_ONGOING" else 5,
        "pattern_kind": "Impulse",
        "direction": direction,
        "effective_degree": "Minor",
        "structure_label": "F3" if phase == "W2_DONE" else "L5",
        "confidence_level": "strict",
        "entry_price": 100.0,
        "target_price": 135.0,
        "invalidation_price": 90.0,
        "rr_ratio": rr,
        "cross_tf_aligned": cross_tf,
        "impulse_rank": rank,
        "universe_size": 50,
        "is_candidate": is_candidate,
        "is_top_n": is_top_n,
    }


# ════════════════════════════════════════════════════════════
# Tests
# ════════════════════════════════════════════════════════════


class TestScanWaveImpulse:

    def test_top_stocks_present_schema(self):
        """plan §9 case 1:top_stocks payload schema 正確壓 LLM-friendly dict。"""
        from mcp_server.tools import data as data_tools

        conn, _cur = _make_mock_conn(
            top_rows=[_row("2330", rank=1), _row("2317", rank=2, cross_tf=True)],
            observe_rows=[],
        )
        with patch("mcp_server._screens.get_connection", return_value=conn):
            r = data_tools.scan_wave_impulse("2026-05-25")
        assert r["timeframe"] == "daily"
        assert r["ranking_date"] == "2026-05-25"
        assert len(r["top_stocks"]) == 2
        # Schema 完整
        first = r["top_stocks"][0]
        for key in ("stock_id", "name", "industry", "rank", "phase", "wave_number",
                    "pattern_kind", "direction", "effective_degree", "structure_label",
                    "confidence_level", "entry_price", "target_price",
                    "invalidation_price", "rr_ratio", "cross_tf_aligned",
                    "is_candidate"):
            assert key in first
        assert first["stock_id"] == "2330"
        assert first["rank"] == 1
        assert first["phase"] == "W2_DONE"
        assert first["cross_tf_aligned"] is False
        # cross_tf_aligned_count 計算正確(2317 是 True)
        assert r["cross_tf_aligned_count"] == 1

    def test_observe_section_separate(self):
        """plan §9 case 2:W5_MATURE rows 進 observe_stocks 不入 top_stocks。"""
        from mcp_server.tools import data as data_tools

        conn, _cur = _make_mock_conn(
            top_rows=[_row("2330", rank=1)],
            observe_rows=[
                _row("3030", phase="W5_MATURE", is_candidate=False,
                     is_top_n=False, rank=None, rr=None),
                _row("2454", phase="W4_DONE", is_candidate=False,
                     is_top_n=False, rank=None, rr=None),
            ],
        )
        with patch("mcp_server._screens.get_connection", return_value=conn):
            r = data_tools.scan_wave_impulse("2026-05-25")
        assert len(r["top_stocks"]) == 1
        assert len(r["observe_stocks"]) == 2
        observe_phases = {s["phase"] for s in r["observe_stocks"]}
        assert "W5_MATURE" in observe_phases
        assert "W4_DONE" in observe_phases
        for s in r["observe_stocks"]:
            assert s["is_candidate"] is False

    def test_timeframe_passthrough_weekly(self):
        """plan §9 case 3:timeframe='weekly' 正確傳到 SQL 三段。"""
        from mcp_server.tools import data as data_tools

        conn, cursor = _make_mock_conn(top_rows=[], observe_rows=[])
        with patch("mcp_server._screens.get_connection", return_value=conn):
            data_tools.scan_wave_impulse("2026-05-25", timeframe="weekly")
        # cursor.execute 收到 3 個 call(date query / top query / observe query)
        # 每個 call 的 params 第一個元素應該是 "weekly"
        assert cursor.execute.call_count == 3
        for call in cursor.execute.call_args_list:
            sql, params = call.args
            assert params[0] == "weekly"

    def test_narrative_and_caveat_present(self):
        """plan §9 case 4:narrative + 三段 caveat 都在。"""
        from mcp_server.tools import data as data_tools

        conn, _cur = _make_mock_conn(top_rows=[_row("2330", rank=1)],
                                     observe_rows=[])
        with patch("mcp_server._screens.get_connection", return_value=conn):
            r = data_tools.scan_wave_impulse("2026-05-25")
        assert "narrative" in r
        assert "Wave Impulse Screen" in r["narrative"]
        assert "2330" in r["narrative"]
        # caveat 三段都在
        caveat = r.get("caveat", "")
        assert "W3" in caveat
        assert "W5" in caveat
        assert "calibrat" in caveat   # production calibration after first 30d

    def test_no_data_graceful(self):
        """ranking_date=None 時不爆,narrative 標示「無 candidates」。"""
        from mcp_server.tools import data as data_tools

        conn, _cur = _make_mock_conn(ranking_date=None,
                                     top_rows=[], observe_rows=[])
        with patch("mcp_server._screens.get_connection", return_value=conn):
            r = data_tools.scan_wave_impulse("2026-05-25")
        assert r["ranking_date"] is None
        assert r["top_stocks"] == []
        assert r["observe_stocks"] == []
        assert "無 top candidates" in r["narrative"]

    def test_include_observe_false_skips_observe(self):
        """include_observe=False 不查 W5 observe section。"""
        from mcp_server.tools import data as data_tools

        conn, cursor = _make_mock_conn(top_rows=[_row("2330", rank=1)],
                                       observe_rows=[])
        with patch("mcp_server._screens.get_connection", return_value=conn):
            r = data_tools.scan_wave_impulse("2026-05-25", include_observe=False)
        # cursor.execute 只 2 call(date + top)— observe SQL 略過
        assert cursor.execute.call_count == 2
        assert r["observe_stocks"] == []

    def test_default_timeframe_is_daily(self):
        from mcp_server.tools import data as data_tools

        conn, cursor = _make_mock_conn(top_rows=[], observe_rows=[])
        with patch("mcp_server._screens.get_connection", return_value=conn):
            data_tools.scan_wave_impulse("2026-05-25")
        first_call = cursor.execute.call_args_list[0]
        sql, params = first_call.args
        assert params[0] == "daily"

    def test_public_surface_exposed(self):
        """確認 scan_wave_impulse function 透過 mcp_server.tools.data 可 import。"""
        from mcp_server.tools import data as data_tools

        assert hasattr(data_tools, "scan_wave_impulse")
        assert callable(data_tools.scan_wave_impulse)
