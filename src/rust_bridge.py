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

Python 與 Rust 透過 CLI 參數傳遞 PG 連線字串，
Rust 用 sqlx 直接連 Postgres（v2.0 起改 PG，先前是 SQLite）。

Signal Handling（v1.1）：
  攔截 CancelledError（Ctrl+C 觸發），先 SIGTERM 再 wait(10s)，
  確保 Rust 端可 commit/rollback 當前 transaction，再 SIGKILL。
"""

import asyncio
import json
import logging
import os
import sys
from pathlib import Path

logger = logging.getLogger("collector.rust_bridge")

# 與 Rust binary 約定的 schema 版本號（v2.0 = PG/sqlx 版）
EXPECTED_SCHEMA_VERSION = "2.0"


class RustComputeError(Exception):
    """Rust binary 執行失敗時拋出"""
    pass


class RustBridge:
    """
    Rust binary 呼叫器。

    使用方式：
        bridge = RustBridge(binary_path)  # database_url 從 DATABASE_URL 環境變數讀
        result = await bridge.run_phase4(mode="backfill")
    """

    def __init__(self, binary_path: str, database_url: str | None = None):
        """
        Args:
            binary_path:  Rust binary 路徑
                          預設：rust_compute/target/release/tw_stock_compute
                          Windows 上若實體檔案是 .exe 而 toml 沒寫副檔名，會自動補上
            database_url: Postgres 連線字串。None 時走環境變數 DATABASE_URL，
                          這也對齊 Rust 端 #[arg(long, env = "DATABASE_URL")]。
        """
        # Windows: cargo build 產出 tw_stock_compute.exe，
        # 但 collector.toml 為跨平台寫成不含副檔名 → 自動補
        if sys.platform == "win32" and not binary_path.lower().endswith(".exe"):
            exe_path = binary_path + ".exe"
            if Path(exe_path).exists():
                binary_path = exe_path
        self.binary       = binary_path
        # 容錯:.env 可能寫成 `DATABASE_URL=  ` 留下純空白,
        # 不 strip 的話會送進 Rust 端才報錯,使用者看到的是 sqlx 的 obscure 錯訊。
        raw_url = database_url if database_url is not None else os.getenv("DATABASE_URL", "")
        self.database_url = raw_url.strip()
        if not self.database_url:
            raise RuntimeError(
                "RustBridge: 找不到 Postgres 連線字串。請設定 DATABASE_URL 環境變數，"
                "或在初始化時傳入 database_url 參數。"
            )

        # Binary 存在性與新鮮度檢查(dev 階段救命用)
        # production 部署 binary 跟 source 通常不在同一台機器,
        # source 不存在時 silently 跳過,不打擾。
        self._check_binary_freshness()

    def _check_binary_freshness(self) -> None:
        """
        Binary 健全性檢查:
          1. binary 不存在 → raise FileNotFoundError(早於 subprocess 啟動)
          2. main.rs 比 binary 新 → 警告但不 raise(dev 場景常見)
        Production 場景下 main.rs 不會跟 binary 在同一台機器,
        source 找不到時 silently 跳過,不洗版。
        """
        binary_path = Path(self.binary)
        if not binary_path.exists():
            raise FileNotFoundError(
                f"Rust binary 不存在:{self.binary}。"
                f"請先執行:cd rust_compute && cargo build --release"
            )

        # mtime 警告(dev 階段救命)
        try:
            project_root = Path(__file__).resolve().parent.parent
            main_rs = project_root / "rust_compute" / "src" / "main.rs"
            cargo_toml = project_root / "rust_compute" / "Cargo.toml"
            if not main_rs.exists():
                return  # production:source 不在同台機器,不打擾

            binary_mtime = binary_path.stat().st_mtime
            stale_sources = [
                p for p in (main_rs, cargo_toml)
                if p.exists() and p.stat().st_mtime > binary_mtime
            ]
            if stale_sources:
                names = ", ".join(p.name for p in stale_sources)
                logger.warning(
                    f"Rust binary 比 source 舊 ({names} 較新)。"
                    f"如剛改過 Rust code,請執行:cd rust_compute && cargo build --release"
                )
        except OSError as e:
            # stat 失敗(權限 / 競態檔案被刪)→ debug log 帶過,不影響執行
            logger.debug(f"Rust binary 新鮮度檢查 stat 失敗:{e}")

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
                "schema_version": "2.0",
                "processed": 1800,
                "skipped": 12,
                "errors": [{"stock_id": "XXXX", "reason": "..."}],
                "af_patched": 3,
                "interrupted": false,
                "elapsed_ms": 45000
            }

        Raises:
            RustComputeError: Rust binary 執行失敗
            FileNotFoundError: binary_path 不存在
        """
        # 組裝 CLI 指令
        # 注意：Rust binary 的 CLI 是 --database-url（不是 --db）
        # 對應 main.rs: #[arg(long, env = "DATABASE_URL")] database_url: String
        cmd = [
            self.binary,
            "--database-url", self.database_url,
            "--mode",         mode,
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

        # ── Schema Version 驗證(v1.2:warn → hard fail)──
        # 三層防線:
        #   1. 缺欄位 → 舊 binary(SQLite 版根本不輸出 schema_version)→ raise
        #   2. 不一致 → Python / Rust / DB 三者版本錯位 → raise
        #   3. 一致   → pass
        # 之前是 logger.warning,在 production log 會被淹沒,Phase 4
        # silently 跑完但 price_daily_fwd 全空(此即 commit 0ba3b5f 的症狀)。
        # 此處 hard fail 為了避免下次 schema 演進時再踩同一個坑。
        rust_schema = result.get("schema_version")
        if not rust_schema:
            raise RustComputeError(
                f"Rust binary 輸出缺 schema_version 欄位,可能是舊版 binary。"
                f"請執行 `cargo build --release` 重新編譯 "
                f"rust_compute/target/release/tw_stock_compute。"
            )
        if rust_schema != EXPECTED_SCHEMA_VERSION:
            raise RustComputeError(
                f"Rust binary schema_version={rust_schema}, "
                f"expected={EXPECTED_SCHEMA_VERSION}。"
                f"請確認:(1) `cargo build --release` 已重編,"
                f"(2) Python 端 EXPECTED_SCHEMA_VERSION 已同步升級,"
                f"(3) DB 已執行 `alembic upgrade head`。"
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
