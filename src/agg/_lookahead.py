"""Look-ahead bias 防衛 — 過濾 as_of 之後才公布的 facts。

對齊 m3Spec/aggregation_layer.md §六 Look-ahead bias 防衛。

核心原則:
- daily facts:直接看 fact_date
- monthly facts(revenue, business_indicator):metadata.report_date 後生效
- quarterly facts(financial_statement):fact_date + 45 天 fallback(無 report_date)
"""

from __future__ import annotations

from datetime import date, timedelta
from typing import Any


FINANCIAL_STATEMENT_LAG_DAYS = 45


def is_visible_at(fact: dict[str, Any], as_of: date) -> bool:
    """判斷一個 fact 是否在 as_of 那天可見。

    Args:
        fact: facts 表 row dict(必有 fact_date, source_core;可能有 metadata.report_date)
        as_of: 查詢日

    Returns:
        True 若 fact 在 as_of 當天或之前已可見
    """
    fact_date = _coerce_date(fact["fact_date"])
    if fact_date > as_of:
        return False

    metadata = fact.get("metadata", {}) or {}

    # monthly facts(revenue / business_indicator)— 看 report_date
    report_date_str = metadata.get("report_date")
    if report_date_str:
        report_date = _coerce_date(report_date_str)
        return report_date <= as_of

    # quarterly financial_statement:T+45 fallback(spec 無 report_date 欄)
    if fact.get("source_core") == "financial_statement_core":
        publish = fact_date + timedelta(days=FINANCIAL_STATEMENT_LAG_DAYS)
        return publish <= as_of

    # daily fact:直接 fact_date 過濾(上面已過)
    return True


def _coerce_date(d: Any) -> date:
    """支援 date / datetime / ISO string 三種輸入。"""
    if isinstance(d, date):
        return d
    if hasattr(d, "date"):  # datetime
        return d.date()
    if isinstance(d, str):
        return date.fromisoformat(d[:10])
    raise TypeError(f"無法 coerce 成 date: {type(d).__name__}={d!r}")


def filter_visible(facts: list[dict[str, Any]], as_of: date) -> list[dict[str, Any]]:
    """批次過濾 facts list,只保留 as_of 當天可見的。"""
    return [f for f in facts if is_visible_at(f, as_of)]
