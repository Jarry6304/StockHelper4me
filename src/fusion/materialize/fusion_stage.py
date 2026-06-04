"""Golden L3 — levels_fusion + resonance_fusion 物化階段。

把 read-time `key_levels()`(per-stock)+ `resonance()`(per-stock × 3 tf)的輸出
write-time 呼叫 + UPSERT 進 `structural_snapshots`(新 core_name)。

設計(對齊 m3Spec/build-pipeline.md):
- **不是 CrossStockBuilder**(那些寫 *_ranked_derived 欄式表);fusion 寫 JSONB 文件,自成 stage。
- 讀用 `get_connection()`(dict_row autocommit)conn 傳給 key_levels/resonance(零改寫)。
- 寫用 `DBWriter.upsert("structural_snapshots", rows, PK)`(已處理 ON CONFLICT + dict→Jsonb)。
- daily = always-recompute-latest(不 skip);backfill = skip-if-exists(靠 fusion row 自身 version)。
- per-stock graceful:單股失敗不中斷整個 universe(對齊 cross_cores orchestrator)。

v4.36 並行:
  原 universe loop 共用單一 conn 串列跑(2171 stocks × 4 ops 對齊 ~30 min)。
  改 ThreadPoolExecutor + 每 worker 自開 get_connection() — psycopg conn 非
  thread-safe,**不能**直接共用。並行度從 env `DB_POOL_SIZE - 1`(對齊 PostgresWriter
  pool size,留 1 conn 給 query 路徑)。`parallelism=1` 退回單緒原行為(test
  fixture / MagicMock 友善)。db.upsert 走 PostgresWriter pool,本來就 thread-safe。
"""

from __future__ import annotations

import logging
import os
import time
from concurrent.futures import ThreadPoolExecutor, as_completed
from datetime import date
from typing import Any

from fusion.dual_track.resonance import resonance
from fusion.key_levels import key_levels
from fusion.materialize import _provenance as P
from fusion.raw._db import get_connection

logger = logging.getLogger("collector.fusion.materialize")

_BATCH = 500  # flush 批量(控記憶體)


def _default_parallelism() -> int:
    """並行 worker 上限 = max(1, DB_POOL_SIZE - 1)。留 1 conn 給 query 路徑。"""
    try:
        return max(1, int(os.getenv("DB_POOL_SIZE", "8")) - 1)
    except ValueError:
        return 7


def run_fusion_materialize(
    db: Any,
    *,
    as_of: date | None = None,
    stocks: list[str] | None = None,
    only: set[str] | None = None,
    backfill: bool = False,
    database_url: str | None = None,
    parallelism: int | None = None,
) -> dict[str, Any]:
    """物化 levels_fusion + resonance_fusion。

    Args:
        db:          DBWriter(寫 structural_snapshots)。
        as_of:       物化日;None = price_daily 最新交易日。
        stocks:      限縮 universe;None = price_daily_fwd 全市場。
        only:        {"levels", "resonance"} 子集;None = 兩者都做。
        backfill:    True = skip-if-exists(回填歷史用);False(daily)= always-recompute。
        database_url: 讀連線(預設 env / .env)。
        parallelism: per-stock worker 並行度;None = 走 env `DB_POOL_SIZE - 1`(預設 7);
                     1 = 退回單緒原行為(對 test fixture / 不開 pool 環境友善)。

    Returns:
        {as_of, levels_written, resonance_written, skipped, errors, elapsed_ms, warnings}
    """
    start = time.monotonic()
    want = only or {"levels", "resonance"}

    # ── Phase 1:metadata 讀(single conn)── universe / latest date / warnings
    meta_conn = get_connection(database_url)
    try:
        resolved = as_of or P.latest_trading_date(meta_conn)
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

        warnings: list[str] = []
        # resonance track2 stale 警告(對齊 plan 風險:forecast 非全市場時 track2 多 single_track)
        if "resonance" in want:
            lag = P.forecast_log_lag_days(meta_conn, resolved)
            if lag is None:
                warnings.append(
                    "forecast_log 無 external row → resonance track2 全市場缺 band(全 single_track)。"
                    "需先跑 forecast 全市場校準(Phase 3b)。"
                )
            elif lag > 7:
                warnings.append(
                    f"forecast_log 最新 forecast_date 落後 as_of {lag} 天 → resonance track2 多數 stale。"
                )

        universe = P.fetch_universe(meta_conn, stocks)
    finally:
        meta_conn.close()

    n_workers = max(1, min(parallelism or _default_parallelism(), max(1, len(universe))))
    logger.info(
        f"[golden.fusion] as_of={resolved} universe={len(universe)} "
        f"only={sorted(want)} backfill={backfill} parallelism={n_workers}"
    )

    lv_hash = P.levels_params_hash()
    rz_hash = P.resonance_params_hash()

    # ── Phase 2:per-stock 並行 compute(每 worker own conn)───────────────
    def _process_stock(sid: str) -> tuple[list[dict[str, Any]], list[dict[str, Any]], int, int]:
        """回 (lv_rows, rz_rows, skipped_inc, errors_inc)。worker-local,no race。"""
        lv_local: list[dict[str, Any]] = []
        rz_local: list[dict[str, Any]] = []
        sk = 0
        er = 0
        wconn = get_connection(database_url)
        try:
            if "levels" in want:
                if backfill and P.fusion_row_exists(
                    wconn, stock_id=sid, timeframe=P.LEVELS_TIMEFRAME,
                    core_name=P.LEVELS_CORE, snapshot_date=resolved,
                    source_version=P.LEVELS_SOURCE_VERSION,
                ):
                    sk += 1
                else:
                    try:
                        doc = key_levels(sid, resolved, conn=wconn)
                        lv_local.append(P.build_row(
                            stock_id=sid, snapshot_date=resolved,
                            timeframe=P.LEVELS_TIMEFRAME, core_name=P.LEVELS_CORE,
                            source_version=P.LEVELS_SOURCE_VERSION, params_hash=lv_hash,
                            snapshot=doc, derived_from_core=P.LEVELS_DERIVED_FROM,
                        ))
                    except Exception as e:
                        er += 1
                        logger.warning(f"[golden.fusion] levels {sid} 失敗: {e}")

            if "resonance" in want:
                for tf in P.RESONANCE_TIMEFRAMES:
                    if backfill and P.fusion_row_exists(
                        wconn, stock_id=sid, timeframe=tf,
                        core_name=P.RESONANCE_CORE, snapshot_date=resolved,
                        source_version=P.RESONANCE_SOURCE_VERSION,
                    ):
                        sk += 1
                        continue
                    try:
                        res = resonance(sid, resolved, timeframe=tf, conn=wconn)
                        rz_local.append(P.build_row(
                            stock_id=sid, snapshot_date=resolved,
                            timeframe=tf, core_name=P.RESONANCE_CORE,
                            source_version=P.RESONANCE_SOURCE_VERSION, params_hash=rz_hash,
                            snapshot=res.to_dict(), derived_from_core=P.RESONANCE_DERIVED_FROM,
                        ))
                    except Exception as e:
                        er += 1
                        logger.warning(f"[golden.fusion] resonance {sid}/{tf} 失敗: {e}")
        finally:
            wconn.close()
        return lv_local, rz_local, sk, er

    # ── Phase 3:aggregate + streaming flush ─────────────────────────────
    rows_lv: list[dict[str, Any]] = []
    rows_rz: list[dict[str, Any]] = []
    levels_written = 0
    resonance_written = 0
    skipped = 0
    errors = 0

    def _flush_if_full() -> None:
        nonlocal rows_lv, rows_rz, levels_written, resonance_written
        if len(rows_lv) >= _BATCH:
            levels_written += db.upsert("structural_snapshots", rows_lv, P.PK_COLS)
            rows_lv = []
        if len(rows_rz) >= _BATCH:
            resonance_written += db.upsert("structural_snapshots", rows_rz, P.PK_COLS)
            rows_rz = []

    if not universe:
        # 空 universe — skip ThreadPoolExecutor
        pass
    elif n_workers == 1:
        # 單緒路徑:test fixture / 不開 pool 場景。對齊原行為,避免 ThreadPool overhead。
        for sid in universe:
            try:
                lv_local, rz_local, sk, er = _process_stock(sid)
            except Exception as e:
                errors += 1
                logger.warning(f"[golden.fusion] worker {sid} crashed: {e}")
                continue
            rows_lv.extend(lv_local)
            rows_rz.extend(rz_local)
            skipped += sk
            errors += er
            _flush_if_full()
    else:
        with ThreadPoolExecutor(max_workers=n_workers,
                                thread_name_prefix="golden-fusion") as ex:
            future_to_sid = {ex.submit(_process_stock, sid): sid for sid in universe}
            for fut in as_completed(future_to_sid):
                sid = future_to_sid[fut]
                try:
                    lv_local, rz_local, sk, er = fut.result()
                except Exception as e:
                    errors += 1
                    logger.warning(f"[golden.fusion] worker {sid} crashed: {e}")
                    continue
                rows_lv.extend(lv_local)
                rows_rz.extend(rz_local)
                skipped += sk
                errors += er
                _flush_if_full()

    # 最終 flush — lv 先 rz 後(保留 v4.35 前測試對 batch order 的預期)
    if rows_lv:
        levels_written += db.upsert("structural_snapshots", rows_lv, P.PK_COLS)
    if rows_rz:
        resonance_written += db.upsert("structural_snapshots", rows_rz, P.PK_COLS)

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
