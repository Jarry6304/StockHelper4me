"""
bronze/segment_runner.py
========================
單一 segment (stock_id, seg_start, seg_end) 的 fetch → transform → aggregate
→ upsert → mark progress 流程。

v3.5 R1 C3:從 phase_executor._run_api() 內 inline `_process_segment` closure
抽出獨立 class,對齊 Layer 1 single-responsibility:
  - PhaseExecutor  = orchestration only(task 蒐集 + asyncio.gather)
  - _SegmentRunner = 單 segment 完整 IO + transform 流程
"""

import asyncio
import logging
from typing import Callable

from bronze.aggregators import apply_aggregation
from api_client import APIError, FinMindClient
from config_loader import ApiConfig
from db import DBWriter
from field_mapper import FieldMapper
from sync_tracker import SyncTracker

logger = logging.getLogger("collector.bronze.segment_runner")


class _SegmentRunner:
    """單 segment 處理流程的物件化封裝。

    Lifecycle:
      - PhaseExecutor._run_api() 為每個 api_config 建立一個 runner
      - 每個 segment 透過 `run(stock_id, seg_start, seg_end)` 處理
      - runner 跟整個 _run_api() 共享同一個 tracker / sem(short-circuit + 並發控制)
    """

    def __init__(
        self,
        api_config: ApiConfig,
        db: DBWriter,
        client: FinMindClient,
        field_mapper: FieldMapper,
        sync_tracker: SyncTracker,
        get_trading_dates: Callable[[], set[str]],
        tracker,                          # _DatasetErrorTracker (from phase_executor)
        sem: asyncio.Semaphore,
        dry_run: bool = False,
    ):
        self.api_config = api_config
        self.db = db
        self.client = client
        self.field_mapper = field_mapper
        self.sync_tracker = sync_tracker
        self._get_trading_dates = get_trading_dates
        self.tracker = tracker
        self.sem = sem
        self.dry_run = dry_run

    async def run(self, stock_id: str, seg_start: str, seg_end: str) -> None:
        """單一 segment 處理(fetch → transform → aggregate → upsert → mark)。

        異常在內部全 catch(asyncio.gather return_exceptions=False 下,任一
        task raise 會 cancel 其餘)。
        """
        api_config = self.api_config

        # short-circuit:abort 後尚未起跑的 task 直接跳過
        if self.tracker.aborted.is_set():
            return

        # 斷點續傳:已完成或空結果直接跳過(走 SyncTracker preload cache)
        if self.sync_tracker.is_completed(api_config.name, stock_id, seg_start):
            logger.debug(
                f"[Phase {api_config.phase}][{api_config.name}] "
                f"Skipped stock={stock_id}, segment={seg_start}~{seg_end} (completed)"
            )
            return

        if self.dry_run:
            logger.info(
                f"[dry-run][{api_config.name}] stock={stock_id}, "
                f"segment={seg_start}~{seg_end}"
            )
            return

        async with self.sem:
            # 進入 critical section 前再 check 一次 abort
            if self.tracker.aborted.is_set():
                return

            raw_records = await self._fetch(stock_id, seg_start, seg_end)
            if raw_records is None:
                return  # error already recorded inside _fetch

            self.tracker.record_success()

            rows = self._transform_and_aggregate(stock_id, seg_start, seg_end, raw_records)
            if rows is None:
                return  # aggregation 失敗已 mark_failed

            if not self._write(stock_id, seg_start, seg_end, rows):
                return  # DB write 失敗已 mark_failed

            # 更新進度
            status = "empty" if not rows else "completed"
            self.sync_tracker.mark_progress(
                api_config.name, stock_id, seg_start, seg_end,
                status=status, record_count=len(rows),
            )

            logger.debug(
                f"[Phase {api_config.phase}][{api_config.name}] "
                f"Done stock={stock_id}, segment={seg_start}~{seg_end}, "
                f"records={len(rows)}"
            )

    async def _fetch(
        self, stock_id: str, seg_start: str, seg_end: str
    ) -> list[dict] | None:
        """FinMind fetch + 錯誤分類處理。回 None 表示已 mark_failed 跳出。"""
        api_config = self.api_config
        try:
            return await self.client.fetch(api_config, stock_id, seg_start, seg_end)
        except APIError as e:
            logger.error(
                f"Max retries exceeded. "
                f"dataset={api_config.dataset}, stock={stock_id}, "
                f"segment={seg_start}~{seg_end}. error={e}"
            )
            self.sync_tracker.mark_failed(
                api_config.name, stock_id, seg_start, seg_end, str(e)
            )
            self.tracker.record_error(e.status_code)
            if self.tracker.aborted.is_set():
                logger.warning(
                    f"[Phase {api_config.phase}][{api_config.name}] "
                    f"連續 {self.tracker.streak} 次 HTTP {e.status_code} 錯誤,"
                    f"視為 dataset-level 拒絕(token quota / tier / dataset 下架)。"
                    f"Abort 此 entry 剩餘 tasks;後續 entries 繼續跑。"
                    f"診斷:`python scripts/probe_finmind_datasets.py` 看 dataset 權限。"
                )
            return None
        except Exception as e:
            logger.error(
                f"非預期錯誤 dataset={api_config.dataset} stock={stock_id} "
                f"segment={seg_start}~{seg_end}: {type(e).__name__}: {e}"
            )
            self.sync_tracker.mark_failed(
                api_config.name, stock_id, seg_start, seg_end, str(e)
            )
            return None

    def _transform_and_aggregate(
        self,
        stock_id: str,
        seg_start: str,
        seg_end: str,
        raw_records: list[dict],
    ) -> list[dict] | None:
        """field_mapper.transform + apply_aggregation。回 None 表 aggregation 失敗已 mark。"""
        api_config = self.api_config

        # 欄位映射(回傳 rows 與 schema_mismatch 旗標)
        rows, schema_mismatch = self.field_mapper.transform(api_config, raw_records)
        if schema_mismatch:
            self.sync_tracker.mark_schema_mismatch(
                api_config.name, stock_id, seg_start, seg_end,
                record_count=len(rows),
            )

        # Phase E:聚合策略(pivot / pack)
        if api_config.aggregation and rows:
            try:
                td = (
                    self._get_trading_dates()
                    if api_config.aggregation in (
                        "pivot_institutional",
                        "pivot_institutional_market",
                    )
                    else None
                )
                rows = apply_aggregation(
                    api_config.aggregation,
                    rows,
                    stmt_type=api_config.stmt_type,
                    trading_dates=td,
                )
            except Exception as e:
                logger.warning(
                    f"[{api_config.name}] aggregation={api_config.aggregation} "
                    f"失敗,跳過此 segment:{e}"
                )
                self.sync_tracker.mark_failed(
                    api_config.name, stock_id, seg_start, seg_end, str(e)
                )
                return None

        return rows

    def _write(
        self,
        stock_id: str,
        seg_start: str,
        seg_end: str,
        rows: list[dict],
    ) -> bool:
        """DB 寫入(含 merge_strategy 特殊處理)。回 False 表寫入失敗已 mark。"""
        api_config = self.api_config
        try:
            if api_config.merge_strategy == "update_delist_date":
                self._merge_delist_date(rows)
            elif rows:
                pks = self.db._table_pks(api_config.target_table)
                self.db.upsert(api_config.target_table, rows, primary_keys=pks)
        except Exception as e:
            logger.error(
                f"[{api_config.name}] DB 寫入失敗 stock={stock_id} "
                f"segment={seg_start}~{seg_end}: {e}"
            )
            self.sync_tracker.mark_failed(
                api_config.name, stock_id, seg_start, seg_end, str(e)
            )
            return False
        return True

    def _merge_delist_date(self, rows: list[dict]) -> None:
        """特殊合併策略:只更新 stock_info_ref 表的 delisting_date 欄位。

        用於 TaiwanStockDelisting 的資料處理。
        v3.2 R-2:stock_info → stock_info_ref;delist_date → delisting_date。
        """
        for row in rows:
            stock_id = row.get("stock_id")
            delisting_date = (
                row.get("date") or row.get("delisting_date") or row.get("delist_date")
            )
            if not stock_id or not delisting_date:
                continue

            self.db.update(
                """
                UPDATE stock_info_ref
                SET delisting_date = %s, updated_at = NOW()
                WHERE market = 'TW' AND stock_id = %s
                """,
                [delisting_date, stock_id],
            )
