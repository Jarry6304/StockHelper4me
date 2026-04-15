"""
main.py
--------
tw-stock-collector CLI 進入點。

使用方式：
  python src/main.py backfill [--phases 1,2,3,4] [--stocks 2330,2317]
  python src/main.py incremental
  python src/main.py status
  python src/main.py validate
  python src/main.py phase 3

完整說明見 README.md 或執行 python src/main.py --help。
"""

import argparse
import asyncio
import logging
import os
import sys
import time
from pathlib import Path

# ──────────────────────────────────────────────
# 將 src/ 加入 sys.path，確保各模組可直接 import
# ──────────────────────────────────────────────
sys.path.insert(0, str(Path(__file__).parent))

from api_client import FinMindClient
from config_loader import load_collector_config, load_stock_list_config
from db import DBWriter
from logger_setup import setup_logger
from phase_executor import PhaseExecutor
from rate_limiter import RateLimiter
from rust_bridge import RustBridge
from sync_tracker import SyncTracker

logger = logging.getLogger("collector.main")


# =============================================================================
# CLI 定義
# =============================================================================

def build_parser() -> argparse.ArgumentParser:
    """建立 argparse 解析器"""
    parser = argparse.ArgumentParser(
        prog="python src/main.py",
        description="tw-stock-collector：台股資料蒐集程式",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
範例：
  # 首次全量回補
  python src/main.py backfill

  # 只跑 Phase 1-4（取得可分析的基礎資料）
  python src/main.py backfill --phases 1,2,3,4

  # 只跑特定股票（開發測試用）
  python src/main.py backfill --stocks 2330,2317,2442

  # 日常增量更新
  python src/main.py incremental

  # 查看同步進度
  python src/main.py status

  # 驗證設定檔格式
  python src/main.py validate
        """,
    )

    # 全域選項
    parser.add_argument(
        "--config",
        default="config/collector.toml",
        help="collector.toml 路徑（預設：config/collector.toml）",
    )
    parser.add_argument(
        "--stock-list",
        default="config/stock_list.toml",
        help="stock_list.toml 路徑（預設：config/stock_list.toml）",
    )
    parser.add_argument(
        "--stocks",
        help="覆蓋股票清單，逗號分隔（開發用，如：2330,2317）",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="只印出計劃，不實際呼叫 API",
    )
    parser.add_argument(
        "--verbose",
        action="store_true",
        help="以 DEBUG 級別輸出日誌",
    )

    subparsers = parser.add_subparsers(dest="command", required=True)

    # ── backfill 子命令
    backfill_parser = subparsers.add_parser("backfill", help="全量歷史回補")
    backfill_parser.add_argument(
        "--phases",
        help="指定要跑的 Phase，逗號分隔（如：1,2,3,4）",
    )

    # ── incremental 子命令
    subparsers.add_parser("incremental", help="增量同步（日常排程用）")

    # ── phase 子命令（只跑單一 Phase）
    phase_parser = subparsers.add_parser("phase", help="只跑指定 Phase")
    phase_parser.add_argument("phase_num", type=int, help="Phase 編號（1-6）")

    # ── status 子命令
    subparsers.add_parser("status", help="顯示同步進度摘要")

    # ── validate 子命令
    subparsers.add_parser("validate", help="驗證 config 格式")

    return parser


# =============================================================================
# 主程式
# =============================================================================

def main() -> None:
    """CLI 主函式：解析參數後分派至對應的執行函式"""
    parser = build_parser()
    args   = parser.parse_args()

    # ── 初始化日誌（在 config 載入之前先啟用基本日誌）
    log_level = "DEBUG" if args.verbose else "INFO"

    # 載入設定
    try:
        config         = load_collector_config(args.config)
        stock_list_cfg = load_stock_list_config(args.stock_list)
    except (FileNotFoundError, ValueError) as e:
        # 設定載入失敗，直接輸出到 stderr（logger 尚未完全初始化）
        print(f"[ERROR] Config 載入失敗：{e}", file=sys.stderr)
        sys.exit(1)

    # 正式初始化日誌（使用 config 中的 log_dir）
    setup_logger(config.global_cfg.log_dir, log_level)

    # 若有 --verbose 覆蓋設定中的 log_level
    if args.verbose:
        config.global_cfg.log_level = "DEBUG"

    # validate 指令不需要 DB 或 API，直接回傳
    if args.command == "validate":
        cmd_validate(config, stock_list_cfg)
        return

    # status 指令
    if args.command == "status":
        cmd_status(config)
        return

    # 其他指令需要執行引擎
    asyncio.run(_run_collector(args, config, stock_list_cfg))


async def _run_collector(args, config, stock_list_cfg) -> None:
    """
    非同步執行主體：初始化所有元件後啟動 PhaseExecutor。

    Args:
        args:           argparse 解析結果
        config:         CollectorConfig
        stock_list_cfg: StockListConfig
    """
    start_time = time.monotonic()
    command    = args.command

    # 處理 --phases 覆蓋
    if command == "backfill" and getattr(args, "phases", None):
        try:
            config.execution.phases = [int(p) for p in args.phases.split(",")]
        except ValueError:
            logger.error(f"--phases 格式錯誤：{args.phases}（應為逗號分隔的整數）")
            sys.exit(1)

    if command == "phase":
        config.execution.phases = [args.phase_num]

    # 處理 --stocks 覆蓋（開發用）
    if args.stocks:
        stock_list_cfg.dev_enabled = True
        stock_list_cfg.static_ids  = [s.strip() for s in args.stocks.split(",")]
        logger.info(f"覆蓋股票清單（--stocks）：{stock_list_cfg.static_ids}")

    # 取得 FinMind Token（環境變數優先，其次 config）
    token = os.environ.get("FINMIND_TOKEN") or config.global_cfg.token
    if not token and command not in ("status", "validate"):
        logger.error(
            "找不到 FinMind Token。"
            "請設定環境變數 FINMIND_TOKEN 或在 collector.toml [global] 填入 token。"
        )
        sys.exit(1)

    # 初始化各元件
    db = DBWriter(config.global_cfg.db_path)
    db.init_schema()

    rate_limiter = RateLimiter(
        calls_per_hour   = config.global_cfg.rate_limit.calls_per_hour,
        burst_size       = config.global_cfg.rate_limit.burst_size,
        min_interval_ms  = config.global_cfg.rate_limit.min_interval_ms,
    )

    sync_tracker = SyncTracker(db)

    # Rust Bridge（Phase 4）
    rust_bridge  = RustBridge(config.global_cfg.rust_binary_path, config.global_cfg.db_path)

    async def rust_runner(mode: str):
        """Phase 4 呼叫函式，傳入 PhaseExecutor"""
        await rust_bridge.run_phase4(mode=mode)

    # 執行模式
    mode = "incremental" if command == "incremental" else "backfill"

    try:
        async with FinMindClient(token, rate_limiter, config.global_cfg.retry) as client:
            executor = PhaseExecutor(
                config          = config,
                stock_list_cfg  = stock_list_cfg,
                db              = db,
                client          = client,
                sync_tracker    = sync_tracker,
                rust_runner     = rust_runner,
                dry_run         = args.dry_run,
            )

            logger.info(
                f"Collector started. command={command}, phases={config.execution.phases}"
            )
            await executor.run(mode)

    except KeyboardInterrupt:
        logger.warning("使用者中斷執行（Ctrl+C）")
    except Exception as e:
        logger.error(f"Collector aborted. reason={e}")
        raise
    finally:
        db.close()

    elapsed = int(time.monotonic() - start_time)
    logger.info(f"Collector finished. elapsed={elapsed}s")


# =============================================================================
# 子命令：validate
# =============================================================================

def cmd_validate(config, stock_list_cfg) -> None:
    """
    驗證 config 格式是否正確。
    config_loader.load_collector_config() 本身已執行驗證，
    到達此處表示驗證通過。
    """
    print("✓ collector.toml 格式正確")
    print(f"  APIs: {len(config.apis)} 個")
    print(f"  啟用 APIs: {sum(1 for a in config.apis if a.enabled)} 個")
    print(f"  執行模式: {config.execution.mode}")
    print(f"  phases: {config.execution.phases}")
    print()
    print(f"✓ stock_list.toml 格式正確")
    print(f"  source.mode: {stock_list_cfg.source_mode}")
    print(f"  dev.enabled: {stock_list_cfg.dev_enabled}")
    if stock_list_cfg.static_ids:
        print(f"  靜態清單: {stock_list_cfg.static_ids}")


# =============================================================================
# 子命令：status
# =============================================================================

def cmd_status(config) -> None:
    """顯示 api_sync_progress 的狀態摘要"""
    db = DBWriter(config.global_cfg.db_path)

    try:
        # 檢查資料表是否存在
        row = db.query_one(
            "SELECT name FROM sqlite_master WHERE type='table' AND name='api_sync_progress'"
        )
        if row is None:
            print("資料庫尚未初始化，請先執行 backfill。")
            return

        tracker = SyncTracker(db)
        summary = tracker.summary()

        total = sum(summary.values())
        print(f"\n=== tw-stock-collector 同步進度 ===")
        print(f"總計 {total} 個 segment\n")

        status_labels = {
            "completed":      "✓ 已完成",
            "empty":          "○ 空結果（正常）",
            "pending":        "- 待執行",
            "failed":         "✗ 失敗",
            "schema_mismatch": "⚠ Schema 不符",
        }

        for status, label in status_labels.items():
            count = summary.get(status, 0)
            print(f"  {label:<20} {count:>6} 筆")

        # 顯示失敗的詳細資訊
        if summary.get("failed", 0) > 0:
            print("\n最近 10 筆失敗記錄：")
            rows = db.query(
                """
                SELECT api_name, stock_id, segment_start, error_message
                FROM api_sync_progress
                WHERE status = 'failed'
                ORDER BY updated_at DESC
                LIMIT 10
                """
            )
            for r in rows:
                print(f"  {r['api_name']} / {r['stock_id']} / {r['segment_start']}")
                print(f"    → {r['error_message']}")

        print()

    finally:
        db.close()


# =============================================================================
# 程式進入點
# =============================================================================

if __name__ == "__main__":
    main()
