"""
silver/orchestrator.py
======================
Phase 7 排程入口:讀 dirty queue 派工到對應 builder。

PR #19c 落地:
  - SilverOrchestrator.run() 真實邏輯(串列跑,parallel 留 follow-up)
  - 7a 跑 12 個獨立 Silver builder(沒實作的會被優雅跳過)
  - 7b 跑 financial_statement(依賴 monthly_revenue;monthly_revenue PR 動工後接)
  - 7c 派 rust_bridge.run_phase4 給 tw_market_core 系列(price_*_fwd + 漲跌停 merge)

為什麼是串列(不是 asyncio.gather):
  PostgresWriter 持單一 connection(非 pool),concurrent thread access 會踩
  psycopg 的 thread-safety 限制(connection 不是 thread-safe by 預設)。要平行
  跑 builder 需先升級 db 層為 connection pool 或 wrap 每個 builder 用 own conn。
  追求平行的 perf gain 在這層實際很小(每個 builder 是 SELECT *  + batch UPSERT,
  ~ms 量級),先求正確,平行優化留後續 PR(blueprint §三 後續迭代)。

stub builder 處理(防衛性 — 13 個 builder PR #19a-c 全實作完成,但 catch
NotImplementedError 防將來新加 stub):
  catch NotImplementedError → 標 skipped 不中斷其他 builder。
  catch 一般 Exception → 標 failed,reason 紀錄,**也不中斷其他 builder**
  (對齊 cores_overview §7.5 dirty 契約:失敗的 builder 不 reset is_dirty,
   下次 phase 再被選中重試)。

呼叫端(Phase 7 CLI):
    python src/main.py silver phase 7a [--stocks ...] [--full-rebuild]
    python src/main.py silver phase 7b
    python src/main.py silver phase 7c
"""

from __future__ import annotations

import logging
import time
from datetime import date, timedelta
from typing import Any

from ._common import clear_incremental_window, set_incremental_window
from .builders import BUILDERS

logger = logging.getLogger("collector.silver.orchestrator")


# =============================================================================
# Phase 分組(per blueprint §三 phase_executor 拆段)
# =============================================================================

# 7a — 不跨表 Silver,可平行(per spec §7.x — 12 + v3.21 3 = 15 張)
PHASE_7A_BUILDERS: list[str] = [
    "institutional",        # institutional_daily_derived
    "margin",               # margin_daily_derived(SBL 6 欄留 follow-up)
    "foreign_holding",      # foreign_holding_derived
    "holding_shares_per",   # holding_shares_per_derived
    "valuation",            # valuation_daily_derived(market_value_weight 留 follow-up)
    "day_trading",          # day_trading_derived(此處 raw,ratio 衍生欄留 follow-up)
    "monthly_revenue",      # monthly_revenue_derived
    "taiex_index",          # taiex_index_derived
    "us_market_index",      # us_market_index_derived
    "exchange_rate",        # exchange_rate_derived
    "market_margin",        # market_margin_maintenance_derived(total_*_balance 留 follow-up)
    "business_indicator",   # business_indicator_derived
    # v3.21(2026-05-17):3 new builders
    "loan_collateral",      # loan_collateral_balance_derived(5 主欄 + 5 change_pct + JSONB)
    "block_trade",          # block_trade_derived(SUM by trade_type per stock,date)
    "commodity_macro",      # commodity_price_daily_derived(z-score / streak / momentum per commodity)
]

# 7b — 跨表依賴(需先算完 7a 才能跑)
PHASE_7B_BUILDERS: list[str] = [
    "financial_statement",   # 需 monthly_revenue 對齊(對齊邏輯留 follow-up)
    # magic_formula_ranked:v3.5 R3 搬到 cross_cores/(Phase 8)— per-stock 契約違規
    # day_trading 補 day_trading_ratio:需 price_daily.volume(7c 後再算)— follow-up
]

# 7c — tw_market_core Rust 系列(走 rust_bridge,不在 builders/ 下)
PHASE_7C_RUST_TARGETS: list[str] = [
    "price_daily_fwd",
    "price_weekly_fwd",
    "price_monthly_fwd",
    "price_limit_merge_events",
]


PHASE_GROUPS: dict[str, list[str]] = {
    "7a": PHASE_7A_BUILDERS,
    "7b": PHASE_7B_BUILDERS,
    "7c": PHASE_7C_RUST_TARGETS,
}


# =============================================================================
# Orchestrator
# =============================================================================

class SilverOrchestrator:
    """Phase 7 排程器。

    Args:
        db:          DBWriter
        rust_bridge: RustBridge instance(7c 才用),None 時 7c 會 raise
    """

    def __init__(self, db: Any, rust_bridge: Any | None = None):
        self.db          = db
        self.rust_bridge = rust_bridge

    # --------------------------------------------------------------- public
    async def run(
        self,
        phases: list[str],
        stock_ids: list[str] | None = None,
        full_rebuild: bool = False,
        builders: list[str] | None = None,
    ) -> dict[str, Any]:
        """跑指定 Phase 7 子階段。

        Args:
            phases:       e.g. ["7a"], ["7a", "7b"], ["7a", "7b", "7c"]
            stock_ids:    None = 全市場;否則只跑指定股(市場級 builder 一律忽略)
            full_rebuild: True = 全量重算所有 Silver row。False(預設)時 7a 走
                          incremental 窗口(只重算最近 N 天;v4.15),7b/7c 不受影響。
            builders:     None = 跑該 phase 全部 builder;否則只跑指定 names。
                          unknown name → ValueError(對齊 cross_cores orchestrator
                          pattern)。7c 不受影響(rust_bridge 走 stock filter,非
                          builder filter)。

        Returns:
            {
                "phases_run": list[str],
                "results":    dict[builder_name, builder_result_dict],
                "elapsed_ms": int,
            }
        """
        start = time.monotonic()

        unknown = [p for p in phases if p not in PHASE_GROUPS]
        if unknown:
            raise ValueError(
                f"未知 phase: {unknown}。可用:{sorted(PHASE_GROUPS)}"
            )

        results: dict[str, Any] = {}
        for phase in phases:
            logger.info(f"[Phase {phase}] start")
            if phase == "7c":
                # 7c 是 rust_bridge 派 stock_ids,沒有「builder」概念可選;
                # 若 user 傳了 --builder 給 7c 則 noop 跳過(不 raise,避免 7a+7b+7c 混跑時擋路)
                results[phase] = await self._run_7c(
                    stock_ids=stock_ids, full_rebuild=full_rebuild,
                )
            else:
                phase_builders = self._filter_builders(phase, builders)
                if phase == "7a" and not full_rebuild:
                    # v4.15:7a 非 full_rebuild 走 incremental 窗口(不再全量重算)
                    results[phase] = self._run_7a_incremental(
                        stock_ids=stock_ids, builders=phase_builders,
                    )
                else:
                    results[phase] = self._run_builders(
                        phase_builders,
                        stock_ids=stock_ids,
                        full_rebuild=full_rebuild,
                    )
            logger.info(f"[Phase {phase}] done")

        elapsed_ms = int((time.monotonic() - start) * 1000)
        return {
            "phases_run": list(phases),
            "results":    results,
            "elapsed_ms": elapsed_ms,
        }

    # ------------------------------------------------------- builder filter
    @staticmethod
    def _filter_builders(
        phase: str, builders: list[str] | None,
    ) -> list[str]:
        """套用 --builder filter 到指定 phase 的 builder 清單。

        builders=None → 回該 phase 全部 builder(預設行為)。
        若 builders 含不屬於該 phase 的 name → ValueError(對齊 cross_cores
        orchestrator early-fail pattern,user typo 早報)。
        """
        phase_all = PHASE_GROUPS[phase]
        if not builders:
            return phase_all
        unknown = [b for b in builders if b not in phase_all]
        if unknown:
            raise ValueError(
                f"未知 silver builder(phase {phase}): {unknown}。"
                f"可用:{sorted(phase_all)}"
            )
        # 保留 PHASE_GROUPS 內定義的順序(對齊執行 ordering)
        return [b for b in phase_all if b in builders]

    # ------------------------------------------------------------ classmethod
    @classmethod
    def builders_in_phase(cls, phase: str) -> list[str]:
        """回傳指定 phase 對應的 builder 清單。"""
        if phase not in PHASE_GROUPS:
            raise ValueError(f"未知 phase: {phase}。可用:{sorted(PHASE_GROUPS)}")
        return PHASE_GROUPS[phase]

    # ----------------------------------------------------------- 7a incremental
    # v4.15:7a 非 full_rebuild 不再全量重算。READ 窗讀回 Bronze 給 builder
    # warmup,WRITE 窗只 upsert 最近 N 天 Silver。warmup = READ - WRITE = 150 天
    # >> 任何 builder 的 history 依賴(最大 commodity_macro 60d z-score)。
    _INCR_READ_LOOKBACK_DAYS = 180
    _INCR_WRITE_LOOKBACK_DAYS = 30

    def _run_7a_incremental(
        self, *, stock_ids: list[str] | None,
        builders: list[str] | None = None,
    ) -> dict[str, dict[str, Any]]:
        """7a 非 full_rebuild:set incremental 窗口 → 跑 builder → clear。

        Silver 表已含完整歷史(過去 full rebuild 留下),incremental 只維護最近
        WRITE 窗;窗外舊 row 不動、保持正確。要全量重算用 `--full-rebuild`。
        若 incremental 間隔超過 WRITE 窗(預設 30 天)→ 跑一次 --full-rebuild 補。

        builders:None = PHASE_GROUPS["7a"] 全跑;否則跑指定子集(已由
        `_filter_builders` 校驗過 + 重排序)。
        """
        today = date.today()
        read_since  = today - timedelta(days=self._INCR_READ_LOOKBACK_DAYS)
        write_since = today - timedelta(days=self._INCR_WRITE_LOOKBACK_DAYS)
        logger.info(
            f"  [7a] incremental window:read >= {read_since},write >= {write_since}"
        )
        set_incremental_window(read_since, write_since)
        try:
            return self._run_builders(
                builders or PHASE_GROUPS["7a"],
                stock_ids=stock_ids, full_rebuild=False,
            )
        finally:
            clear_incremental_window()

    # --------------------------------------------------------------- private
    def _run_builders(
        self,
        names: list[str],
        *,
        stock_ids: list[str] | None,
        full_rebuild: bool,
    ) -> dict[str, dict[str, Any]]:
        """串列跑一組 builder。NotImplementedError → skipped(不中斷整個 phase)。"""
        out: dict[str, dict[str, Any]] = {}
        for name in names:
            module = BUILDERS.get(name)
            if module is None:
                logger.error(f"  [{name}] 不在 BUILDERS 註冊表,跳過")
                out[name] = {"name": name, "status": "missing"}
                continue

            try:
                result = module.run(
                    self.db,
                    stock_ids=stock_ids,
                    full_rebuild=full_rebuild,
                )
                result["status"] = "ok"
                out[name] = result
            except NotImplementedError as e:
                logger.warning(f"  [{name}] skipped(stub): {e}")
                out[name] = {"name": name, "status": "skipped", "reason": str(e)}
            except Exception as e:
                logger.error(f"  [{name}] FAILED: {e}", exc_info=True)
                out[name] = {"name": name, "status": "failed", "reason": str(e)}
                # 失敗不中斷其他 builder;對齊 cores_overview §7.5 dirty 契約:
                # 失敗的 builder 不 reset is_dirty,下次 phase 再被選中重試。
        return out

    async def _run_7c(
        self, *, stock_ids: list[str] | None, full_rebuild: bool,
    ) -> dict[str, Any]:
        """7c:呼叫 rust_bridge 跑 tw_market_core 後復權 + 漲跌停 merge。

        stock_ids 解讀(PR #20 dirty queue 接管後):
          - 用戶明確傳 list:pass through 給 Rust(manual ops / 開發測試)
          - None + full_rebuild=False:從 `price_daily_fwd.is_dirty=TRUE` 拉
            DISTINCT stock_id 派 Rust(走 dirty queue;blueprint §5.7 設計)
          - None + full_rebuild=True:從 `price_daily_fwd` 拉所有 DISTINCT stock_id
            派 Rust(全市場重算)

        移除前路徑(PR #19):None 直接 pass None 給 Rust → Rust 內部 SELECT
        `stock_sync_status.fwd_adj_valid=0`。PR #20 後該路徑由 trigger +
        orchestrator 接管,Rust 不再依賴 fwd_adj_valid。
        """
        if self.rust_bridge is None:
            raise RuntimeError(
                "Phase 7c 需要 rust_bridge,但 SilverOrchestrator 初始化時未傳。"
                "請在建構時傳 RustBridge instance。"
            )

        if stock_ids is None:
            stock_ids = self._fetch_dirty_fwd_stocks(full_rebuild=full_rebuild)
            if not stock_ids:
                msg = ("無 dirty stock(全市場重算)" if full_rebuild
                       else "price_daily_fwd.is_dirty queue 為空")
                logger.info(f"  [7c] {msg},skip Rust dispatch")
                return {"rust": None, "status": "ok", "reason": "no_stocks", "n_stocks": 0}

        logger.info(
            f"  [7c] dispatching to rust_bridge.run_phase4(stocks={len(stock_ids)})"
        )
        rust_result = await self.rust_bridge.run_phase4(
            stock_ids=stock_ids,
            mode="backfill",
        )
        return {
            "rust": rust_result, "status": "ok", "n_stocks": len(stock_ids),
        }

    def _fetch_dirty_fwd_stocks(self, *, full_rebuild: bool) -> list[str]:
        """從 price_daily_fwd 拉待算 stock_id 清單(PR #20 dirty queue pull)。

        full_rebuild=False:`WHERE is_dirty = TRUE` UNION 「Bronze 有但 Silver 沒」
                            的 circular bootstrap miss(2026-05-14 P1 fix)
        full_rebuild=True: 全部 DISTINCT stock_id from Bronze price_daily(全市場重算)

        **2026-05-14 P1 circular bootstrap fix**(對齊 P0 Gate runbook §6.1.1):
        原邏輯只拉 `price_daily_fwd.is_dirty=TRUE`,新 listing 的股票從未進過
        `price_daily_fwd`(dirty queue 永遠選不到)→ Phase 4 never runs → Silver
        永遠空。本 fix 加 fallback UNION:Bronze 有但 Silver 沒的 stocks 強制加入
        待算清單,打破 circular bootstrap。

        PostgresWriter 走 query() 回 list[dict],抽出 stock_id 欄。
        """
        if full_rebuild:
            sql = "SELECT DISTINCT stock_id FROM price_daily ORDER BY stock_id"
        else:
            # dirty queue + circular bootstrap fallback
            sql = """
                SELECT DISTINCT stock_id FROM (
                    -- normal dirty queue
                    SELECT stock_id FROM price_daily_fwd WHERE is_dirty = TRUE
                    UNION
                    -- circular bootstrap miss:Bronze 有但 Silver 沒
                    SELECT pd.stock_id FROM price_daily pd
                    WHERE NOT EXISTS (
                        SELECT 1 FROM price_daily_fwd pdf
                        WHERE pdf.market = pd.market AND pdf.stock_id = pd.stock_id
                    )
                ) t
                ORDER BY stock_id
            """
        rows = self.db.query(sql)
        return [r["stock_id"] for r in rows]
