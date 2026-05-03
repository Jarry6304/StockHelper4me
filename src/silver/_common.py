"""
silver/_common.py
=================
Silver builder 共用工具(blueprint §三 builder 入口/出口契約 + helper)。

PR #19a 落地:filter_to_trading_days + SilverBuilder protocol。
PR #19b 補齊 4 個共用 helper(本檔):
  - get_trading_dates(db) — 一次讀 trading_date_ref 給 institutional builder 用
  - fetch_bronze(db, table, stock_ids, where) — 統一 SELECT bronze 模式
  - upsert_silver(db, table, rows, pk_cols) — UPSERT 包 is_dirty=FALSE / dirty_at=NULL
  - reset_dirty(db, table, pks) — 顯式 reset(若 row 已存在 + 被 trigger 標 dirty)

PR #19c 會在這裡再加:
  - select_dirty_pks(db, silver_table, stock_ids) — pull dirty queue 的 SQL
  - 若 builder 失敗,is_dirty 不 reset(對齊 cores_overview §7.5 dirty 契約)
"""

from __future__ import annotations

import logging
from datetime import date, datetime
from decimal import Decimal
from typing import Any, Iterable, Protocol, runtime_checkable

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

    NAME: str                # builder 唯一識別,e.g. "institutional"
    SILVER_TABLE: str        # 目標 Silver 表
    BRONZE_TABLES: list[str] # 來源 Bronze 表(可多張)

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
                "elapsed_ms": int,
            }
        """
        ...


# =============================================================================
# Trading-day 過濾(從 src/aggregators.py 原搬,sentinel 行為一致)
# =============================================================================

def filter_to_trading_days(
    rows: list[dict[str, Any]],
    trading_dates: set,
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


# =============================================================================
# PR #19b 新 helper
# =============================================================================

def get_trading_dates(db: Any) -> set[date]:
    """從 trading_date_ref 一次讀全部 date(回傳 datetime.date 物件 set)。

    給 institutional builder 用(過濾 FinMind 週六鬼資料)。
    若 trading_date_ref 還沒填,回空 set,呼叫端的 filter 會走 safety bypass。

    Note:psycopg 對 PG DATE 欄位回 datetime.date 物件,Bronze 讀進來的 date 也是
    date 物件;set 保持 date 型別,filter 用 `d in trading_dates` 自然比對。
    """
    rows = db.query("SELECT date FROM trading_date_ref")
    return {r["date"] for r in rows} if rows else set()


def fetch_bronze(
    db: Any,
    table: str,
    *,
    stock_ids: list[str] | None = None,
    where: str | None = None,
    params: list[Any] | None = None,
) -> list[dict[str, Any]]:
    """統一 SELECT Bronze 模式。stock_ids 與 where 兩條過濾路徑可合用(AND)。

    回傳 dict list(psycopg dict_row)。caller 自行 transform 進 Silver shape。
    """
    sql = f"SELECT * FROM {table}"
    where_parts = []
    all_params: list[Any] = list(params) if params else []
    if stock_ids:
        placeholders = ",".join(["%s"] * len(stock_ids))
        where_parts.append(f"stock_id IN ({placeholders})")
        all_params.extend(stock_ids)
    if where:
        where_parts.append(f"({where})")
    if where_parts:
        sql += " WHERE " + " AND ".join(where_parts)
    sql += " ORDER BY market, stock_id, date"
    return db.query(sql, all_params if all_params else None)


def upsert_silver(
    db: Any,
    table: str,
    rows: list[dict[str, Any]],
    pk_cols: list[str],
    *,
    batch_size: int = 1000,
    reset_dirty_on_write: bool = True,
) -> int:
    """批次 UPSERT 到 Silver 表。回傳寫入列數。

    reset_dirty_on_write=True(預設):每筆寫入時自動 set is_dirty=FALSE / dirty_at=NULL,
    對應 builder 完成後重置 dirty queue 的契約(blueprint §三)。
    若 row dict 沒明確帶 is_dirty / dirty_at,在這裡補上(讓 caller 不用每筆塞)。

    對 dict-valued 欄(典型 = `detail` JSONB)自動把內容轉 JSON-safe 型別
    (Decimal → float / date → ISO str),解 psycopg 對 NUMERIC / DATE 回的
    Decimal / date 物件 json.dumps 不認的問題。
    """
    if not rows:
        return 0

    if reset_dirty_on_write:
        for r in rows:
            r.setdefault("is_dirty", False)
            r.setdefault("dirty_at", None)

    # Walk dict-valued columns,把 Decimal / date 等轉 JSON-safe 型別
    for r in rows:
        for k, v in list(r.items()):
            if isinstance(v, dict):
                r[k] = _to_jsonb_safe(v)

    total = 0
    for i in range(0, len(rows), batch_size):
        batch = rows[i:i + batch_size]
        total += db.upsert(table, batch, pk_cols)
    return total


def _to_jsonb_safe(value: Any) -> Any:
    """遞迴把 dict / list / scalar 轉成 JSON-safe 型別。

    給 psycopg 自動 JSONB 序列化用 — Decimal / date / datetime 不在 Python
    json stdlib 預設處理範圍,需先轉 float / ISO 字串。
    """
    if isinstance(value, dict):
        return {k: _to_jsonb_safe(v) for k, v in value.items()}
    if isinstance(value, list):
        return [_to_jsonb_safe(v) for v in value]
    if isinstance(value, Decimal):
        return float(value)
    if isinstance(value, (date, datetime)):
        return value.isoformat()
    return value


def reset_dirty(
    db: Any,
    table: str,
    pks: list[dict[str, Any]],
    pk_cols: Iterable[str],
) -> int:
    """顯式 reset 一批 PK 的 is_dirty / dirty_at(builder 完成後呼叫)。

    用於 row 已 UPSERT 過(state 還在)但 dirty 標記是 trigger 後加的 case。
    upsert_silver 預設 reset_dirty_on_write=True 已涵蓋大多數情境;
    本 helper 保留給特殊路徑(如 builder 只 UPDATE 部分欄)用。
    """
    if not pks:
        return 0
    pk_list = list(pk_cols)
    where_clause = " AND ".join(f"{c} = %s" for c in pk_list)
    sql = (
        f"UPDATE {table} SET is_dirty = FALSE, dirty_at = NULL "
        f"WHERE {where_clause}"
    )
    affected = 0
    for pk_row in pks:
        params = [pk_row[c] for c in pk_list]
        affected += db.update(sql, params)
    return affected
