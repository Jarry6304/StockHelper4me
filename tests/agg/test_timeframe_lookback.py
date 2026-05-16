"""Per-timeframe lookback fold-forward 測試(對齊 spec §4.2)。

`_filter_by_timeframe_lookback` 是 pure function,可獨立驗:
  - daily / weekly / unset timeframe → 套 daily lookback
  - monthly timeframe → 套 monthly lookback
  - quarterly timeframe → 套 quarterly lookback
  - fact_date None → 保留(安全 default)
"""

from __future__ import annotations

from datetime import date

from agg.query import _filter_by_timeframe_lookback


def _row(fact_date, timeframe="daily", **extra):
    base = {"fact_date": fact_date, "timeframe": timeframe}
    base.update(extra)
    return base


class TestFilterByTimeframeLookback:
    def test_daily_cutoff(self):
        as_of = date(2026, 5, 1)
        rows = [
            _row(date(2026, 4, 1), "daily"),  # 30 days ago → within 90
            _row(date(2026, 1, 1), "daily"),  # 120 days ago → outside 90
        ]
        result = _filter_by_timeframe_lookback(
            rows,
            as_of,
            lookback_daily=90,
            lookback_monthly=90,
            lookback_quarterly=180,
        )
        assert len(result) == 1
        assert result[0]["fact_date"] == date(2026, 4, 1)

    def test_monthly_uses_monthly_cutoff(self):
        as_of = date(2026, 5, 1)
        # monthly fact 60 days ago — daily=90 接受、monthly=30 排除
        rows = [_row(date(2026, 3, 1), "monthly")]
        result = _filter_by_timeframe_lookback(
            rows,
            as_of,
            lookback_daily=90,
            lookback_monthly=30,
            lookback_quarterly=180,
        )
        assert result == []  # monthly cutoff 排除

        result2 = _filter_by_timeframe_lookback(
            rows,
            as_of,
            lookback_daily=90,
            lookback_monthly=90,
            lookback_quarterly=180,
        )
        assert len(result2) == 1  # monthly 90 包含

    def test_quarterly_uses_quarterly_cutoff(self):
        as_of = date(2026, 5, 1)
        # quarterly fact 150 days ago — daily=90 排除、quarterly=180 接受
        rows = [_row(date(2025, 12, 2), "quarterly")]
        result = _filter_by_timeframe_lookback(
            rows,
            as_of,
            lookback_daily=90,
            lookback_monthly=90,
            lookback_quarterly=180,
        )
        assert len(result) == 1  # quarterly 180 包含

        result2 = _filter_by_timeframe_lookback(
            rows,
            as_of,
            lookback_daily=90,
            lookback_monthly=90,
            lookback_quarterly=60,
        )
        assert result2 == []  # quarterly 60 排除

    def test_unknown_timeframe_treated_as_daily(self):
        as_of = date(2026, 5, 1)
        # timeframe 不存在 → 視 daily
        rows = [_row(date(2026, 1, 1), None)]
        result = _filter_by_timeframe_lookback(
            rows,
            as_of,
            lookback_daily=90,
            lookback_monthly=180,
            lookback_quarterly=180,
        )
        assert result == []  # 120 天 > daily 90,被排除

    def test_mixed_timeframes(self):
        """同時混合 daily / monthly / quarterly 驗證 per-row cutoff。"""
        as_of = date(2026, 5, 1)
        rows = [
            _row(date(2026, 4, 1), "daily"),  # 30d 內,daily 90 接受
            _row(date(2026, 3, 1), "monthly"),  # 60d,monthly 90 接受
            _row(date(2026, 1, 1), "quarterly"),  # 120d,quarterly 180 接受
            _row(date(2025, 10, 1), "daily"),  # 210d,daily 90 排除
            _row(date(2025, 10, 1), "monthly"),  # 210d,monthly 90 排除
            _row(date(2025, 10, 1), "quarterly"),  # 210d,quarterly 180 排除
        ]
        result = _filter_by_timeframe_lookback(
            rows,
            as_of,
            lookback_daily=90,
            lookback_monthly=90,
            lookback_quarterly=180,
        )
        kept_dates = {(r["fact_date"], r["timeframe"]) for r in result}
        assert kept_dates == {
            (date(2026, 4, 1), "daily"),
            (date(2026, 3, 1), "monthly"),
            (date(2026, 1, 1), "quarterly"),
        }

    def test_fact_date_none_preserved(self):
        """fact_date 為 None 的 row 保留(安全 default,避免誤殺 schema 缺漏)。"""
        as_of = date(2026, 5, 1)
        rows = [_row(None, "daily")]
        result = _filter_by_timeframe_lookback(
            rows,
            as_of,
            lookback_daily=90,
            lookback_monthly=90,
            lookback_quarterly=180,
        )
        assert len(result) == 1

    def test_uppercase_timeframe_normalized(self):
        """timeframe 大小寫不敏感(對齊 .lower())。"""
        as_of = date(2026, 5, 1)
        rows = [_row(date(2025, 12, 2), "QUARTERLY")]
        result = _filter_by_timeframe_lookback(
            rows,
            as_of,
            lookback_daily=90,
            lookback_monthly=90,
            lookback_quarterly=180,
        )
        assert len(result) == 1  # 視同 quarterly
