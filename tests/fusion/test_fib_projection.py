"""v4.33 — fusion._fib_projection 抽出後行為 + re-import alias 0 regression。"""

from __future__ import annotations

from fusion import _fib_projection as fp


def test_extract_invalidation_bullish_break_below():
    primary = {
        "power_rating": "Bullish",
        "invalidation_triggers": [{"trigger_type": {"PriceBreakBelow": 880.0}}],
    }
    assert fp.extract_invalidation_price(primary, 1000.0) == 880.0


def test_extract_invalidation_bearish_break_above():
    primary = {
        "power_rating": "Bearish",
        "invalidation_triggers": [{"trigger_type": {"PriceBreakAbove": 1100.0}}],
    }
    assert fp.extract_invalidation_price(primary, 1000.0) == 1100.0


def test_extract_invalidation_none_when_no_primary():
    assert fp.extract_invalidation_price(None, 1000.0) is None


def test_find_closest_zone_by_source_ratio():
    zones = [
        {"low": 1, "high": 2, "source_ratio": 0.382},
        {"low": 3, "high": 4, "source_ratio": 0.618},
        {"low": 5, "high": 6, "source_ratio": 1.0},
    ]
    z = fp.find_closest_zone(zones, 0.6)
    assert z["source_ratio"] == 0.618


def test_project_range_uses_fib_zones():
    zones = [{"low": 1180, "high": 1220, "source_ratio": 0.618}]
    rl, rh = fp.project_range(zones, 0.618, 1.0, 1000.0, 1)
    assert rl == [1180, 1220]
    assert rh == [1180, 1220]  # 唯一 zone,最接近 0.618 與 1.0 都是它


def test_project_range_empty_fib_fallback_no_raise():
    rl, rh = fp.project_range([], 0.382, 0.618, 1000.0, 1)
    assert rl is not None and rh is not None
    # bullish fallback:upside range_high 高於 current
    assert max(rh) > 1000.0


def test_project_range_bearish_swaps():
    zones = [
        {"low": 800, "high": 820, "source_ratio": 0.382},
        {"low": 1180, "high": 1220, "source_ratio": 0.618},
    ]
    rl, rh = fp.project_range(zones, 0.382, 0.618, 1000.0, -1)
    # bearish → range_low / range_high swap
    assert rl == [1180, 1220]
    assert rh == [800, 820]


def test_timeframe_fib_range_constant():
    assert fp.TIMEFRAME_FIB_RANGE["1m"] == (0.382, 0.618)
    assert fp.TIMEFRAME_FIB_RANGE["6m"] == (1.000, 1.382)


def test_public_facade_reexports_are_shared_objects():
    """fusion 公開出口(PEP 562 轉發)與 _fib_projection 須是同一物件
    (single source;v4.39 _forecast.py 退役後守衛移到公開面)。"""
    import fusion

    assert fusion.project_range is fp.project_range
    assert fusion.find_closest_zone is fp.find_closest_zone
    assert fusion.extract_invalidation_price is fp.extract_invalidation_price
    assert fusion.TIMEFRAME_FIB_RANGE is fp.TIMEFRAME_FIB_RANGE
