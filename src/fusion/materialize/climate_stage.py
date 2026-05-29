"""Golden L3 — climate_fusion 物化(marketwide aggregator,獨立非 per-stock)。

對齊 m3Spec/build-pipeline.md 決議 A:climate 是全市場 fan-in,不塞進 per-stock 的
fusion loop,自成一個 marketwide 階段。寫一筆 `core_name='climate_fusion'`、
`stock_id='_market_'`、`timeframe='_all_'` 進 structural_snapshots。
"""

from __future__ import annotations

import logging
import time
from datetime import date
from typing import Any

from fusion.materialize import _provenance as P
from fusion.raw._db import get_connection

logger = logging.getLogger("collector.fusion.materialize")


def run_climate_materialize(
    db: Any,
    *,
    as_of: date | None = None,
    lookback_days: int = 60,
    database_url: str | None = None,
) -> dict[str, Any]:
    """物化 climate_fusion(marketwide 一筆)。

    Args:
        db:            DBWriter(寫 structural_snapshots)。
        as_of:         物化日;None = price_daily 最新交易日。
        lookback_days: climate facts 期間(對齊 compute_market_context 預設 60)。
        database_url:  讀連線。

    Returns:
        {as_of, written, elapsed_ms, climate_score?}
    """
    from mcp_server._climate import compute_market_context

    start = time.monotonic()
    conn = get_connection(database_url)
    try:
        resolved = as_of or P.latest_trading_date(conn)
        if resolved is None:
            return {"as_of": None, "written": 0,
                    "elapsed_ms": int((time.monotonic() - start) * 1000),
                    "warnings": ["price_daily 無資料,無法決定 as_of"]}

        # 共享 conn(避免 compute_market_context 自開連線)
        doc = compute_market_context(resolved, lookback_days=lookback_days, conn=conn)
    finally:
        conn.close()

    row = P.build_row(
        stock_id=P.CLIMATE_STOCK_ID, snapshot_date=resolved,
        timeframe=P.CLIMATE_TIMEFRAME, core_name=P.CLIMATE_CORE,
        source_version=P.CLIMATE_SOURCE_VERSION,
        params_hash=P.climate_params_hash(lookback_days=lookback_days),
        snapshot=doc, derived_from_core=P.CLIMATE_DERIVED_FROM,
    )
    written = db.upsert("structural_snapshots", [row], P.PK_COLS)

    summary = {
        "as_of": resolved.isoformat(),
        "written": written,
        "climate_score": doc.get("climate_score"),
        "overall_climate": doc.get("overall_climate"),
        "elapsed_ms": int((time.monotonic() - start) * 1000),
    }
    logger.info(f"[golden.climate] done {summary}")
    return summary
