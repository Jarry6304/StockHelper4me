"""Unit tests for mcp_server.tools.render。

Mock figure_to_png + agg.as_of_with_ohlc → 驗 plumbing,不依賴真實 PG / chromium。
"""

from __future__ import annotations

import sys
from datetime import date
from pathlib import Path

_REPO_ROOT = Path(__file__).resolve().parent.parent.parent
_SRC_ROOT = _REPO_ROOT / "src"
for p in (str(_SRC_ROOT), str(_REPO_ROOT)):
    if p not in sys.path:
        sys.path.insert(0, p)

import pytest

from agg._types import AsOfSnapshot, QueryMetadata
from mcp_server.tools import render as render_tools


# ────────────────────────────────────────────────────────────
# Mock helpers
# ────────────────────────────────────────────────────────────


def _make_snapshot(stock_id="2330", as_of=date(2026, 5, 13)) -> AsOfSnapshot:
    return AsOfSnapshot(
        stock_id=stock_id,
        as_of=as_of,
        facts=[],
        indicator_latest={},
        structural={},
        market={},
        metadata=QueryMetadata(
            stock_id=stock_id,
            as_of=as_of,
            lookback_days=90,
            cores=None,
            include_market=False,
            timeframes=None,
        ),
    )


def _mock_ohlc(n=5) -> list[dict]:
    return [
        {
            "date":   date(2026, 5, 1 + i),
            "open":   100.0 + i,
            "high":   105.0 + i,
            "low":    99.0 + i,
            "close":  103.0 + i,
            "volume": 1_000_000 + i * 100,
        }
        for i in range(n)
    ]


@pytest.fixture
def patch_png(monkeypatch):
    """讓 figure_to_png 回固定 fake bytes(不跑 chromium)。"""
    fake_png = b"\x89PNG\r\n\x1a\n_fake_png_bytes_"

    def fake(_fig, *, width=1280, height=800):
        return fake_png

    monkeypatch.setattr(render_tools, "figure_to_png", fake)
    return fake_png


@pytest.fixture
def patch_fetch(monkeypatch):
    """讓 as_of_with_ohlc 回固定 snapshot + ohlc(不依賴 PG)。"""
    snapshot = _make_snapshot()
    ohlc = _mock_ohlc(5)

    def fake_as_of_with_ohlc(stock_id, as_of_date, **_kwargs):
        snapshot.stock_id = stock_id
        snapshot.as_of = as_of_date
        return snapshot, ohlc

    import agg
    monkeypatch.setattr(agg, "as_of_with_ohlc", fake_as_of_with_ohlc)
    return snapshot, ohlc


# ────────────────────────────────────────────────────────────
# Tests
# ────────────────────────────────────────────────────────────


class TestParseDate:
    def test_iso(self):
        assert render_tools._parse_date("2026-05-13") == date(2026, 5, 13)

    def test_passthrough(self):
        d = date(2026, 5, 13)
        assert render_tools._parse_date(d) == d


class TestRenderKline:
    def test_returns_image_plus_summary(self, patch_png, patch_fetch):
        result = render_tools.render_kline(
            "2330", "2026-05-13", lookback_days=60, indicators=["macd", "rsi"],
        )
        assert isinstance(result, list)
        assert len(result) == 2

        from fastmcp.utilities.types import Image
        assert isinstance(result[0], Image)

        summary = result[1]
        assert summary["stock_id"] == "2330"
        assert summary["as_of"] == "2026-05-13"
        assert summary["lookback_days"] == 60
        assert summary["ohlc_days"] == 5
        assert summary["indicators_rendered"] == ["MACD", "RSI"]

    def test_empty_ohlc_warning(self, patch_png, monkeypatch):
        snapshot = _make_snapshot()

        def fake(stock_id, as_of_date, **_):
            snapshot.stock_id = stock_id
            return snapshot, []  # empty OHLC

        import agg
        monkeypatch.setattr(agg, "as_of_with_ohlc", fake)

        result = render_tools.render_kline("2330", "2026-05-13")
        assert len(result) == 2
        assert "warning" in result[1]
        assert result[1]["warning"] == "no OHLC"


class TestRenderChip:
    def test_basic(self, patch_png, patch_fetch):
        result = render_tools.render_chip("2330", "2026-05-13", lookback_days=60)
        assert len(result) == 2
        summary = result[1]
        assert summary["stock_id"] == "2330"
        assert summary["ohlc_days"] == 5
        assert isinstance(summary["chip_facts"], int)


class TestRenderFundamental:
    @pytest.mark.parametrize("view", ["revenue", "valuation", "financial"])
    def test_each_view(self, view, patch_png, patch_fetch):
        result = render_tools.render_fundamental(
            "2330", "2026-05-13", view=view,
        )
        assert len(result) == 2
        assert result[1]["view"] == view

    def test_unknown_view_raises(self, patch_png, patch_fetch):
        with pytest.raises(ValueError, match="unknown view"):
            render_tools.render_fundamental("2330", "2026-05-13", view="bogus")


class TestRenderEnvironment:
    def test_taiex_view(self, patch_png, patch_fetch):
        result = render_tools.render_environment("2026-05-13", view="taiex")
        assert len(result) == 2
        assert result[1]["view"] == "taiex"

    def test_unknown_view_raises(self, patch_png, patch_fetch):
        with pytest.raises(ValueError, match="unknown view"):
            render_tools.render_environment("2026-05-13", view="bogus")


class TestRenderNeely:
    def test_no_scenarios(self, patch_png, patch_fetch):
        # snapshot 的 structural 是空 dict,無 neely scenarios
        result = render_tools.render_neely("2330", "2026-05-13")
        assert len(result) == 2
        summary = result[1]
        assert summary["scenario_count"] == 0
        assert summary["selected_scenario"] is None


class TestRenderFactsCloud:
    def test_empty_facts(self, patch_png, patch_fetch):
        result = render_tools.render_facts_cloud("2330", "2026-05-13", lookback_days=60)
        assert len(result) == 2
        summary = result[1]
        assert summary["facts_count"] == 0
        # 0 <= 20 → 應該有 facts list(空)
        assert summary["facts"] == []

    def test_filter_cores_passed_through(self, patch_png, patch_fetch):
        result = render_tools.render_facts_cloud(
            "2330", "2026-05-13", source_cores=["macd_core", "rsi_core"],
        )
        assert result[1]["filtered_cores"] == ["macd_core", "rsi_core"]
