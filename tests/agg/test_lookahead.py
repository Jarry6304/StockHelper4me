"""Look-ahead bias 防衛 tests。"""

from datetime import date, timedelta

import pytest

from agg._lookahead import (
    FINANCIAL_STATEMENT_LAG_DAYS,
    filter_visible,
    is_visible_at,
)


def _fact(*, fact_date, source_core="macd_core", metadata=None, **extra):
    return {
        "fact_date": fact_date,
        "source_core": source_core,
        "metadata": metadata or {},
        **extra,
    }


class TestDailyFacts:
    def test_past_fact_visible(self):
        f = _fact(fact_date=date(2026, 1, 1))
        assert is_visible_at(f, date(2026, 1, 5))

    def test_future_fact_invisible(self):
        f = _fact(fact_date=date(2026, 1, 5))
        assert not is_visible_at(f, date(2026, 1, 1))

    def test_same_day_visible(self):
        d = date(2026, 1, 1)
        assert is_visible_at(_fact(fact_date=d), d)

    def test_iso_string_date(self):
        f = _fact(fact_date="2026-01-01")
        assert is_visible_at(f, date(2026, 1, 5))


class TestMonthlyFactsReportDate:
    def test_revenue_before_report_date_invisible(self):
        # 3 月營收 fact_date=3/31, report_date=4/10
        f = _fact(
            fact_date=date(2026, 3, 31),
            source_core="revenue_core",
            metadata={"report_date": "2026-04-10"},
        )
        assert not is_visible_at(f, date(2026, 4, 5))

    def test_revenue_after_report_date_visible(self):
        f = _fact(
            fact_date=date(2026, 3, 31),
            source_core="revenue_core",
            metadata={"report_date": "2026-04-10"},
        )
        assert is_visible_at(f, date(2026, 4, 15))

    def test_business_indicator_before_release_invisible(self):
        f = _fact(
            fact_date=date(2026, 1, 31),
            source_core="business_indicator_core",
            metadata={"report_date": "2026-02-27"},
        )
        assert not is_visible_at(f, date(2026, 2, 20))


class TestQuarterlyFinancialStatement:
    def test_financial_before_t45_invisible(self):
        # Q1 ends 3/31, T+45 = 5/15
        f = _fact(
            fact_date=date(2026, 3, 31),
            source_core="financial_statement_core",
            metadata={},
        )
        assert not is_visible_at(f, date(2026, 5, 10))

    def test_financial_after_t45_visible(self):
        f = _fact(
            fact_date=date(2026, 3, 31),
            source_core="financial_statement_core",
            metadata={},
        )
        # 3/31 + 45 = 5/15
        assert is_visible_at(f, date(2026, 5, 15))
        assert is_visible_at(f, date(2026, 5, 20))


class TestFilterBatch:
    def test_mixed_facts_filtered_correctly(self):
        as_of = date(2026, 5, 1)
        facts = [
            _fact(fact_date=date(2026, 4, 1)),                                     # 可見
            _fact(fact_date=date(2026, 5, 5)),                                     # 未來
            _fact(                                                                 # report_date 未到
                fact_date=date(2026, 4, 30),
                source_core="revenue_core",
                metadata={"report_date": "2026-05-10"},
            ),
            _fact(                                                                 # T+45 未到
                fact_date=date(2026, 3, 31),
                source_core="financial_statement_core",
            ),
        ]
        visible = filter_visible(facts, as_of)
        assert len(visible) == 1
        assert visible[0]["fact_date"] == date(2026, 4, 1)
