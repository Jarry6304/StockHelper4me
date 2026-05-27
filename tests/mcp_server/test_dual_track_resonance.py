"""Tests for mcp_server.tools.data.dual_track_resonance — MCP wrapper。

對齊 m3Spec/dual_track_resonance.md §八「MCP 工具:thin wrapper」+ §十一。
"""

from __future__ import annotations

import sys
from datetime import date
from pathlib import Path
from unittest.mock import patch

import pytest

_REPO_ROOT = Path(__file__).resolve().parent.parent.parent
_SRC_ROOT = _REPO_ROOT / "src"
for p in (str(_SRC_ROOT), str(_REPO_ROOT)):
    if p not in sys.path:
        sys.path.insert(0, p)

from fusion.dual_track._shared import (  # noqa: E402
    DualTrackResult,
    FibLine,
    Track1View,
    Track2Band,
    Track2View,
)
from mcp_server.tools.data import dual_track_resonance  # noqa: E402


def _make_track1():
    return Track1View(
        stock_id="2330",
        as_of=date(2024, 6, 1),
        snapshot_date=date(2024, 5, 30),
        has_snapshot=True,
        pattern_type="Impulse",
        power_rating="StrongBullish",
        direction="bullish",
        effective_degree="Minute",
        wave_count=5,
        fib_lines=[FibLine(price=100.0, low=98.0, high=102.0, label="0.618",
                            source_ratio=0.618)],
        invalidation_price=80.0,
        invalidated=False,
    )


def _make_track2():
    b = Track2Band(
        horizon_days=63, confidence=0.80,
        lower=90.0, upper=110.0, point=100.5,
        source_core="fusion",
        width_ratio=0.20, is_overly_wide=False,
    )
    return Track2View(
        stock_id="2330", as_of=date(2024, 6, 1),
        current_price=100.0,
        primary_horizon=63, primary_confidence=0.80,
        primary_band=b, horizons={63: b},
    )


def _patch_get_connection():
    """Patch get_connection at both sites:resonance.py(原 caller)+ fusion.raw._db
    (v4.28+ MCP wrapper 自己開 conn 設 statement_timeout)。"""
    from unittest.mock import MagicMock
    # MagicMock 自動支援 cur.execute("SET LOCAL ...")等 chain calls
    return patch("fusion.raw._db.get_connection", return_value=MagicMock())


class TestDualTrackResonanceMCP:
    def test_returns_full_dict_schema(self):
        """MCP 工具呼叫回完整 dict schema(對齊 docstring + to_dict())。"""
        t1 = _make_track1()
        t2 = _make_track2()
        with _patch_get_connection(), \
             patch("fusion.dual_track.resonance.read_track1", return_value=t1), \
             patch("fusion.dual_track.resonance.read_track2", return_value=t2), \
             patch("fusion.dual_track.resonance.fetch_is_top_30",
                   return_value=(True, date(2024, 5, 31))), \
             patch("fusion.dual_track.resonance.fetch_latest_close",
                   return_value={"close": 100.0}):
            out = dual_track_resonance("2330", "2024-06-01")

        # 頂層 keys
        assert out["stock_id"] == "2330"
        assert out["as_of"] == "2024-06-01"
        assert "track1" in out and "track2" in out
        assert "is_top_30" in out and out["is_top_30"] is True
        assert out["is_top_30_date"] == "2024-05-31"
        assert "findings" in out and len(out["findings"]) == 1
        assert out["findings"][0]["level"] == "strong"  # covers + median close + is_top_30

    def test_a3_gate_yields_single_track_mode(self):
        t1 = Track1View(
            stock_id="2330", as_of=date(2024, 6, 1),
            snapshot_date=date(2024, 5, 30),
            has_snapshot=True, pattern_type="Impulse",
            power_rating="StrongBullish", direction="bullish",
            effective_degree="Minute", wave_count=5,
            fib_lines=[FibLine(price=100.0, low=98, high=102, label="0.5",
                                source_ratio=0.5)],
            invalidation_price=80.0,
            invalidated=True,  # 模擬現價 < 80
        )
        t2 = _make_track2()
        with _patch_get_connection(), \
             patch("fusion.dual_track.resonance.read_track1", return_value=t1), \
             patch("fusion.dual_track.resonance.read_track2", return_value=t2), \
             patch("fusion.dual_track.resonance.fetch_is_top_30",
                   return_value=(False, None)), \
             patch("fusion.dual_track.resonance.fetch_latest_close",
                   return_value={"close": 75.0}):
            out = dual_track_resonance("2330", "2024-06-01")

        assert out["single_track_mode"] is True
        assert out["findings"] == []
        assert any("A-3 invalidation gate" in n for n in out["notes"])

    def test_passes_through_overrides(self):
        """confidence / horizon / cross_stock_table override 應傳到 underlying resonance()。

        透過捕捉 read_track2 / fetch_is_top_30 的 call kwargs 間接驗證
        (避開 from-import 後的 patch 困難)。
        """
        t1 = _make_track1()
        t2 = _make_track2()
        track2_calls: list[dict] = []
        cross_stock_calls: list[dict] = []

        def _spy_track2(*args, **kwargs):
            track2_calls.append(kwargs)
            return t2

        def _spy_cross_stock(*args, **kwargs):
            cross_stock_calls.append(kwargs)
            return False, None

        with _patch_get_connection(), \
             patch("fusion.dual_track.resonance.read_track1", return_value=t1), \
             patch("fusion.dual_track.resonance.read_track2", side_effect=_spy_track2), \
             patch("fusion.dual_track.resonance.fetch_is_top_30",
                   side_effect=_spy_cross_stock), \
             patch("fusion.dual_track.resonance.fetch_latest_close",
                   return_value={"close": 100.0}):
            dual_track_resonance(
                "2330", "2024-06-01",
                primary_horizon=21, primary_confidence=0.95,
                cross_stock_table="custom_ranked_derived",
            )

        # read_track2 收到 override 的 primary_horizon / primary_confidence
        assert track2_calls, "read_track2 not called"
        assert track2_calls[0]["primary_horizon"] == 21
        assert track2_calls[0]["primary_confidence"] == 0.95

        # fetch_is_top_30 收到 override 的 cross_stock_table
        assert cross_stock_calls, "fetch_is_top_30 not called"
        assert cross_stock_calls[0]["source_table"] == "custom_ranked_derived"

    def test_json_serializable(self):
        """MCP 回傳必為 JSON-serializable(no datetime / Decimal 漏)。"""
        import json
        t1 = _make_track1()
        t2 = _make_track2()
        with _patch_get_connection(), \
             patch("fusion.dual_track.resonance.read_track1", return_value=t1), \
             patch("fusion.dual_track.resonance.read_track2", return_value=t2), \
             patch("fusion.dual_track.resonance.fetch_is_top_30",
                   return_value=(False, None)), \
             patch("fusion.dual_track.resonance.fetch_latest_close",
                   return_value={"close": 100.0}):
            out = dual_track_resonance("2330", "2024-06-01")
        # 不 raise = OK
        json.dumps(out)

    def test_timeout_returns_graceful_error(self):
        """v4.28+ 安全網:QueryCanceled → graceful error dict,不 raise。"""
        from unittest.mock import MagicMock

        class _FakeQueryCanceled(Exception):
            """Mimic psycopg.errors.QueryCanceled 名稱對齊 type(e).__name__ 比對。"""
            pass

        # rename class for type(exc).__name__ string match
        _FakeQueryCanceled.__name__ = "QueryCanceled"

        # Mock conn that raises QueryCanceled when resonance() runs
        mock_conn = MagicMock()
        # 第一次 execute("SET LOCAL ...") OK;第二次以後 resonance() 內讀 facts raise
        # 直接讓 resonance.read_track1 噴 timeout(對齊 production 真實樣態)
        def _raise_timeout(*a, **kw):
            raise _FakeQueryCanceled("statement timeout")

        with patch("fusion.raw._db.get_connection", return_value=mock_conn), \
             patch("fusion.dual_track.resonance.read_track1", side_effect=_raise_timeout), \
             patch("fusion.dual_track.resonance.fetch_latest_close",
                   return_value={"close": 100.0}):
            out = dual_track_resonance("2330", "2024-06-01")

        assert "error" in out
        assert "30s timeout" in out["error"]
        assert out["exception_type"] == "QueryCanceled"
        assert out["stock_id"] == "2330"
        assert out["as_of"] == "2024-06-01"
        assert "diagnostics" in out

    def test_non_timeout_exception_propagates(self):
        """非 QueryCanceled 的 exception 正常 propagate(避免靜默誤判)。"""
        from unittest.mock import MagicMock

        mock_conn = MagicMock()
        with patch("fusion.raw._db.get_connection", return_value=mock_conn), \
             patch("fusion.dual_track.resonance.read_track1",
                   side_effect=ValueError("bad data")), \
             patch("fusion.dual_track.resonance.fetch_latest_close",
                   return_value={"close": 100.0}):
            with pytest.raises(ValueError, match="bad data"):
                dual_track_resonance("2330", "2024-06-01")

    def test_sets_statement_timeout_on_connection(self):
        """v4.28+:wrapper 必須在 acquire conn 後立刻 SET LOCAL statement_timeout。"""
        from unittest.mock import MagicMock, call

        mock_conn = MagicMock()
        mock_cursor = MagicMock()
        mock_conn.cursor.return_value.__enter__.return_value = mock_cursor

        t1 = _make_track1()
        t2 = _make_track2()
        with patch("fusion.raw._db.get_connection", return_value=mock_conn), \
             patch("fusion.dual_track.resonance.read_track1", return_value=t1), \
             patch("fusion.dual_track.resonance.read_track2", return_value=t2), \
             patch("fusion.dual_track.resonance.fetch_is_top_30",
                   return_value=(False, None)), \
             patch("fusion.dual_track.resonance.fetch_latest_close",
                   return_value={"close": 100.0}):
            dual_track_resonance("2330", "2024-06-01")

        # cur.execute("SET LOCAL statement_timeout = '30s'") 必被呼叫
        execute_calls = [c for c in mock_cursor.execute.call_args_list
                         if "statement_timeout" in str(c)]
        assert execute_calls, (
            f"SET LOCAL statement_timeout not called; calls were: "
            f"{mock_cursor.execute.call_args_list}"
        )


class TestPublicSurface:
    def test_dual_track_resonance_in_data_module(self):
        from mcp_server.tools import data

        assert hasattr(data, "dual_track_resonance")
        assert callable(data.dual_track_resonance)

    def test_mcp_server_registers_dual_track(self):
        """server.py 必須有 mcp.tool(_data_tools.dual_track_resonance) 行。"""
        from pathlib import Path

        server_py = (
            Path(__file__).resolve().parent.parent.parent
            / "mcp_server" / "server.py"
        )
        content = server_py.read_text(encoding="utf-8")
        assert "_data_tools.dual_track_resonance" in content, (
            "dual_track_resonance not registered in mcp_server/server.py"
        )
