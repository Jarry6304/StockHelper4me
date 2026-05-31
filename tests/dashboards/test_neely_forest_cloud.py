"""v4.33 — NeelyWave 複合雲圖 builder + Monowave zigzag 欄位修復測試。"""

from __future__ import annotations

import plotly.graph_objects as go

from dashboards.charts import neely_wave, overlays
from dashboards.charts._base import make_kline_subplots, neely_monowave_points


# ────────────────────────────────────────────────────────────
# fixtures(production-shape monowave_series)
# ────────────────────────────────────────────────────────────

def _mw(n: int = 3) -> list[dict]:
    """N 段方向性 monowave(start_date/end_date/start_price/end_price)。"""
    base = [
        ("2024-01-01", "2024-02-01", 100.0, 120.0, "Up", "1"),
        ("2024-02-01", "2024-03-01", 120.0, 110.0, "Down", "2"),
        ("2024-03-01", "2024-04-01", 110.0, 140.0, "Up", "3"),
    ]
    return [
        {"start_date": s, "end_date": e, "start_price": sp,
         "end_price": ep, "direction": d, "label": lb}
        for s, e, sp, ep, d, lb in base[:n]
    ]


def _forest_bullish() -> list[dict]:
    return [
        {"id": "s0", "power_rating": "StrongBullish", "rules_passed_count": 7,
         "wave_tree": {"start": "2022-01-01", "end": "2024-04-01"},
         "monowave_labels": ["1", "2", "3"],
         "expected_fib_zones": [
             {"label": "0.618", "low": 150, "high": 160, "source_ratio": 0.618},
             {"label": "1.0", "low": 175, "high": 185, "source_ratio": 1.0},
             {"label": "1.382", "low": 200, "high": 215, "source_ratio": 1.382},
         ],
         "invalidation_triggers": [
             {"on_trigger": "InvalidateScenario", "trigger_type": {"PriceBreakBelow": 108.0}}
         ]},
        {"id": "s1", "power_rating": "Bullish", "rules_passed_count": 5,
         "wave_tree": {"start": "2023-06-01", "end": "2024-04-01"},
         "expected_fib_zones": [
             {"label": "0.618", "low": 152, "high": 158, "source_ratio": 0.618}
         ]},
    ]


def _ohlc(n: int = 10) -> list[dict]:
    return [
        {"date": f"2024-04-{d:02d}", "open": 138, "high": 142, "low": 136, "close": 140.0}
        for d in range(1, n + 1)
    ]


def _structural(forest=None, mw=None) -> dict:
    return {"snapshot": {"scenario_forest": forest if forest is not None else _forest_bullish(),
                         "monowave_series": mw if mw is not None else _mw()}}


# ────────────────────────────────────────────────────────────
# Zigzag 欄位修復
# ────────────────────────────────────────────────────────────

def test_zigzag_points_production_shape_n_plus_1():
    mw = _mw(3)
    xs, ys = neely_monowave_points(mw)
    assert len(xs) == 4  # N+1
    assert ys == [100.0, 120.0, 110.0, 140.0]  # start_price + 連續 end_price


def test_zigzag_points_old_shape_returns_empty():
    """舊 start/price_range shape 不再被讀(production 修復 regression guard)。"""
    old = [{"start": "2024-01-01", "end": "2024-02-01",
            "price_range": {"low": 100, "high": 120}}]
    xs, ys = neely_monowave_points(old)
    assert xs == [] and ys == []


def test_zigzag_points_empty_input():
    assert neely_monowave_points([]) == ([], [])
    assert neely_monowave_points(None) == ([], [])


def test_overlay_add_neely_zigzag_draws_for_production_shape():
    fig = make_kline_subplots(1, [1.0])
    overlays.add_neely_zigzag(fig, _structural(), show_fib_zones=True)
    zz = [t for t in fig.data if t.name == "Neely zigzag"]
    assert zz and len(zz[0].y) == 4


def test_overlay_no_price_range_key_remnant():
    """舊 shape 對 overlay 畫空(修復前會誤畫中點;修復後 0 zigzag trace)。"""
    fig = make_kline_subplots(1, [1.0])
    old = {"snapshot": {"monowave_series": [
        {"start": "2024-01-01", "end": "2024-02-01", "price_range": {"low": 100, "high": 120}}]}}
    overlays.add_neely_zigzag(fig, old)
    assert [t for t in fig.data if t.name == "Neely zigzag"] == []


def test_deep_dive_zigzag_production_shape():
    fig = neely_wave.build_neely_deep_dive(_ohlc(), _structural())
    zz = [t for t in fig.data if t.name == "Monowaves"]
    assert zz and len(zz[0].y) == 4


# ────────────────────────────────────────────────────────────
# Layer 2 密度雲
# ────────────────────────────────────────────────────────────

def test_density_overlap_bin_deeper_than_non_overlap():
    # 兩 scenario fib zones 在同價位 [150,160] 重疊 + 一處不重疊
    forest = [
        {"power_rating": "StrongBullish",
         "expected_fib_zones": [{"low": 150, "high": 160, "source_ratio": 0.618}]},
        {"power_rating": "Bullish",
         "expected_fib_zones": [{"low": 150, "high": 160, "source_ratio": 0.618},
                                {"low": 200, "high": 210, "source_ratio": 1.382}]},
    ]
    heat, any_zone = neely_wave._build_density_heat(forest, 100.0, 250.0)
    assert any_zone
    span = 250.0 - 100.0

    def _bin(p):
        return int((p - 100.0) / span * neely_wave._CLOUD_N_BINS)
    overlap = heat[_bin(155)]
    non_overlap = heat[_bin(205)]
    assert overlap > non_overlap  # 重疊區 power 累加更深


def test_density_strength_zero_no_contribution():
    forest = [
        {"power_rating": "Neutral",  # strength 0 → 跳過
         "expected_fib_zones": [{"low": 150, "high": 160, "source_ratio": 0.618}]},
    ]
    heat, any_zone = neely_wave._build_density_heat(forest, 100.0, 250.0)
    assert any_zone is False
    assert sum(heat) == 0.0


def test_cloud_sparse_when_all_scenario_fib_empty_adds_note():
    forest = [{"power_rating": "StrongBullish", "expected_fib_zones": [],
               "wave_tree": {"start": "2022-01-01", "end": "2024-04-01"}}]
    fig = neely_wave.build_neely_forest_cloud(
        _ohlc(), _structural(forest=forest), current_price=140.0, show_resonance=False,
    )
    # 不退 flat 假裝密集 → 無 Heatmap trace + note annotation
    assert not [t for t in fig.data if isinstance(t, go.Heatmap)]
    texts = " ".join(a.text for a in fig.layout.annotations if a.text)
    assert "稀疏" in texts


def test_cloud_has_heatmap_when_zones_present():
    fig = neely_wave.build_neely_forest_cloud(
        _ohlc(), _structural(), current_price=140.0, show_resonance=False,
    )
    assert [t for t in fig.data if isinstance(t, go.Heatmap)]


# ────────────────────────────────────────────────────────────
# Layer 3 合成波 + 失效
# ────────────────────────────────────────────────────────────

def test_synth_bullish_median_slopes_up_and_labeled():
    fig = neely_wave.build_neely_forest_cloud(
        _ohlc(), _structural(), current_price=140.0, show_resonance=False,
    )
    synth = [t for t in fig.data if t.name == "synthesized projection"]
    assert synth
    ys = list(synth[0].y)
    assert ys[-1] > ys[0]  # bullish 向上
    texts = " ".join(a.text for a in fig.layout.annotations if a.text)
    assert "synthesized" in texts


def test_synth_empty_fib_fallback_no_raise():
    forest = [{"id": "s0", "power_rating": "StrongBullish",
               "wave_tree": {"start": "2022-01-01", "end": "2024-04-01"},
               "expected_fib_zones": []}]
    # 不應 raise;synth path 走 project_range fallback
    fig = neely_wave.build_neely_forest_cloud(
        _ohlc(), _structural(forest=forest), current_price=140.0, show_resonance=False,
    )
    assert isinstance(fig, go.Figure)


def test_invalidation_line_and_gauge():
    fig = neely_wave.build_neely_forest_cloud(
        _ohlc(), _structural(), current_price=140.0, show_resonance=False,
    )
    # add_hline → shape + annotation 含 buffer %
    texts = " ".join(a.text for a in fig.layout.annotations if a.text)
    assert "失效" in texts and "%" in texts


# ────────────────────────────────────────────────────────────
# 共振疊層
# ────────────────────────────────────────────────────────────

def _res(single: bool = False) -> dict:
    return {
        "single_track_mode": single,
        "track2": {"horizons": {
            "21": {"lower": 130, "upper": 160, "point": 145},
            "63": {"lower": 120, "upper": 190, "point": 155},
        }},
        "findings": [
            {"fib_line": {"price": 155.0}, "level": "strong"},
            {"fib_line": {"price": 180.0}, "level": "basic"},
        ],
    }


def test_resonance_overlay_draws_track2_bands():
    fig = neely_wave.build_neely_forest_cloud(
        _ohlc(), _structural(), current_price=140.0, resonance_result=_res(),
    )
    assert [t for t in fig.data if t.name and t.name.startswith("Track2")]


def test_resonance_single_track_only_track2_with_banner():
    fig = neely_wave.build_neely_forest_cloud(
        _ohlc(), _structural(), current_price=140.0, resonance_result=_res(single=True),
    )
    assert [t for t in fig.data if t.name and t.name.startswith("Track2")]
    texts = " ".join(a.text for a in fig.layout.annotations if a.text)
    assert "A-3" in texts or "軌道一退場" in texts


def test_resonance_none_adds_skip_note():
    fig = neely_wave.build_neely_forest_cloud(
        _ohlc(), _structural(), current_price=140.0, resonance_result=None, show_resonance=True,
    )
    texts = " ".join(a.text for a in fig.layout.annotations if a.text)
    assert "共振" in texts


# ────────────────────────────────────────────────────────────
# 降級
# ────────────────────────────────────────────────────────────

def test_no_structural_placeholder_no_raise():
    fig = neely_wave.build_neely_forest_cloud(None, None)
    assert isinstance(fig, go.Figure)
    texts = " ".join(a.text for a in fig.layout.annotations if a.text)
    assert "無 neely" in texts


def test_no_scenario_forest_placeholder():
    fig = neely_wave.build_neely_forest_cloud(_ohlc(), {"snapshot": {"scenario_forest": []}})
    texts = " ".join(a.text for a in fig.layout.annotations if a.text)
    assert "scenario_forest" in texts or "無 neely" in texts


def test_empty_monowave_layer1_skipped_others_drawn():
    fig = neely_wave.build_neely_forest_cloud(
        _ohlc(), _structural(mw=[]), current_price=140.0, show_resonance=False,
    )
    # zigzag 跳過但 heatmap / synth 仍畫
    assert not [t for t in fig.data if t.name == "Monowave zigzag"]
    assert [t for t in fig.data if isinstance(t, go.Heatmap)]
