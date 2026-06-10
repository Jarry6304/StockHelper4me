"""Regression:add_ma_lines 對 ma_core 真實 spec shape(dict)不再 TypeError。

ma_core Rust `MaSpec` 序列化為 `{"kind": "Sma", "period": 20, "source": "Close"}`
(struct,非舊 docstring 假設的 "SMA20" 字串);舊碼拿 dict 當 `_MA_COLOR_MAP`
key → `TypeError: unhashable type: 'dict'`(K-line tab 勾 MA layer 即炸)。
本 test 鎖定 dict spec 組回 "SMA20" 標籤 + 上對應色;字串 spec 向下相容。

plotly 缺裝時整檔 skip(對齊 sandbox 無 plotly)。
"""

from __future__ import annotations

import pytest

pytest.importorskip("plotly")

from plotly.subplots import make_subplots  # noqa: E402

from dashboards.charts._base import PALETTE  # noqa: E402
from dashboards.charts.overlays import add_ma_lines  # noqa: E402


def _fig():
    """對齊 production:K-line 主圖是 make_subplots 產物(row 參照需要 grid)。"""
    return make_subplots(rows=1, cols=1)


def _ma_indicator(spec) -> dict:
    return {
        "value": {
            "series_by_spec": [
                {"spec": spec,
                 "series": [{"date": "2026-06-01", "value": 100.0},
                            {"date": "2026-06-02", "value": 101.5}]},
            ],
        },
    }


def test_dict_spec_builds_label_and_color():
    """MaSpec struct 序列化(真實 shape)→ 標籤 SMA20 + ma20 色,不 raise。"""
    fig = _fig()
    add_ma_lines(fig, _ma_indicator({"kind": "Sma", "period": 20, "source": "Close"}))
    assert len(fig.data) == 1
    assert fig.data[0].name == "SMA20"
    assert fig.data[0].line.color == PALETTE["ma20"]


def test_dict_spec_unmapped_falls_back_default_color():
    fig = _fig()
    add_ma_lines(fig, _ma_indicator({"kind": "Hma", "period": 9, "source": "Close"}))
    assert fig.data[0].name == "HMA9"
    assert fig.data[0].line.color == PALETTE["ma_default"]


def test_string_spec_backward_compat():
    fig = _fig()
    add_ma_lines(fig, _ma_indicator("EMA60"))
    assert fig.data[0].name == "EMA60"
    assert fig.data[0].line.color == "#F57C00"
