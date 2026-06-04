"""
cross_cores/orchestrator.py
===========================
Phase 8 排程入口:Cross-Stock Cores 跑全部 builders for given date。

CLI:
    python src/main.py cross_cores phase 8
    python src/main.py cross_cores phase 8 --builder magic_formula
    python src/main.py cross_cores phase 8 --date 2026-05-15 --full-rebuild

不走 dirty queue(全市場永遠重算 latest date 即可,~5s for MF);對映
silver/orchestrator 但語意更簡單。
"""

from __future__ import annotations

import asyncio
import logging
import os
import time
from typing import Any

from cross_cores import magic_formula
# v3.32:10 new cross_cores builders(對齊 plan/hashed-foraging-pixel.md)
from cross_cores import (
    persistent_momentum,
    revenue_momentum,
    institutional_concert,
    f_score,
    low_volatility,
    industry_adj_gp,
    long_term_low_vol,
    dividend_yield,
    mom_12_1,
    monthly_trigger,
)
# Wave Impulse Cross-Stock Screen(plan wave-impulse-cross-stock-virtual-papert.md)
from cross_cores import wave_impulse_screen

logger = logging.getLogger("collector.cross_cores.orchestrator")


# 註冊表:name → module
BUILDERS: dict[str, Any] = {
    "magic_formula":          magic_formula,
    # v3.32 Toolkit A(monthly)
    "persistent_momentum":    persistent_momentum,
    "revenue_momentum":       revenue_momentum,
    "institutional_concert":  institutional_concert,
    # v3.32 Toolkit B(quarterly)
    "f_score":                f_score,
    "low_volatility":         low_volatility,
    "industry_adj_gp":        industry_adj_gp,
    # v3.32 Toolkit C(annual)
    "long_term_low_vol":      long_term_low_vol,
    "dividend_yield":         dividend_yield,
    "mom_12_1":               mom_12_1,
    # v3.32 Layer 5
    "monthly_trigger":        monthly_trigger,
    # Wave Impulse Screen(讀 M3 structural_snapshots)
    "wave_impulse_screen":    wave_impulse_screen,
}


class CrossStockOrchestrator:
    """Phase 8 排程器。

    Args:
        db: DBWriter

    v4.36 並行:
      v3.3 PostgresWriter 已 ConnectionPool。builder 之間互不依賴(各寫自己的
      `*_ranked_derived`),改 `asyncio.gather + asyncio.to_thread + Semaphore`
      平行跑;並行度 = max(1, pool.max_size - 1)(留 1 conn 給 query 路徑)。
      原本 12 builder 串列(總和 ~30-60s,wave_impulse_screen 單一 ~21s)→
      並行下整體 ≈ 最慢 builder + 排隊時間。
    """

    def __init__(self, db: Any):
        self.db = db

    def _parallelism_limit(self) -> int:
        """並行 builder 上限 = max(1, DB pool max_size - 1)。

        留 1 conn 給其他 query 路徑(MCP / dashboards / refresh chain 同時用)。
        MagicMock / 無 pool 屬性 fixture → 走 env DB_POOL_SIZE fallback。
        """
        pool = getattr(self.db, "pool", None)
        max_size = getattr(pool, "max_size", None)
        if isinstance(max_size, int) and max_size > 0:
            return max(1, max_size - 1)
        try:
            return max(1, int(os.getenv("DB_POOL_SIZE", "8")) - 1)
        except ValueError:
            return 7

    async def run(
        self,
        *,
        builders: list[str] | None = None,
        target_date: Any = None,
        full_rebuild: bool = False,
        lookback_days: int | None = None,
    ) -> dict[str, Any]:
        """跑指定的 cross-stock builders(預設全跑)。

        v4.36:builder 之間 asyncio.gather 並行,各自走 asyncio.to_thread
        把同步 module.run() 派給 worker thread,event loop 不被 sync DB 操作擋。

        Args:
            builders:      None = 全跑;否則只跑指定 names
            target_date:   None = builder 自己決定 latest available
            full_rebuild:  True = 重算 lookback window 全部 dates
            lookback_days: full_rebuild 時往回幾天;None = builder 預設值
        """
        start = time.monotonic()

        names = builders or list(BUILDERS)
        unknown = [n for n in names if n not in BUILDERS]
        if unknown:
            raise ValueError(
                f"未知 cross_cores builder: {unknown}。可用:{sorted(BUILDERS)}"
            )

        if target_date is not None:
            logger.warning(
                "[Phase 8] target_date 參數目前 cross_cores builder 尚未支援"
                "(magic_formula 走 latest N dates),忽略。"
            )

        limit = self._parallelism_limit()
        sem = asyncio.Semaphore(limit)
        logger.info(
            f"[Phase 8] 並行 {len(names)} builder(concurrency={limit})"
        )

        async def _run_one(name: str) -> tuple[str, dict[str, Any]]:
            module = BUILDERS[name]
            logger.info(f"[Phase 8][{name}] start")
            async with sem:
                try:
                    kwargs: dict[str, Any] = {"full_rebuild": full_rebuild}
                    if lookback_days is not None:
                        kwargs["lookback_days"] = lookback_days
                    result = await asyncio.to_thread(module.run, self.db, **kwargs)
                    result["status"] = "ok"
                    logger.info(
                        f"[Phase 8][{name}] done "
                        f"rows={result.get('rows_written', 0)} "
                        f"elapsed={result.get('elapsed_ms', 0)}ms"
                    )
                    return name, result
                except Exception as e:
                    logger.error(f"[Phase 8][{name}] FAILED: {e}", exc_info=True)
                    return name, {"name": name, "status": "failed", "reason": str(e)}

        result_pairs = await asyncio.gather(*(_run_one(n) for n in names))
        results: dict[str, Any] = {name: result for name, result in result_pairs}

        elapsed_ms = int((time.monotonic() - start) * 1000)
        return {
            "phase":      "8",
            "results":    results,
            "elapsed_ms": elapsed_ms,
        }
