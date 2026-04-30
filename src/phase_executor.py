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
from db import DBWriter   # 改成 type hint 用,實際 instantiate 由呼叫方傳入
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
        self.field_mapper    = FieldMapper(db=db)
        # sync_tracker 傳入 DateSegmenter，供 incremental 模式查詢上次同步日期
        self.date_segmenter  = DateSegmenter(config, sync_tracker)

        # 初始解析股票清單。
        # - dev_enabled = True（含 --stocks 覆蓋）→ 直接回 static_ids，不查 DB
        # - 否則查 DB；首次執行時 stock_info 表可能為空，phase 1 完成後會再 refresh 一次
        self._stock_list: list[str] = stock_resolver.resolve(stock_list_cfg, db)
        logger.info(f"StockList initial resolve. total={len(self._stock_list)}")

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
                    # mode 從 CLI runtime 傳入（main.py 依 command 決定 backfill/incremental），
                    # 不從 config.execution.mode 讀，避免 toml 寫死 backfill 但 CLI 跑 incremental 時錯位
                    await self._run_phase4(mode)
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

                # 欄位映射（回傳 rows 與 schema_mismatch 旗標）
                rows, schema_mismatch = self.field_mapper.transform(api_config, raw_records)

                # 若欄位定義與 API 回傳不符，記錄 schema_mismatch（仍繼續入庫）
                if schema_mismatch:
                    self.sync_tracker.mark_schema_mismatch(
                        api_config.name, stock_id, seg_start, seg_end,
                        record_count=len(rows),
                    )

                # Phase E：聚合策略（pivot / pack）
                # 在 field_mapper 之後、DB 寫入之前，對需要跨列合併的資料執行聚合
                if api_config.aggregation and rows:
                    try:
                        # institutional 兩個策略需要 trading_dates 過濾掉
                        # FinMind 週六回的鬼資料；其他策略傳 None 即可
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
                    # 依 table 決定 PK（對應 schema_pg.sql 的 PRIMARY KEY 定義）
                    _TABLE_PKS = {
                        "stock_info":                ["market", "stock_id"],
                        "trading_calendar":          ["market", "date"],
                        "exchange_rate":             ["market", "date", "currency"],
                        "fear_greed_index":          ["market", "date"],
                        "institutional_market_daily":["market", "date"],
                        "market_margin_maintenance": ["market", "date"],
                        "financial_statement":       ["market", "stock_id", "date", "type"],
                        "price_adjustment_events":   ["market", "stock_id", "date", "event_type"],
                        "_dividend_policy_staging":  ["market", "stock_id", "date"],
                    }
                    pks = _TABLE_PKS.get(api_config.target_table, ["market", "stock_id", "date"])
                    self.db.upsert(api_config.target_table, rows, primary_keys=pks)

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

    async def _run_phase4(self, mode: str) -> None:
        """呼叫 Rust binary 執行後復權 + 週K/月K 聚合。

        Args:
            mode: "backfill" | "incremental"，由 run() 從 CLI command 決定後傳入。
                  Rust binary 端目前未消費 mode（process_stock 永遠全段重算），
                  但保留參數對齊 CLI 語意，避免 toml execution.mode 與 CLI 命令錯位。

        註：Rust process_stock 永遠全量重算（後復權 multiplier 從尾端倒推，新除權息
            事件會回頭改全段歷史值，partial 邏輯上是錯的；詳見 main.rs 的 docstring）。
            mode 參數目前不影響 Rust 行為，但保留供未來「Python 偵測除權息變化後決定
            要不要叫 Rust」的優化空間。
        """
        if self.rust_runner is None:
            logger.warning("[Phase 4] rust_runner 未設定，跳過 Phase 4")
            return

        logger.info(
            f"[Phase 4] Started（呼叫 Rust binary）stocks={len(self._stock_list)}, mode={mode}"
        )
        await self.rust_runner(mode=mode, stock_ids=self._stock_list)

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

        all_market / all_market_no_id → ["__ALL__"]（sentinel，不送 data_id）
        per_stock_fixed               → 使用 fixed_ids（如 SPY, ^VIX, TAIEX）
        per_stock / per_stock_no_end  → 優先 fixed_ids，否則動態股票清單

        Args:
            api_config: API 設定

        Returns:
            要迭代的股票代碼清單
        """
        if api_config.param_mode in ("all_market", "all_market_no_id"):
            return [ALL_MARKET_SENTINEL]

        # fixed_ids 為主要欄位名，fixed_stock_ids 為舊版相容
        fixed = api_config.fixed_ids or api_config.fixed_stock_ids

        if api_config.param_mode == "per_stock_fixed":
            if not fixed:
                logger.warning(
                    f"[{api_config.name}] per_stock_fixed 但無 fixed_ids，跳過此 API"
                )
                return []
            return fixed

        # per_stock / per_stock_no_end：先看 fixed_ids（如 v1.2 用法），否則動態清單
        if fixed:
            return fixed

        return self._stock_list

    def _refresh_stock_list(self) -> None:
        """
        Phase 1 完成後重新從 DB 解析股票清單。
        解決「首次執行時 stock_info 表為空」的先雞後蛋問題。
        """
        self._stock_list = stock_resolver.resolve(self.stock_list_cfg, self.db)
        logger.info(f"StockList refreshed from DB. total={len(self._stock_list)}")

    def _get_trading_dates(self) -> set[str]:
        """
        Lazy-load trading_calendar 日期集合（per-process 快取一次）。
        institutional aggregator 用來過濾 FinMind 在週六回的鬼資料。
        若 trading_calendar 表為空（罕見：Phase 1 還沒跑），回傳空集合 →
        aggregator 不做過濾、保留全部 rows。
        """
        if not hasattr(self, "_trading_dates_cache"):
            rows = self.db.query("SELECT date FROM trading_calendar")
            # psycopg3 從 Postgres DATE 欄位回傳 datetime.date，
            # 但 FinMind rows 的 date 是 str（"2024-01-15"），
            # 統一轉成 str 避免 set membership check 永遠 False
            self._trading_dates_cache = {
                r["date"].isoformat() if hasattr(r["date"], "isoformat") else str(r["date"])
                for r in rows
            }
            logger.debug(
                f"trading_dates cache loaded: {len(self._trading_dates_cache)} days"
            )
        return self._trading_dates_cache

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
                SET delist_date = %s, updated_at = NOW()
                WHERE market = 'TW' AND stock_id = %s
                """,
                [delist_date, stock_id],
            )
