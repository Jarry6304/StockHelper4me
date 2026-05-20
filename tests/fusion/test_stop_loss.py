"""Fusion Layer · stop_loss 單元測試(monkeypatch fetch,不依賴真實 PG)。"""

from datetime import date
from unittest.mock import MagicMock

import fusion.stop_loss as sl_mod
from fusion.stop_loss import stop_loss


def test_stop_loss_long(monkeypatch):
    monkeypatch.setattr(
        sl_mod, "fetch_indicator_latest",
        lambda *a, **k: [{"value": {"series": [
            {"date": "2026-05-18", "atr": 5.0, "atr_pct": 2.0}]}}],
    )
    monkeypatch.setattr(
        sl_mod, "key_levels",
        lambda *a, **k: {"levels": [{"price": 95.0}, {"price": 115.0}]},
    )
    out = stop_loss("2330", 100.0, date(2026, 5, 18), conn=MagicMock())
    assert out["direction"] == "long"
    assert out["atr"] == 5.0
    assert out["stops"]["atr_based"]["price"] == 90.0      # 100 - 2*5
    assert out["stops"]["atr_based"]["distance"] == 10.0
    assert out["stops"]["nearest_level"]["price"] == 95.0   # 最近的支撐(< 100)
    assert out["targets"]["atr_based"]["price"] == 120.0    # 100 + 2*2*5
    assert out["targets"]["nearest_level"]["price"] == 115.0  # 最近的壓力(> 100)


def test_stop_loss_short_mirrors(monkeypatch):
    monkeypatch.setattr(
        sl_mod, "fetch_indicator_latest",
        lambda *a, **k: [{"value": {"series": [{"atr": 4.0}]}}],
    )
    monkeypatch.setattr(
        sl_mod, "key_levels",
        lambda *a, **k: {"levels": [{"price": 95.0}, {"price": 115.0}]},
    )
    out = stop_loss("2330", 100.0, date(2026, 5, 18), direction="short", conn=MagicMock())
    assert out["direction"] == "short"
    assert out["stops"]["atr_based"]["price"] == 108.0       # 100 + 2*4(止損在上)
    assert out["stops"]["nearest_level"]["price"] == 115.0    # short 止損參考上方
    assert out["targets"]["atr_based"]["price"] == 84.0       # 100 - 2*2*4(止盈在下)
    assert out["targets"]["nearest_level"]["price"] == 95.0


def test_stop_loss_no_atr_degrades(monkeypatch):
    monkeypatch.setattr(sl_mod, "fetch_indicator_latest", lambda *a, **k: [])
    monkeypatch.setattr(sl_mod, "key_levels", lambda *a, **k: {"levels": [{"price": 95.0}]})
    out = stop_loss("2330", 100.0, date(2026, 5, 18), conn=MagicMock())
    assert out["atr"] is None
    assert out["stops"]["atr_based"] is None
    assert out["stops"]["nearest_level"]["price"] == 95.0  # level-based 仍可用
