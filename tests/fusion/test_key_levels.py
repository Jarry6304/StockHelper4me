"""Fusion Layer · key_levels 單元測試(monkeypatch fetch,不依賴真實 PG)。"""

from datetime import date
from unittest.mock import MagicMock

import fusion.key_levels as kl_mod
from fusion._shared import cluster_price_levels
from fusion.key_levels import key_levels


def test_cluster_groups_within_1pct():
    pts = [
        {"price": 100.0, "source": "a"},
        {"price": 100.5, "source": "b"},
        {"price": 120.0, "source": "a"},
    ]
    out = cluster_price_levels(pts)
    assert len(out) == 2
    assert out[0]["strength"] == 2  # 100 & 100.5 → 2 distinct sources
    assert out[0]["member_count"] == 2
    assert out[1]["price"] == 120.0


def test_cluster_skips_bad_prices():
    out = cluster_price_levels([
        {"price": 0, "source": "a"},
        {"price": -5, "source": "b"},
        {"price": None, "source": "c"},
        {"price": True, "source": "d"},
    ])
    assert out == []


def test_key_levels_integrates_three_sources(monkeypatch):
    sr_facts = [
        {"metadata": {"event_kind": "Support", "price": 100.0, "touch_count": 4}},
        {"metadata": {"event_kind": "Resistance", "price": 130.0, "touch_count": 3}},
    ]
    structural = [
        {"core_name": "trendline_core", "snapshot": {"trendlines": [
            {"status": "Valid", "direction": "Up", "anchor_pivots": [
                {"date": "2026-01-01", "price": 90.0},
                {"date": "2026-04-01", "price": 100.4}]},
            # v4.30 Finding 2:Broken trendlines 改進入(歷史 SR roleflip);要驗
            # skip 的話用非 {Valid, Broken} 的 status(e.g. Pending / Invalid)
            {"status": "Pending", "anchor_pivots": [{"price": 50.0}]},
        ]}},
        {"core_name": "neely_core", "snapshot": {"flat_fib_zones": [
            {"low": 99.0, "high": 101.0, "source_ratio": 0.618}]}},
    ]
    monkeypatch.setattr(kl_mod, "fetch_facts", lambda *a, **k: sr_facts)
    monkeypatch.setattr(kl_mod, "fetch_structural_latest", lambda *a, **k: structural)

    out = key_levels("2330", date(2026, 5, 18), conn=MagicMock())
    assert out["stock_id"] == "2330"
    # Support 100 + Resistance 130 + trendline 100.4 + fib 中點 100 = 4 個來源點
    assert out["source_point_count"] == 4
    assert out["level_count"] == 2  # ~100 cluster + 130 cluster
    near_100 = next(lv for lv in out["levels"] if lv["price"] < 110)
    assert near_100["strength"] == 3
    # neely_core mock 無 timeframe → 走 backward-compat fallback,source = "neely_fib"
    assert set(near_100["sources"]) == {"sr_support", "trendline", "neely_fib"}


def test_key_levels_includes_broken_trendlines_as_historical_sr(monkeypatch):
    """v4.30 Finding 2:Broken trendlines 進入 levels(roleflip 經典 SR)。"""
    structural = [
        {"core_name": "trendline_core", "snapshot": {"trendlines": [
            {"status": "Valid", "anchor_pivots": [{"price": 200.0}]},
            {"status": "Broken", "anchor_pivots": [{"price": 100.0}]},
            {"status": "Broken", "anchor_pivots": [{"price": 100.5}]},
            {"status": "Pending", "anchor_pivots": [{"price": 50.0}]},  # 仍跳過
        ]}},
    ]
    monkeypatch.setattr(kl_mod, "fetch_facts", lambda *a, **k: [])
    monkeypatch.setattr(kl_mod, "fetch_structural_latest", lambda *a, **k: structural)
    out = key_levels("2330", date(2026, 5, 18), conn=MagicMock())
    # 1 Valid + 2 Broken = 3 points;Pending 跳過
    assert out["source_point_count"] == 3
    # 100 + 100.5 cluster(1% 內,distinct sources = trendline_historical)+ 200
    by_price = sorted(out["levels"], key=lambda lv: lv["price"])
    assert by_price[0]["sources"] == ["trendline_historical"]
    assert by_price[0]["member_count"] == 2
    assert by_price[1]["sources"] == ["trendline"]


def test_key_levels_neely_timeframe_distinguishes_source(monkeypatch):
    """v4.30 Finding 4:daily + weekly Fib 同 1% bucket 內 strength 真實升 2。"""
    structural = [
        {"core_name": "neely_core", "timeframe": "daily",
         "snapshot": {"flat_fib_zones": [{"low": 199.0, "high": 201.0}]}},
        {"core_name": "neely_core", "timeframe": "weekly",
         "snapshot": {"flat_fib_zones": [{"low": 199.5, "high": 201.5}]}},
    ]
    monkeypatch.setattr(kl_mod, "fetch_facts", lambda *a, **k: [])
    monkeypatch.setattr(kl_mod, "fetch_structural_latest", lambda *a, **k: structural)
    out = key_levels("2330", date(2026, 5, 18), conn=MagicMock())
    assert out["level_count"] == 1  # ~200 cluster
    assert set(out["levels"][0]["sources"]) == {"neely_fib_daily", "neely_fib_weekly"}
    assert out["levels"][0]["strength"] == 2


def test_key_levels_top_n_caps_to_strongest(monkeypatch):
    """v4.30 Finding 4:top_n=20 cap 排序 by strength * member_count。"""
    # 30 個 fib zones,step 5(> 1% bucket)→ 確保各自獨立 cluster,30 distinct levels
    zones = [{"low": p - 0.5, "high": p + 0.5} for p in range(100, 250, 5)]
    structural = [
        {"core_name": "neely_core", "timeframe": "daily",
         "snapshot": {"flat_fib_zones": zones}},
    ]
    monkeypatch.setattr(kl_mod, "fetch_facts", lambda *a, **k: [])
    monkeypatch.setattr(kl_mod, "fetch_structural_latest", lambda *a, **k: structural)
    out = key_levels("2330", date(2026, 5, 18), conn=MagicMock(), top_n=20)
    assert out["level_count_total"] == 30
    assert out["level_count"] == 20
    assert len(out["levels"]) == 20
    # 取的 20 個照 price 升序回(對齊 API 慣例)
    prices = [lv["price"] for lv in out["levels"]]
    assert prices == sorted(prices)


def test_key_levels_top_n_zero_disables_cap(monkeypatch):
    """top_n=0 → 不 cap,回全部 levels。"""
    zones = [{"low": p - 0.5, "high": p + 0.5} for p in range(100, 250, 5)]
    structural = [
        {"core_name": "neely_core", "timeframe": "daily",
         "snapshot": {"flat_fib_zones": zones}},
    ]
    monkeypatch.setattr(kl_mod, "fetch_facts", lambda *a, **k: [])
    monkeypatch.setattr(kl_mod, "fetch_structural_latest", lambda *a, **k: structural)
    out = key_levels("2330", date(2026, 5, 18), conn=MagicMock(), top_n=0)
    assert out["level_count_total"] == 30
    assert out["level_count"] == 30


def test_key_levels_empty(monkeypatch):
    monkeypatch.setattr(kl_mod, "fetch_facts", lambda *a, **k: [])
    monkeypatch.setattr(kl_mod, "fetch_structural_latest", lambda *a, **k: [])
    out = key_levels("2330", date(2026, 5, 18), conn=MagicMock())
    assert out["level_count"] == 0
    assert out["level_count_total"] == 0
    assert out["levels"] == []
