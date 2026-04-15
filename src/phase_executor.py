"""
phase_executor.py
------------------
Phase 排程引擎模組。

負責依 Phase 1→6 順序執行各 API 蒐集任務：
- Phase 1：META（全市場）
- Phase 2：EVENTS（每支股票）
- Phase 3：RAW PRICE（每支股票 × 日期分段）
- Phase 4：RUST 計算（呼叫 rust_bridge）
- Phase 5：CHIP / FUNDAMENTAL
- Phase 6：MACRO

Phase 1 完成後需重新解析股票清單（先雞後蛋問題）。
"""

import logging
from typing import Callable

from aggregators import apply_aggregation
from api_client import ALL_MARKET_SENTINEL, APIError, FinMindClient
from config_loader import ApiConfig, CollectorConfig, StockListConfig
from date_segmenter import DateSegmenter
from db import DBWriter
from field_mapper import FieldMapper
import stock_resolver
from sync_tracker import SyncTracker

logger = logging.getLogger("collector.phase_executor")


class PhaseExecutor:
    """
    Phase 排程引擎。

    使用方式：
        executor = PhaseExecutor(config, stock_list_cfg, db, client, ...)
        await executor.run("backfill")
    """

    def __init__(
        self,
        config: CollectorConfig,
        stock_list_cfg: StockListConfig,
        db: DBWriter,
        client: FinMindClient,
        sync_tracker: SyncTracker,
        rust_runner: Callable | None = None,
        dry_run: bool = False,
    ):
        """
        Args:
            config:          整合後的 Collector 設定
            stock_list_cfg:  stock_list.toml 設定
            db:              SQLite 連線
            client:          FinMind HTTP Client
            sync_tracker:    斷點續傳追蹤器
            rust_runner:     Phase 4 呼叫函式（傳入 None 則跳過 Phase 4）
            dry_run:         True 時只印計劃，不實際呼叫 API
        """
        self.config          = config
        self.stock_list_cfg  = stock_list_cfg
        self.db              = db
        self.client          = client
        self.sync_tracker    = sync_tracker
        self.rust_runner     = rust_runner
        self.dry_run         = dry_run
        self.field_mapper    = FieldMapper()
        self.date_segmenter  = DateSegmenter(config)

        # 初始股票清單（Phase 1 前可能為空，Phase 1 後刷新）
        self._stock_list: list[str] = []

    # =========================================================================
    # 主要執行入口
    # =========================================================================

    async def run(self, mode: str) -> None:
        """
        依照設定的 mode（backfill / incremental）與 phases 清單執行。

        Args:
            mode: "backfill" | "incremental"
        """
        phases_to_run = sorted(self.config.execution.phases)
        logger.info(f"Collector started. command={mode}, phases={phases_to_run}")

        import time
        start_time = time.monotonic()

        try:
            for phase_num in phases_to_run:
                if phase_num == 4:
                    # Phase 4 特殊處理：呼叫 Rust binary
                    await self._run_phase4()
                    continue

                await self._run_phase(phase_num, mode)

                # Phase 1 完成後重新解析股票清單（解決先雞後蛋問題）
                if phase_num == 1:
                    self._refresh_stock_list()

        except Exception as e:
            logger.error(f"Collector aborted. reason={e}")
            raise

        elapsed = int(time.monotonic() - start_time)
        logger.info(f"Collector finished. elapsed={elapsed}s")

    # =========================================================================
    # 執行單一 Phase
    # =========================================================================

    async def _run_phase(self, phase_num: int, mode: str) -> None:
        """
        執行指定 Phase 的所有已啟用 API 任務。

        Args:
            phase_num: Phase 編號（1-3, 5-6）
            mode:      "backfill" | "incremental"
        """
        import time

        apis = [
            a for a in self.config.apis
            if a.phase == phase_num and a.enabled
        ]
        logger.info(f"[Phase {phase_num}] Started. apis={len(apis)}")
        phase_start = time.monotonic()

        for api_config in apis:
            await self._run_api(api_config, mode)

        elapsed = int(time.monotonic() - phase_start)
        logger.info(f"[Phase {phase_num}] Completed. elapsed={elapsed}s")

    async def _run_api(self, api_config: ApiConfig, mode: str) -> None:
        """
        執行單一 API 設定的所有股票 × 日期段組合。

        Args:
            api_config: API 設定
            mode:       "backfill" | "incremental"
        """
        stock_ids = self._resolve_stock_ids(api_config)
        has_post_process = bool(api_config.post_process)

        for stock_id in stock_ids:
            segments = self.date_segmenter.segments(api_config, mode, stock_id)

            for (seg_start, seg_end) in segments:
                # 斷點續傳：已完成或空結果的 segment 直接跳過
                if self.sync_tracker.is_completed(api_config.name, stock_id, seg_start):
                    logger.info(
                        f"[Phase {api_config.phase}][{api_config.name}] "
                        f"Skipped stock={stock_id}, segment={seg_start}~{seg_end} (completed)"
                    )
                    continue

                logger.info(
                    f"[Phase {api_config.phase}][{api_config.name}] "
                    f"Start stock={stock_id}, segment={seg_start}~{seg_end}"
                )

                if self.dry_run:
                    logger.info("[dry-run] 跳過實際 API 呼叫")
                    continue

                # 呼叫 API
                try:
                    raw_records = await self.client.fetch(
                        api_config, stock_id, seg_start, seg_end
                    )
                except APIError as e:
                    logger.error(
                        f"Max retries exceeded. "
                        f"dataset={api_config.dataset}, stock={stock_id}, "
                        f"segment={seg_start}~{seg_end}. error={e}"
                    )
                    self.sync_tracker.mark_failed(
                        api_config.name, stock_id, seg_start, seg_end, str(e)
                    )
                    continue

                # 欄位映射
                rows = self.field_mapper.transform(api_config, raw_records)

                # Phase E：聚合策略（pivot / pack）
                # 在 field_mapper 之後、DB 寫入之前，對需要跨列合併的資料執行聚合
                if api_config.aggregation and rows:
                    try:
                        rows = apply_aggregation(
                            api_config.aggregation,
                            rows,
                            stmt_type=api_config.stmt_type,
                        )
                    except Exception as e:
                        logger.warning(
                            f"[{api_config.name}] aggregation={api_config.aggregation} "
                            f"失敗，跳過此 segment：{e}"
                        )
                        self.sync_tracker.mark_failed(
                            api_config.name, stock_id, seg_start, seg_end, str(e)
                        )
                        continue

                # 寫入 DB（merge_strategy 特殊處理）
                if api_config.merge_strategy == "update_delist_date":
                    self._merge_delist_date(rows)
                elif rows:
                    self.db.upsert(api_config.target_table, rows, primary_keys=["market", "stock_id", "date"])

                # 更新進度
                status = "empty" if not rows else "completed"
                self.sync_tracker.mark_progress(
                    api_config.name, stock_id, seg_start, seg_end,
                    status=status, record_count=len(rows),
                )

                logger.info(
                    f"[Phase {api_config.phase}][{api_config.name}] "
                    f"Done stock={stock_id}, segment={seg_start}~{seg_end}, "
                    f"records={len(rows)}"
                )

        # Post-process（如 dividend_policy_merge）
        if has_post_process:
            await self._run_post_process(api_config, stock_ids)

    # =========================================================================
    # Phase 4（Rust）
    # =========================================================================

    async def _run_phase4(self) -> None:
        """呼叫 Rust binary 執行後復權 + 週K/月K 聚合"""
        if self.rust_runner is None:
            logger.warning("[Phase 4] rust_runner 未設定，跳過 Phase 4")
            return

        logger.info("[Phase 4] Started（呼叫 Rust binary）")
        mode = self.config.execution.mode
        await self.rust_runner(mode=mode)

    # =========================================================================
    # Post-Process
    # =========================================================================

    async def _run_post_process(
        self,
        api_config: ApiConfig,
        stock_ids: list[str],
    ) -> None:
        """
        執行 api_config.post_process 指定的後處理邏輯。
        目前只支援 "dividend_policy_merge"。

        Args:
            api_config: 觸發 post_process 的 API 設定
            stock_ids:  要處理的股票清單
        """
        if api_config.post_process == "dividend_policy_merge":
            from post_process import dividend_policy_merge
            logger.info(f"[post_process] dividend_policy_merge: {len(stock_ids)} 檔")
            for stock_id in stock_ids:
                if stock_id == ALL_MARKET_SENTINEL:
                    continue
                dividend_policy_merge(self.db, stock_id)

    # =========================================================================
    # 輔助方法
    # =========================================================================

    def _resolve_stock_ids(self, api_config: ApiConfig) -> list[str]:
        """
        依 param_mode 決定要迭代的股票清單。

        all_market / all_market_no_id → ["__ALL__"]（sentinel）
        fixed_stock_ids               → 使用固定清單（如 SPY, ^VIX）
        per_stock / per_stock_no_end  → 使用動態股票清單

        Args:
            api_config: API 設定

        Returns:
            要迭代的股票代碼清單
        """
        if api_config.param_mode in ("all_market", "all_market_no_id"):
            return [ALL_MARKET_SENTINEL]

        if api_config.fixed_stock_ids:
            return api_config.fixed_stock_ids

        return self._stock_list

    def _refresh_stock_list(self) -> None:
        """
        Phase 1 完成後重新從 DB 解析股票清單。
        解決「首次執行時 stock_info 表為空」的先雞後蛋問題。
        """
        self._stock_list = stock_resolver.resolve(self.stock_list_cfg, self.db)
        logger.info(f"StockList refreshed from DB. total={len(self._stock_list)}")

    def _merge_delist_date(self, rows: list[dict]) -> None:
        """
        特殊合併策略：只更新 stock_info 表的 delist_date 欄位。
        用於 TaiwanStockDelisting 的資料處理。

        Args:
            rows: 已映射的資料列
        """
        for row in rows:
            stock_id   = row.get("stock_id")
            delist_date = row.get("date") or row.get("delist_date")
            if not stock_id or not delist_date:
                continue

            self.db.update(
                """
                UPDATE stock_info
                SET delist_date = ?, updated_at = datetime('now')
                WHERE market = 'TW' AND stock_id = ?
                """,
                [delist_date, stock_id],
            )
