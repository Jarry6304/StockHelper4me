"""
bronze/aggregators/
===================
Bronze 層 schema transform + business aggregation(v3.5 R1 C2 拆 package)。

4 個 module:
  - pivot_institutional.py  三大法人 N 列 → 1 寬列 pivot(per-stock + market-level)
  - pack_financial.py       財報 N 科目 → 1 row + detail JSONB pack
  - pack_holding_shares.py  股權分散 N 級距 → 1 row + detail JSONB pack
  - first_per_day.py        intraday → daily 收斂(取每日最早一筆,v3.20 GoldPrice)

入口 `apply_aggregation(strategy, rows, ...)` 依 collector.toml aggregation 欄
分派到對應函式。
"""

from typing import Any

from bronze.aggregators.pivot_institutional import (
    aggregate_institutional,
    aggregate_institutional_market,
)
from bronze.aggregators.pack_financial import aggregate_financial
from bronze.aggregators.pack_holding_shares import aggregate_holding_shares
from bronze.aggregators.first_per_day import aggregate_first_per_day

__all__ = [
    "apply_aggregation",
    "aggregate_institutional",
    "aggregate_institutional_market",
    "aggregate_financial",
    "aggregate_holding_shares",
    "aggregate_first_per_day",
]


def apply_aggregation(
    strategy: str,
    rows: list[dict[str, Any]],
    stmt_type: str | None = None,
    trading_dates: set[str] | None = None,
) -> list[dict[str, Any]]:
    """依 strategy 名稱分派到對應的聚合函式。

    Args:
        strategy:      collector.toml `aggregation` 欄值
        rows:          field_mapper 輸出的原始資料列
        stmt_type:     財報類型(僅 pack_financial 需要)
        trading_dates: 交易日集合(僅 institutional 兩個策略會用到,過濾 FinMind
                       週六回的鬼資料)

    Raises:
        ValueError: 未知的 strategy 名稱
    """
    if strategy == "pivot_institutional":
        return aggregate_institutional(rows, trading_dates=trading_dates)

    if strategy == "pivot_institutional_market":
        return aggregate_institutional_market(rows, trading_dates=trading_dates)

    if strategy == "pack_financial":
        if stmt_type is None:
            raise ValueError("pack_financial 需要 stmt_type 參數")
        return aggregate_financial(rows, stmt_type)

    if strategy == "pack_holding_shares":
        return aggregate_holding_shares(rows)

    if strategy == "first_per_day":
        return aggregate_first_per_day(rows)

    raise ValueError(f"未知的聚合策略:'{strategy}'")
