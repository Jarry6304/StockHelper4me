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
from typing import Any

from .builders import BUILDERS

logger = logging.getLogger("collector.silver.orchestrator")


# =============================================================================
# Phase 分組(per blueprint §三 phase_executor 拆段)
# =============================================================================

# 7a — 不跨表 Silver,可平行(per spec §7.x — 11 張 + business_indicator = 12 張)
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
]

# 7b — 跨表依賴(需先算完 7a 才能跑)
PHASE_7B_BUILDERS: list[str] = [
    "financial_statement",  # 需 monthly_revenue 對齊(對齊邏輯留 follow-up)
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
    ) -> dict[str, Any]:
        """跑指定 Phase 7 子階段。

        Args:
            phases:       e.g. ["7a"], ["7a", "7b"], ["7a", "7b", "7c"]
            stock_ids:    None = 全市場;否則只跑指定股(市場級 builder 一律忽略)
            full_rebuild: True = 忽略 dirty queue 全部重算(目前唯一支援的模式;
                          dirty queue pull 留 PR 後續動工)

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
                results[phase] = await self._run_7c(
                    stock_ids=stock_ids, full_rebuild=full_rebuild,
                )
            else:
                results[phase] = self._run_builders(
                    PHASE_GROUPS[phase],
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

    # ------------------------------------------------------------ classmethod
    @classmethod
    def builders_in_phase(cls, phase: str) -> list[str]:
        """回傳指定 phase 對應的 builder 清單。"""
        if phase not in PHASE_GROUPS:
            raise ValueError(f"未知 phase: {phase}。可用:{sorted(PHASE_GROUPS)}")
        return PHASE_GROUPS[phase]

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

        full_rebuild=False:`WHERE is_dirty = TRUE`(走 dirty queue)
        full_rebuild=True: 全部 DISTINCT stock_id(全市場重算;ignore is_dirty)

        PostgresWriter 走 query() 回 list[dict],抽出 stock_id 欄。
        """
        sql = (
            "SELECT DISTINCT stock_id FROM price_daily_fwd"
            + ("" if full_rebuild else " WHERE is_dirty = TRUE")
            + " ORDER BY stock_id"
        )
        rows = self.db.query(sql)
        return [r["stock_id"] for r in rows]
