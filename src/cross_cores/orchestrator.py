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

import logging
import time
from typing import Any

from cross_cores import magic_formula

logger = logging.getLogger("collector.cross_cores.orchestrator")


# 註冊表:name → module
BUILDERS: dict[str, Any] = {
    "magic_formula": magic_formula,
}


class CrossStockOrchestrator:
    """Phase 8 排程器。

    Args:
        db: DBWriter
    """

    def __init__(self, db: Any):
        self.db = db

    async def run(
        self,
        *,
        builders: list[str] | None = None,
        target_date: Any = None,
        full_rebuild: bool = False,
        lookback_days: int | None = None,
    ) -> dict[str, Any]:
        """跑指定的 cross-stock builders(預設全跑)。

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

        results: dict[str, Any] = {}
        for name in names:
            module = BUILDERS[name]
            logger.info(f"[Phase 8][{name}] start")
            try:
                kwargs: dict[str, Any] = {"full_rebuild": full_rebuild}
                if lookback_days is not None:
                    kwargs["lookback_days"] = lookback_days
                result = module.run(self.db, **kwargs)
                result["status"] = "ok"
                results[name] = result
                logger.info(
                    f"[Phase 8][{name}] done "
                    f"rows={result.get('rows_written', 0)} "
                    f"elapsed={result.get('elapsed_ms', 0)}ms"
                )
            except Exception as e:
                logger.error(f"[Phase 8][{name}] FAILED: {e}", exc_info=True)
                results[name] = {"name": name, "status": "failed", "reason": str(e)}

        elapsed_ms = int((time.monotonic() - start) * 1000)
        return {
            "phase":      "8",
            "results":    results,
            "elapsed_ms": elapsed_ms,
        }
