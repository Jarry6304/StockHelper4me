"""
silver/_common.py
=================
Silver builder 共用工具。

PR #19a 落地:
  - filter_to_trading_days(rows, trading_dates, label) — 從 src/aggregators.py 搬來,
    給 institutional / institutional_market builder 用,過濾 FinMind 週六回的鬼資料
  - SilverBuilder protocol — builder 共通介面契約

PR #19b 起會在這裡加:
  - get_trading_dates(db) — 一次讀 trading_date_ref 給 builder 用
  - select_dirty_pks(db, table) — pull dirty queue 的共通 SQL
  - reset_dirty(db, table, pks) — 同 transaction reset is_dirty/dirty_at
"""

from __future__ import annotations

import logging
from typing import Any, Protocol, runtime_checkable

logger = logging.getLogger("collector.silver._common")


# =============================================================================
# Builder protocol(每個 silver/builders/*.py 必須符合這個介面)
# =============================================================================

@runtime_checkable
class SilverBuilder(Protocol):
    """
    Silver builder 共通介面契約(blueprint §三 Silver builder 入口/出口契約)。

    執行流程(由 orchestrator 呼叫):
      1. select_dirty_pks() → 取得 (market, stock_id, date_range) 清單
      2. 從對應 Bronze SELECT raw + 必要 ref 表 join
      3. 計算 derived 欄位(per spec)
      4. UPSERT 到 *_derived(同 transaction reset is_dirty/dirty_at)
    """

    name: str                # builder 唯一識別,e.g. "institutional"
    silver_table: str        # 目標 Silver 表
    bronze_tables: list[str] # 來源 Bronze 表(可多張)

    def run(
        self,
        db: Any,                                # DBWriter
        stock_ids: list[str] | None = None,    # None = 全市場
        full_rebuild: bool = False,            # True = 忽略 dirty 全重算
    ) -> dict[str, Any]:
        """
        Returns:
            {
                "name": str,
                "rows_read": int,
                "rows_written": int,
                "rows_dirty_reset": int,
                "elapsed_ms": int,
            }
        """
        ...


# =============================================================================
# Trading-day 過濾(從 src/aggregators.py 原搬,sentinel 行為一致)
# =============================================================================

def filter_to_trading_days(
    rows: list[dict[str, Any]],
    trading_dates: set[str],
    label: str,
) -> list[dict[str, Any]]:
    """過濾掉 date 不在 trading_dates 集合內的 rows,並記錄被丟掉的日期。

    安全閥:trading_dates 為空(trading_date_ref 還沒灌資料)時不過濾,
    避免把整批資料都當鬼資料丟掉。

    Note:
      PR #19c aggregators.py 全砍時,src/aggregators.py:_filter_to_trading_days
      原檔同步 deprecate;在那之前兩邊並存,行為一致。
    """
    if not trading_dates:
        logger.warning(
            f"[{label}] trading_dates 為空(trading_date_ref 表未填充?)"
            f",跳過非交易日過濾"
        )
        return rows

    kept: list[dict[str, Any]] = []
    dropped_dates: set[str] = set()
    for row in rows:
        d = row.get("date")
        if d is None or d in trading_dates:
            kept.append(row)
        else:
            dropped_dates.add(d)
    if dropped_dates:
        logger.warning(
            f"[{label}] FinMind 回了 {len(dropped_dates)} 個非交易日的資料,"
            f"已過濾:{sorted(dropped_dates)}"
        )
    return kept
