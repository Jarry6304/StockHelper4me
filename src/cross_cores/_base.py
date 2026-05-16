"""
cross_cores/_base.py
====================
CrossStockBuilder ABC + 共用 base class。

契約:cross-stock builder 輸入 date(or date range),全市場 universe,emit
cross-stock ranked / clustered output。
"""

from __future__ import annotations

from typing import Any, Iterable, Protocol, runtime_checkable


@runtime_checkable
class CrossStockBuilder(Protocol):
    """Cross-stock builder 契約(v3.5 R3 新)。

    與 PerStockBuilder 對映:後者輸入 stock_id;本契約輸入 date / date range
    (全市場 universe)。

    執行流程(由 CrossStockOrchestrator 呼叫):
      1. compute(db, target_date) → 全市場跨股計算
      2. 寫入 `*_derived` 表(Silver-like schema,PK 含 stock_id)
      3. 不走 dirty queue(全市場永遠重算 latest date 即可)
    """

    NAME: str                       # builder 唯一識別,e.g. "magic_formula"
    OUTPUT_TABLE: str               # 目標 derived 表
    UPSTREAM_TABLES: list[str]      # 上游依賴(Bronze + Silver)

    def run(
        self,
        db: Any,                              # DBWriter
        *,
        target_date: Any = None,              # None = latest available date
        full_rebuild: bool = False,           # True = 重算 lookback window 內全部 dates
        lookback_days: int | None = None,     # full_rebuild 時往回幾天
    ) -> dict[str, Any]:
        """Returns:
            {
                "name":         str,
                "rows_read":    int,
                "rows_written": int,
                "elapsed_ms":   int,
                "dates":        int,    # cross_cores 特有:cross-rank 算了幾個 dates
            }
        """
        ...
