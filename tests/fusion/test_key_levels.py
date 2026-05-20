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
            {"status": "Broken", "anchor_pivots": [{"price": 50.0}]},  # broken → 跳過
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
    assert set(near_100["sources"]) == {"sr_support", "trendline", "neely_fib"}


def test_key_levels_empty(monkeypatch):
    monkeypatch.setattr(kl_mod, "fetch_facts", lambda *a, **k: [])
    monkeypatch.setattr(kl_mod, "fetch_structural_latest", lambda *a, **k: [])
    out = key_levels("2330", date(2026, 5, 18), conn=MagicMock())
    assert out["level_count"] == 0
    assert out["levels"] == []
