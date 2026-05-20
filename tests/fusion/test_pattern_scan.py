"""Fusion Layer · pattern_scan 單元測試(monkeypatch fetch,不依賴真實 PG)。"""

from datetime import date
from unittest.mock import MagicMock

import fusion.pattern_scan as ps_mod
from fusion.pattern_scan import _level_context, pattern_scan


def test_level_context_flags_near_and_far():
    levels = [{"price": 100.0, "strength": 3, "sources": ["sr_support"]},
              {"price": 130.0, "strength": 1, "sources": ["neely_fib"]}]
    near = _level_context(101.0, levels, 0.02)   # 1% 距離 → near
    assert near["near_level"] is True
    assert near["nearest_level_price"] == 100.0
    assert near["level_strength"] == 3
    far = _level_context(115.0, levels, 0.02)    # 距 130 為 ~13% → not near
    assert far["near_level"] is False
    assert _level_context(None, levels, 0.02)["near_level"] is False


def test_pattern_scan_attaches_context(monkeypatch):
    facts = [
        {"fact_date": date(2026, 5, 18),
         "metadata": {"pattern": "Hammer", "trend_context": "Downtrend",
                      "strength_metric": 0.8}},
        {"fact_date": date(2026, 5, 10),
         "metadata": {"event_kind": "Doji", "trend_context": "Sideways",
                      "strength_metric": 0.4}},
    ]
    ohlc = [
        {"date": date(2026, 5, 18), "close": 100.5},
        {"date": date(2026, 5, 10), "close": 118.0},
    ]
    monkeypatch.setattr(ps_mod, "fetch_facts", lambda *a, **k: facts)
    monkeypatch.setattr(ps_mod, "fetch_ohlc", lambda *a, **k: ohlc)
    monkeypatch.setattr(
        ps_mod, "key_levels",
        lambda *a, **k: {"levels": [{"price": 100.0, "strength": 2, "sources": ["sr_support"]}]},
    )
    out = pattern_scan("2330", date(2026, 5, 18), conn=MagicMock())
    assert out["pattern_count"] == 2
    # 依 date 降序 → 5/18 在前
    assert out["patterns"][0]["date"] == "2026-05-18"
    assert out["patterns"][0]["pattern"] == "Hammer"
    assert out["patterns"][0]["price"] == 100.5
    assert out["patterns"][0]["level_context"]["near_level"] is True   # 100.5 vs 100
    # 5/10 的 Doji,price 118 距 100 為 18% → 不貼近
    assert out["patterns"][1]["pattern"] == "Doji"
    assert out["patterns"][1]["level_context"]["near_level"] is False


def test_pattern_scan_missing_ohlc_degrades(monkeypatch):
    facts = [{"fact_date": date(2026, 5, 18), "metadata": {"pattern": "Hammer"}}]
    monkeypatch.setattr(ps_mod, "fetch_facts", lambda *a, **k: facts)
    monkeypatch.setattr(ps_mod, "fetch_ohlc", lambda *a, **k: [])  # 無 OHLC
    monkeypatch.setattr(ps_mod, "key_levels", lambda *a, **k: {"levels": []})
    out = pattern_scan("2330", date(2026, 5, 18), conn=MagicMock())
    assert out["patterns"][0]["price"] is None
    assert out["patterns"][0]["level_context"]["near_level"] is False
