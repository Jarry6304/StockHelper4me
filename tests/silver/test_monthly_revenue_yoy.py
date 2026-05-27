"""Tests for silver/builders/monthly_revenue.py — v4.28+ Bug A fix。

驗證 `revenue_yoy` / `revenue_mom` 從 raw `revenue` 跨月計算,**不再 rename
FinMind `revenue_year` / `revenue_month` 欄**(後者是 calendar year/month,
非 YoY%/MoM%)。

對齊 CLAUDE.md v4.28+ 段:
- Bronze `revenue_year` / `revenue_month` = calendar year/month(e.g. 2026.0 / 4.0)
- Silver `revenue_yoy` / `revenue_mom` = percentage from raw revenue cross-month
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

from silver.builders.monthly_revenue import (  # noqa: E402
    _build_period_index,
    _build_silver_rows,
    _compute_yoy_mom,
)


# ───────────────────────────────────────────────────────────────────────────────
# _compute_yoy_mom unit tests


def _bronze_row(*, market="TW", stock_id="2330", y, m, rev, day=1):
    """Build a Bronze monthly_revenue row(對齊真實 schema)。"""
    return {
        "market": market,
        "stock_id": stock_id,
        "date": date(int(y), int(m), day),
        "revenue": rev,
        "revenue_year": float(y),
        "revenue_month": float(m),
        "country": "Taiwan",
        "create_time": "",
    }


def test_yoy_computed_from_revenue_when_prior_year_exists():
    """2026-04 rev=100 / 2025-04 rev=80 → YoY = (100-80)/80*100 = 25.0"""
    rows = [
        _bronze_row(y=2025, m=4, rev=80),
        _bronze_row(y=2026, m=4, rev=100),
    ]
    index = _build_period_index(rows)
    yoy, _ = _compute_yoy_mom(_bronze_row(y=2026, m=4, rev=100), index)
    assert yoy == 25.0


def test_yoy_none_when_no_prior_year_base():
    """只有 2026-04,無 2025-04 → YoY=None"""
    rows = [_bronze_row(y=2026, m=4, rev=100)]
    index = _build_period_index(rows)
    yoy, _ = _compute_yoy_mom(_bronze_row(y=2026, m=4, rev=100), index)
    assert yoy is None


def test_mom_computed_from_revenue():
    """2026-04 rev=100 / 2026-03 rev=90 → MoM = (100-90)/90*100 ≈ 11.11"""
    rows = [
        _bronze_row(y=2026, m=3, rev=90),
        _bronze_row(y=2026, m=4, rev=100),
    ]
    index = _build_period_index(rows)
    _, mom = _compute_yoy_mom(_bronze_row(y=2026, m=4, rev=100), index)
    assert mom is not None
    assert abs(mom - 11.1111) < 0.001


def test_mom_january_uses_december_prior_year():
    """2026-01 rev=100 / 2025-12 rev=80 → MoM = 25.0(wrap-around)"""
    rows = [
        _bronze_row(y=2025, m=12, rev=80),
        _bronze_row(y=2026, m=1, rev=100),
    ]
    index = _build_period_index(rows)
    _, mom = _compute_yoy_mom(_bronze_row(y=2026, m=1, rev=100), index)
    assert mom == 25.0


def test_yoy_skips_zero_or_negative_base():
    """2025-04 rev=0 → 不算 base,YoY=None(避免除零)"""
    rows = [
        _bronze_row(y=2025, m=4, rev=0),
        _bronze_row(y=2026, m=4, rev=100),
    ]
    index = _build_period_index(rows)
    yoy, _ = _compute_yoy_mom(_bronze_row(y=2026, m=4, rev=100), index)
    assert yoy is None


def test_yoy_negative_when_revenue_dropped():
    """2026-04 rev=60 / 2025-04 rev=80 → YoY = -25.0(負 YoY 真實存在)"""
    rows = [
        _bronze_row(y=2025, m=4, rev=80),
        _bronze_row(y=2026, m=4, rev=60),
    ]
    index = _build_period_index(rows)
    yoy, _ = _compute_yoy_mom(_bronze_row(y=2026, m=4, rev=60), index)
    assert yoy == -25.0


def test_per_stock_isolation():
    """不同 stock 的 revenue 不應交叉計算。"""
    rows = [
        _bronze_row(stock_id="2330", y=2025, m=4, rev=80),
        _bronze_row(stock_id="2317", y=2025, m=4, rev=50),
        _bronze_row(stock_id="2330", y=2026, m=4, rev=100),
        _bronze_row(stock_id="2317", y=2026, m=4, rev=70),
    ]
    index = _build_period_index(rows)
    # 2330 YoY = (100-80)/80 = 25.0
    yoy_2330, _ = _compute_yoy_mom(
        _bronze_row(stock_id="2330", y=2026, m=4, rev=100), index)
    assert yoy_2330 == 25.0
    # 2317 YoY = (70-50)/50 = 40.0
    yoy_2317, _ = _compute_yoy_mom(
        _bronze_row(stock_id="2317", y=2026, m=4, rev=70), index)
    assert yoy_2317 == 40.0


# ───────────────────────────────────────────────────────────────────────────────
# Integration:_build_silver_rows


def test_full_build_silver_rows_yoy_mom_populated():
    """End-to-end:Bronze rows 一年 +1 月,Silver 應正確算出 YoY + MoM。"""
    rows = [
        _bronze_row(y=2025, m=3, rev=70),
        _bronze_row(y=2025, m=4, rev=80),
        _bronze_row(y=2026, m=3, rev=90),
        _bronze_row(y=2026, m=4, rev=100),
    ]
    silver = _build_silver_rows(rows)
    assert len(silver) == 4
    # find 2026-04 silver row
    s = next(r for r in silver if r["date"] == date(2026, 4, 1))
    assert s["revenue"] == 100
    assert s["revenue_yoy"] == 25.0     # 100 vs 80
    assert abs(s["revenue_mom"] - 11.1111) < 0.001  # 100 vs 90
    assert s["detail"]["country"] == "Taiwan"
    # 2025-03(最早 row,無 base 任一方向)→ 都 None
    s_earliest = next(r for r in silver if r["date"] == date(2025, 3, 1))
    assert s_earliest["revenue_yoy"] is None
    assert s_earliest["revenue_mom"] is None


def test_revenue_yoy_never_stores_calendar_year_value():
    """Regression-lock:確保不再有 2026.0 / 2025.0 等 calendar year 殘留在
    Silver revenue_yoy 欄(對齊 v4.28+ Bug A fix)。"""
    rows = [_bronze_row(y=2025, m=4, rev=80), _bronze_row(y=2026, m=4, rev=100)]
    silver = _build_silver_rows(rows)
    for s in silver:
        if s["revenue_yoy"] is not None:
            # YoY 範圍合理性:單月 YoY 絕對值通常 < 500%(極端景氣股 < 1000%)
            # 永不應 = calendar year(2000~2100)
            assert abs(s["revenue_yoy"]) < 1000, (
                f"revenue_yoy={s['revenue_yoy']} looks like calendar year residue;"
                f" expected percentage |yoy| < 1000"
            )


def test_invalid_revenue_skipped_in_index():
    """Bronze rev=None / 非數值 → index 不收(避免下游 base lookup 對到 garbage)。"""
    rows = [
        _bronze_row(y=2025, m=4, rev=None),
        _bronze_row(y=2026, m=4, rev=100),
    ]
    index = _build_period_index(rows)
    # (2025, 4) 不在 index;(2026, 4) 在
    assert ("TW", "2330", 2025, 4) not in index
    assert ("TW", "2330", 2026, 4) in index
    yoy, _ = _compute_yoy_mom(_bronze_row(y=2026, m=4, rev=100), index)
    assert yoy is None    # 因為 2025-04 不在 index
