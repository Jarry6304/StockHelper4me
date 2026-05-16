"""
bronze/phase_executor.py
------------------------
Phase 1-6(Bronze 收集)排程引擎。

負責依 Phase 1→6 順序執行各 API 蒐集任務:
- Phase 1:META(全市場)
- Phase 2:EVENTS(每支股票)
- Phase 3:RAW PRICE(每支股票 × 日期分段)
- Phase 4:RUST 計算(呼叫 rust_bridge)
- Phase 5:CHIP / FUNDAMENTAL
- Phase 6:MACRO

v3.3 改動(對齊 plan §規格 4):
- `_run_api` 從序列 `for stock_id: for segment: ...` 改 `asyncio.gather`
  並發,搭配 `asyncio.Semaphore(BRONZE_CONCURRENCY)` 限同時 in-flight 任務數
- RateLimiter 是真正 throttle(5700/hr Sponsor ≈ 1.58/s),Semaphore 防爆
  連線 / memory
- v1.36 short-circuit fix 共存:用 `_DatasetErrorTracker` class 持 streak
  counter + `asyncio.Event` abort signal;**保留** `_DATASET_ERROR_CODES` /
  `_DATASET_ERROR_STREAK_THRESHOLD` class constants(既有 tests 不破)
- env knob `BRONZE_CONCURRENCY` 可緊急 dial-down(預設 12)

Phase 1 完成後需重新解析股票清單(先雞後蛋問題)。
"""

import asyncio
import logging
import os
from dataclasses import dataclass, field
from typing import Callable

from api_client import ALL_MARKET_SENTINEL, FinMindClient
from bronze.segment_runner import _SegmentRunner
from config_loader import ApiConfig, CollectorConfig, StockListConfig
from date_segmenter import DateSegmenter
from db import DBWriter
from field_mapper import FieldMapper
import stock_resolver
from sync_tracker import SyncTracker

logger = logging.getLogger("collector.phase_executor")


# Spec 4 並發上限:env BRONZE_CONCURRENCY 覆蓋(預設 12)。
# rollback knob:緊急 disable 並發改 1,行為退回近似序列(asyncio.gather 仍跑但只 1-in-flight)。
_DEFAULT_BRONZE_CONCURRENCY = 12


@dataclass
class _DatasetErrorTracker:
    """Spec 4 並發 + v1.36 short-circuit 共存的協作物件。

    在 asyncio.gather 並發場景下,若 dataset 整體被 FinMind 拒絕(403/404/422),
    100+ tasks 平行起跑時 streak 累計到 5 之前後面 99 個 request 已發出。
    本 tracker 用 asyncio.Event abort signal:
      - 每個 task 跑前先 check `aborted.is_set()` → 若是直接 return,不發 request
      - APIError 後若 status_code ∈ DATASET_ERROR_CODES → increment();超
        threshold → `aborted.set()`,通知尚未起跑的 task 跳過

    並發安全:streak / lock 在 asyncio 單執行緒下不需顯式 lock(原子 int +=
    在 Python 仍是 thread-safe;asyncio await 點才會切換 task)。
    """
    threshold: int
    error_codes: frozenset[int]
    streak: int = 0
    aborted: asyncio.Event = field(default_factory=asyncio.Event)

    def record_error(self, status_code: int | None) -> None:
        """APIError 後呼叫。dataset-level error 累計;非 dataset-level reset。"""
        if status_code in self.error_codes:
            self.streak += 1
            if self.streak >= self.threshold:
                self.aborted.set()
        else:
            self.streak = 0

    def record_success(self) -> None:
        """成功 fetch 後 reset streak。"""
        self.streak = 0


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

    # ---- Short-circuit thresholds for dataset-level errors -----------------
    # 連續 N 次同 dataset 收到 dataset-level error(403 / 404 / 422)→ abort 整個 entry
    # 剩餘 stocks(對齊「FinMind quota / tier / dataset 下架等系統性問題,不該 retry 1700+ 股」場景)
    # v3.3 保留 class constants(既有 tests 不破),但實際短路邏輯改走 _DatasetErrorTracker。
    _DATASET_ERROR_CODES: frozenset[int] = frozenset({403, 404, 422})
    _DATASET_ERROR_STREAK_THRESHOLD: int = 5

    async def _run_api(self, api_config: ApiConfig, mode: str) -> None:
        """執行單一 API 設定的所有股票 × 日期段組合(v3.3 並發版)。

        v3.5 R1 C3 後 orchestration only:
          - 解析 stock 清單 + 預先蒐集 (stock_id, seg_start, seg_end) tasks
          - 建 `_SegmentRunner`(共享 tracker + sem)
          - `asyncio.gather` 並發跑全部 task
          - short-circuit 後 dispatch post_process

        Args:
            api_config: API 設定
            mode:       "backfill" | "incremental"
        """
        stock_ids = self._resolve_stock_ids(api_config)
        has_post_process = bool(api_config.post_process)

        # env knob:緊急可改 BRONZE_CONCURRENCY=1 退回近似序列
        concurrency = int(os.getenv("BRONZE_CONCURRENCY", str(_DEFAULT_BRONZE_CONCURRENCY)))
        sem = asyncio.Semaphore(max(1, concurrency))

        tracker = _DatasetErrorTracker(
            threshold=self._DATASET_ERROR_STREAK_THRESHOLD,
            error_codes=self._DATASET_ERROR_CODES,
        )

        # 預先蒐集 (stock_id, seg_start, seg_end) tasks
        # date_segmenter.segments 仍是 sync call(查 get_last_sync 走 DB),
        # 在進入 gather 前一次性算完 — 後續 fetch 才是 IO bound 部分
        tasks: list[tuple[str, str, str]] = []
        for stock_id in stock_ids:
            for (seg_start, seg_end) in self.date_segmenter.segments(api_config, mode, stock_id):
                tasks.append((stock_id, seg_start, seg_end))

        logger.info(
            f"[Phase {api_config.phase}][{api_config.name}] "
            f"Started, total_tasks={len(tasks)}, concurrency={concurrency}"
        )

        runner = _SegmentRunner(
            api_config=api_config,
            db=self.db,
            client=self.client,
            field_mapper=self.field_mapper,
            sync_tracker=self.sync_tracker,
            get_trading_dates=self._get_trading_dates,
            tracker=tracker,
            sem=sem,
            dry_run=self.dry_run,
        )

        await asyncio.gather(
            *(runner.run(sid, ss, se) for sid, ss, se in tasks),
            return_exceptions=False,
        )

        if tracker.aborted.is_set():
            # short-circuit 觸發 → 不跑 post_process(可能也需重整 streak)
            return

        # Post-process(如 dividend_policy_merge)
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

        v1.26 起加入「incremental 模式 dirty queue 偵測」優化:
          - backfill: 一律傳完整 self._stock_list 給 Rust(全市場重算)
          - incremental: 先查 price_daily_fwd.is_dirty=TRUE 的 distinct stock_id,
            只對 dirty stocks 派工;0 dirty → skip Rust dispatch 完整省 ~6 分鐘
            (1700+ stocks × 200ms/檔 ~= 6 分鐘的 rust pricing latency)
          - 對齊 silver/orchestrator._run_7c 的同款 dirty queue pattern

        註：Rust process_stock 永遠全量重算（後復權 multiplier 從尾端倒推，新除權息
            事件會回頭改全段歷史值，partial 邏輯上是錯的；詳見 main.rs 的 docstring）。
            mode 參數目前不影響 Rust 內部行為,但 Python 端 mode=incremental 會走
            dirty queue filter,只送 dirty stocks 給 Rust。
        """
        if self.rust_runner is None:
            logger.warning("[Phase 4] rust_runner 未設定，跳過 Phase 4")
            return

        if mode == "incremental":
            dirty_stocks = self._fetch_dirty_fwd_stocks()
            # v3.4 r2:--stocks 透過 stock_list_cfg.dev_enabled=True 表示「user
            # 顯式限縮範圍」。dirty queue intersect with self._stock_list 避免
            # Phase 4 送 1700+ stocks 給 Rust 而 user 只想動 10 檔(對齊
            # main.py:296 `if args.stocks: stock_list_cfg.dev_enabled = True`)。
            if self.stock_list_cfg.dev_enabled and self._stock_list:
                allowed = set(self._stock_list)
                before = len(dirty_stocks)
                dirty_stocks = [s for s in dirty_stocks if s in allowed]
                logger.info(
                    f"[Phase 4] --stocks filter: dirty queue {before} → {len(dirty_stocks)} "
                    f"(intersect with {len(allowed)} user-specified stocks)"
                )
            if not dirty_stocks:
                logger.info(
                    "[Phase 4] dirty queue 為空(無新除權息事件),"
                    "skip Rust dispatch"
                )
                return
            logger.info(
                f"[Phase 4] Started(dirty queue pull)dirty_stocks={len(dirty_stocks)}, "
                f"mode={mode}"
            )
            await self.rust_runner(mode=mode, stock_ids=dirty_stocks)
            return

        # backfill:全量送 Rust(對齊 v1.25 之前行為)
        logger.info(
            f"[Phase 4] Started(全市場重算)stocks={len(self._stock_list)}, mode={mode}"
        )
        await self.rust_runner(mode=mode, stock_ids=self._stock_list)

    def _fetch_dirty_fwd_stocks(self) -> list[str]:
        """從 price_daily_fwd.is_dirty=TRUE 拉 distinct stock_id(PR #20 dirty queue)。

        對齊 silver/orchestrator._fetch_dirty_fwd_stocks 的相同 pattern。
        Rust process_stock 跑完會 DELETE+INSERT price_daily_fwd,
        新 row 的 is_dirty default=FALSE → dirty queue 自動 drain。
        """
        sql = (
            "SELECT DISTINCT stock_id FROM price_daily_fwd "
            "WHERE is_dirty = TRUE ORDER BY stock_id"
        )
        rows = self.db.query(sql)
        return [r["stock_id"] for r in rows]

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
            from bronze.post_process_dividend import dividend_policy_merge
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
            rows = self.db.query("SELECT date FROM trading_date_ref")  # v3.2 R-1: trading_calendar → trading_date_ref
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

    # v3.5 R1 C3:_merge_delist_date 搬到 bronze/segment_runner.py(內部用)。
