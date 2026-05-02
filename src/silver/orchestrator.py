"""
silver/orchestrator.py
======================
Phase 7 排程入口:讀 dirty queue 派工到對應 builder。

PR #19a 骨架(本檔):
  - SilverOrchestrator class(skeleton,run() raise NotImplementedError)
  - PHASE_GROUPS 對映表:7a 平行 11 張 / 7b 跨表依賴 2 張 / 7c Rust 後復權系列

PR #19c 補實際邏輯:
  - 7a 平行(asyncio.gather): institutional / margin / foreign_holding /
        holding_shares_per / valuation / day_trading / monthly_revenue /
        taiex / us_market / exchange_rate / market_margin / business_indicator
  - 7b 跨表依賴:financial_statement(需 monthly_revenue 對齊) /
        day_trading(需 price_daily volume 算 day_trading_ratio)
  - 7c tw_market_core 系列:price_daily_fwd / price_weekly_fwd / price_monthly_fwd /
        price_limit_merge_events,全 Rust(走 rust_bridge.run_phase4)

呼叫端(PR #19c CLI 整合):
    python src/main.py silver phase 7a [--stocks ...] [--full-rebuild]
"""

from __future__ import annotations

import logging
from typing import Any

logger = logging.getLogger("collector.silver.orchestrator")


# =============================================================================
# Phase 分組(per blueprint §三 phase_executor 拆段)
# =============================================================================

# 7a — 不跨表 Silver,可平行(per spec §7.x — 11 張 + business_indicator = 12 張平行)
PHASE_7A_BUILDERS: list[str] = [
    "institutional",        # institutional_daily_derived
    "margin",               # margin_daily_derived(整合 SBL)
    "foreign_holding",      # foreign_holding_derived
    "holding_shares_per",   # holding_shares_per_derived
    "valuation",            # valuation_daily_derived
    "day_trading",          # day_trading_derived(此處 raw,7b 才補 ratio)
    "monthly_revenue",      # monthly_revenue_derived
    "taiex_index",          # taiex_index_derived
    "us_market_index",      # us_market_index_derived
    "exchange_rate",        # exchange_rate_derived
    "market_margin",        # market_margin_maintenance_derived
    "business_indicator",   # business_indicator_derived
]

# 7b — 跨表依賴(需先算完 7a 才能跑)
PHASE_7B_BUILDERS: list[str] = [
    "financial_statement",  # 需 monthly_revenue 對齊
    # day_trading 補 ratio 階段:需 price_daily volume(7c 完成後再算)
    # — 這層在 PR #19c 視必要拆出獨立 step,目前留 7a 算 raw 部份
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
    """Phase 7 排程器(skeleton)。

    PR #19a 階段:run() raise NotImplementedError,只 expose 結構供 PR #19c 填邏輯。
    """

    def __init__(self, db: Any, rust_bridge: Any | None = None):
        """
        Args:
            db:           DBWriter
            rust_bridge:  RustBridge instance(7c 才用),None 時 7c 會 raise
        """
        self.db          = db
        self.rust_bridge = rust_bridge

    def run(
        self,
        phases: list[str],
        stock_ids: list[str] | None = None,
        full_rebuild: bool = False,
    ) -> dict[str, Any]:
        """跑指定 Phase 7 子階段(7a / 7b / 7c)。

        PR #19c 動工:
          - 7a 平行 asyncio.gather 所有 PHASE_7A_BUILDERS
          - 7b 序列(需 7a 完成的依賴表)
          - 7c 呼叫 rust_bridge.run_phase4(stock_ids) 觸發 Rust 系列

        Args:
            phases:       ["7a"] / ["7a", "7b"] / ["7a", "7b", "7c"] 等
            stock_ids:    None = 全市場;否則只跑指定股
            full_rebuild: True = 忽略 dirty queue 全部重算

        Returns:
            {
                "phases_run": list[str],
                "results": dict[builder_name, builder_result_dict],
                "elapsed_ms": int,
            }

        Raises:
            NotImplementedError: PR #19a 階段未實作
        """
        raise NotImplementedError(
            "SilverOrchestrator.run() 留 PR #19c 動工。"
            "目前只有 PR #19a schema scaffolding,builders 都是 NotImplementedError stub。"
        )

    @classmethod
    def builders_in_phase(cls, phase: str) -> list[str]:
        """回傳指定 phase 對應的 builder 清單(不論 7a/7b/7c)。"""
        if phase not in PHASE_GROUPS:
            raise ValueError(f"未知 phase: {phase}。可用:{sorted(PHASE_GROUPS)}")
        return PHASE_GROUPS[phase]
