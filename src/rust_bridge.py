"""
rust_bridge.py
---------------
Python → Rust Binary 橋接模組（Phase 4）。

Phase 4 由 Python 呼叫 Rust binary 執行：
  1.   補算 capital_increase 事件的 adjustment_factor
  1.5. 後復權計算：price_daily → price_daily_fwd
  2.   週K聚合：price_weekly_fwd
  3.   月K聚合：price_monthly_fwd
  4.   更新 stock_sync_status.fwd_adj_valid = 1

Python 與 Rust 透過 CLI 參數傳遞設定，
資料交換直接讀寫同一個 SQLite 檔案。

Signal Handling（v1.1）：
  攔截 CancelledError（Ctrl+C 觸發），先 SIGTERM 再 wait(10s)，
  確保 Rust 端可 commit/rollback 當前 transaction，再 SIGKILL。
"""

import asyncio
import json
import logging
import sys
from pathlib import Path

logger = logging.getLogger("collector.rust_bridge")

# 與 Rust binary 約定的 schema 版本號
EXPECTED_SCHEMA_VERSION = "1.1"


class RustComputeError(Exception):
    """Rust binary 執行失敗時拋出"""
    pass


class RustBridge:
    """
    Rust binary 呼叫器。

    使用方式：
        bridge = RustBridge(binary_path, db_path)
        result = await bridge.run_phase4(mode="backfill")
    """

    def __init__(self, binary_path: str, db_path: str):
        """
        Args:
            binary_path: Rust binary 路徑
                         預設：rust_compute/target/release/tw_stock_compute
                         Windows 上若實體檔案是 .exe 而 toml 沒寫副檔名，會自動補上
            db_path:     SQLite 資料庫路徑（Rust 直接讀寫此檔案）
        """
        # Windows: cargo build 產出 tw_stock_compute.exe，
        # 但 collector.toml 為跨平台寫成不含副檔名 → 自動補
        if sys.platform == "win32" and not binary_path.lower().endswith(".exe"):
            exe_path = binary_path + ".exe"
            if Path(exe_path).exists():
                binary_path = exe_path
        self.binary = binary_path
        self.db     = db_path

    async def run_phase4(
        self,
        stock_ids: list[str] | None = None,
        mode: str = "backfill",
    ) -> dict:
        """
        呼叫 Rust binary 執行後復權 + 週K/月K 聚合。

        Args:
            stock_ids: 指定要處理的股票代碼清單；
                       None 表示由 Rust 自行從 stock_sync_status 取待計算清單
            mode:      "backfill" | "incremental"

        Returns:
            Rust binary 輸出的 JSON 摘要，格式：
            {
                "schema_version": "1.1",
                "processed": 1800,
                "skipped": 12,
                "errors": [{"stock_id": "XXXX", "reason": "..."}],
                "af_patched": 3,
                "elapsed_ms": 45000
            }

        Raises:
            RustComputeError: Rust binary 執行失敗
            FileNotFoundError: binary_path 不存在
        """
        # 組裝 CLI 指令
        cmd = [
            self.binary,
            "--db",   self.db,
            "--mode", mode,
        ]

        # 若有指定股票清單，以逗號分隔傳入
        if stock_ids:
            cmd.extend(["--stocks", ",".join(stock_ids)])

        stocks_desc = f"stocks={','.join(stock_ids)}" if stock_ids else "stocks=all"
        logger.info(f"[Phase 4] Rust binary started. mode={mode}, {stocks_desc}")

        # 啟動子進程
        process = await asyncio.create_subprocess_exec(
            *cmd,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )

        stdout_data: bytes = b""
        stderr_data: bytes = b""

        try:
            # 等待執行完畢，收集輸出
            stdout_data, stderr_data = await process.communicate()

        except asyncio.CancelledError:
            # ── Signal Handling（v1.1）──
            # Ctrl+C 或外部取消：先送 SIGTERM 讓 Rust 優雅結束
            logger.warning("Phase 4 cancelled, sending SIGTERM to Rust binary...")
            process.terminate()

            try:
                # 等待最多 10 秒讓 Rust 完成當前 transaction
                await asyncio.wait_for(process.wait(), timeout=10)
            except asyncio.TimeoutError:
                # 超時還沒結束 → 強制 SIGKILL
                logger.error("Rust binary did not exit in 10s, sending SIGKILL")
                process.kill()

            raise  # 重新拋出 CancelledError，讓上層處理

        # 檢查 return code
        if process.returncode != 0:
            error_msg = stderr_data.decode(errors="replace")
            logger.error(f"[Phase 4] Rust binary failed. returncode={process.returncode}, stderr={error_msg}")
            raise RustComputeError(
                f"Phase 4 failed (returncode={process.returncode}): {error_msg}"
            )

        # 解析 stdout（JSON 格式摘要）
        stdout_str = stdout_data.decode(errors="replace").strip()
        if not stdout_str:
            logger.warning("[Phase 4] Rust binary 輸出為空，可能未處理任何資料")
            return {}

        try:
            result = json.loads(stdout_str)
        except json.JSONDecodeError as e:
            logger.error(f"[Phase 4] 無法解析 Rust binary 輸出：{e}\n輸出內容：{stdout_str}")
            raise RustComputeError(f"無法解析 Rust binary JSON 輸出：{e}") from e

        # ── Schema Version 驗證（v1.1）──
        rust_schema = result.get("schema_version")
        if rust_schema and rust_schema != EXPECTED_SCHEMA_VERSION:
            logger.warning(
                f"Rust binary schema_version={rust_schema}, "
                f"expected={EXPECTED_SCHEMA_VERSION}. "
                f"Consider rebuilding: cargo build --release"
            )

        # 記錄執行摘要
        processed  = result.get("processed", 0)
        skipped    = result.get("skipped", 0)
        af_patched = result.get("af_patched", 0)
        elapsed    = result.get("elapsed_ms", 0)
        errors     = result.get("errors", [])

        logger.info(
            f"[Phase 4] Rust binary finished. "
            f"processed={processed}, skipped={skipped}, "
            f"af_patched={af_patched}, elapsed={elapsed}ms"
        )

        if errors:
            logger.warning(f"[Phase 4] Errors: {errors}")

        return result
