"""Golden L3 — levels_fusion + resonance_fusion 物化階段。

把 read-time `key_levels()`(per-stock)+ `resonance()`(per-stock × 3 tf)的輸出
write-time 呼叫 + UPSERT 進 `structural_snapshots`(新 core_name)。

設計(對齊 m3Spec/build-pipeline.md):
- **不是 CrossStockBuilder**(那些寫 *_ranked_derived 欄式表);fusion 寫 JSONB 文件,自成 stage。
- 讀用共享 `get_connection()`(dict_row autocommit)conn 傳給 key_levels/resonance(零改寫)。
- 寫用 `DBWriter.upsert("structural_snapshots", rows, PK)`(已處理 ON CONFLICT + dict→Jsonb)。
- daily = always-recompute-latest(不 skip);backfill = skip-if-exists(靠 fusion row 自身 version)。
- per-stock graceful:單股失敗不中斷整個 universe(對齊 cross_cores orchestrator)。
"""

from __future__ import annotations

import logging
import time
from datetime import date
from typing import Any

from fusion.dual_track.resonance import resonance
from fusion.key_levels import key_levels
from fusion.materialize import _provenance as P
from fusion.raw._db import get_connection

logger = logging.getLogger("collector.fusion.materialize")

_BATCH = 500  # flush 批量(控記憶體)


def run_fusion_materialize(
    db: Any,
    *,
    as_of: date | None = None,
    stocks: list[str] | None = None,
    only: set[str] | None = None,
    backfill: bool = False,
    database_url: str | None = None,
) -> dict[str, Any]:
    """物化 levels_fusion + resonance_fusion。

    Args:
        db:          DBWriter(寫 structural_snapshots)。
        as_of:       物化日;None = price_daily 最新交易日。
        stocks:      限縮 universe;None = price_daily_fwd 全市場。
        only:        {"levels", "resonance"} 子集;None = 兩者都做。
        backfill:    True = skip-if-exists(回填歷史用);False(daily)= always-recompute。
        database_url: 讀連線(預設 env / .env)。

    Returns:
        {as_of, levels_written, resonance_written, skipped, errors, elapsed_ms, warnings}
    """
    start = time.monotonic()
    want = only or {"levels", "resonance"}
    conn = get_connection(database_url)

    levels_written = 0
    resonance_written = 0
    skipped = 0
    errors = 0
    warnings: list[str] = []
    rows_lv: list[dict[str, Any]] = []
    rows_rz: list[dict[str, Any]] = []

    def _flush() -> None:
        nonlocal rows_lv, rows_rz, levels_written, resonance_written
        if rows_lv:
            levels_written += db.upsert("structural_snapshots", rows_lv, P.PK_COLS)
            rows_lv = []
        if rows_rz:
            resonance_written += db.upsert("structural_snapshots", rows_rz, P.PK_COLS)
            rows_rz = []

    try:
        resolved = as_of or P.latest_trading_date(conn)
        if resolved is None:
            return {
                "as_of": None,
                "levels_written": 0,
                "resonance_written": 0,
                "skipped": 0,
                "errors": 0,
                "elapsed_ms": int((time.monotonic() - start) * 1000),
                "warnings": ["price_daily 無資料,無法決定 as_of"],
            }

        # resonance track2 stale 警告(對齊 plan 風險:forecast 非全市場時 track2 多 single_track)
        if "resonance" in want:
            lag = P.forecast_log_lag_days(conn, resolved)
            if lag is None:
                warnings.append(
                    "forecast_log 無 external row → resonance track2 全市場缺 band(全 single_track)。"
                    "需先跑 forecast 全市場校準(Phase 3b)。"
                )
            elif lag > 7:
                warnings.append(
                    f"forecast_log 最新 forecast_date 落後 as_of {lag} 天 → resonance track2 多數 stale。"
                )

        universe = P.fetch_universe(conn, stocks)
        logger.info(
            f"[golden.fusion] as_of={resolved} universe={len(universe)} "
            f"only={sorted(want)} backfill={backfill}"
        )

        lv_hash = P.levels_params_hash()
        rz_hash = P.resonance_params_hash()

        for sid in universe:
            # ── levels(per-stock,哨兵 tf _all_)────────────────────────────
            if "levels" in want:
                if backfill and P.fusion_row_exists(
                    conn, stock_id=sid, timeframe=P.LEVELS_TIMEFRAME,
                    core_name=P.LEVELS_CORE, snapshot_date=resolved,
                    source_version=P.LEVELS_SOURCE_VERSION,
                ):
                    skipped += 1
                else:
                    try:
                        doc = key_levels(sid, resolved, conn=conn)
                        rows_lv.append(P.build_row(
                            stock_id=sid, snapshot_date=resolved,
                            timeframe=P.LEVELS_TIMEFRAME, core_name=P.LEVELS_CORE,
                            source_version=P.LEVELS_SOURCE_VERSION, params_hash=lv_hash,
                            snapshot=doc, derived_from_core=P.LEVELS_DERIVED_FROM,
                        ))
                    except Exception as e:  # per-stock graceful
                        errors += 1
                        logger.warning(f"[golden.fusion] levels {sid} 失敗: {e}")

            # ── resonance(per-stock × 3 tf)──────────────────────────────
            if "resonance" in want:
                for tf in P.RESONANCE_TIMEFRAMES:
                    if backfill and P.fusion_row_exists(
                        conn, stock_id=sid, timeframe=tf,
                        core_name=P.RESONANCE_CORE, snapshot_date=resolved,
                        source_version=P.RESONANCE_SOURCE_VERSION,
                    ):
                        skipped += 1
                        continue
                    try:
                        res = resonance(sid, resolved, timeframe=tf, conn=conn)
                        rows_rz.append(P.build_row(
                            stock_id=sid, snapshot_date=resolved,
                            timeframe=tf, core_name=P.RESONANCE_CORE,
                            source_version=P.RESONANCE_SOURCE_VERSION, params_hash=rz_hash,
                            snapshot=res.to_dict(), derived_from_core=P.RESONANCE_DERIVED_FROM,
                        ))
                    except Exception as e:  # per-stock graceful
                        errors += 1
                        logger.warning(f"[golden.fusion] resonance {sid}/{tf} 失敗: {e}")

            if len(rows_lv) >= _BATCH or len(rows_rz) >= _BATCH:
                _flush()

        _flush()
    finally:
        conn.close()

    elapsed_ms = int((time.monotonic() - start) * 1000)
    summary = {
        "as_of": resolved.isoformat() if isinstance(resolved, date) else None,
        "levels_written": levels_written,
        "resonance_written": resonance_written,
        "skipped": skipped,
        "errors": errors,
        "elapsed_ms": elapsed_ms,
        "warnings": warnings,
    }
    logger.info(f"[golden.fusion] done {summary}")
    return summary
