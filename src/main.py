"""
main.py
--------
tw-stock-collector CLI 進入點。

使用方式：
  python src/main.py backfill [--phases 1,2,3,4] [--stocks 2330,2317]
  python src/main.py incremental [--stocks 2330,2317]
  python src/main.py phase 3
  python src/main.py status
  python src/main.py validate

完整說明見 README.md 或執行 python src/main.py --help。

argparse 設計原則：
  全域選項（--config、--stock-list、--verbose）放在子命令前，影響 config 載入與日誌初始化。
  執行選項（--stocks、--dry-run）透過 parents= 注入 backfill / incremental / phase，
  放在子命令後面，符合使用者直覺：
    python src/main.py backfill --stocks 2330,2317 --phases 1,2,3
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
from db import create_writer
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
    """
    建立 argparse 解析器。

    全域選項（必須放在子命令前）：
      --config、--stock-list、--verbose

    執行子命令專屬選項（放在子命令後）：
      --stocks、--dry-run  → 透過 _exec_parent 注入 backfill / incremental / phase
      --phases             → 僅 backfill
      phase_num            → 僅 phase
    """
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

  # 搭配 verbose 和 dry-run
  python src/main.py --verbose backfill --stocks 2330 --dry-run

  # 日常增量更新
  python src/main.py incremental

  # 查看同步進度
  python src/main.py status

  # 驗證設定檔格式
  python src/main.py validate
        """,
    )

    # ── 真正的全域選項：影響 config 載入與日誌初始化，必須放在子命令前
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
        "--verbose",
        action="store_true",
        help="以 DEBUG 級別輸出日誌（放在子命令前）",
    )

    # ── 執行子命令共用選項（backfill / incremental / phase 共享）
    # add_help=False 避免與子命令自身的 -h 衝突
    _exec_parent = argparse.ArgumentParser(add_help=False)
    _exec_parent.add_argument(
        "--stocks",
        help="覆蓋股票清單，逗號分隔（開發用，如：2330,2317）",
    )
    _exec_parent.add_argument(
        "--dry-run",
        action="store_true",
        help="只印出計劃，不實際呼叫 API",
    )

    subparsers = parser.add_subparsers(dest="command", required=True)

    # ── backfill 子命令
    backfill_parser = subparsers.add_parser(
        "backfill",
        parents=[_exec_parent],
        help="全量歷史回補",
    )
    backfill_parser.add_argument(
        "--phases",
        help="指定要跑的 Phase，逗號分隔（如：1,2,3,4）",
    )

    # ── incremental 子命令
    incremental_parser = subparsers.add_parser(
        "incremental",
        parents=[_exec_parent],
        help="增量同步（日常排程用）",
    )
    incremental_parser.add_argument(
        "--phases",
        help="指定要跑的 Phase，逗號分隔（如：1,2,3,4）。未指定則跑 collector.toml 設定的全部",
    )

    # ── phase 子命令（只跑單一 Phase）
    phase_parser = subparsers.add_parser(
        "phase",
        parents=[_exec_parent],
        help="只跑指定 Phase",
    )
    phase_parser.add_argument(
        "phase_num",
        type=int,
        choices=range(0, 7),
        metavar="N",
        help="Phase 編號（0-6；Phase 0 為 trading_calendar 預載入）",
    )

    # ── status 子命令（不需要執行選項）
    subparsers.add_parser("status", help="顯示同步進度摘要")

    # ── validate 子命令（不需要執行選項）
    subparsers.add_parser("validate", help="驗證 config 格式")

    return parser


# =============================================================================
# 主程式
# =============================================================================

def main() -> None:
    """CLI 主函式：解析參數後分派至對應的執行函式"""
    parser = build_parser()
    args   = parser.parse_args()

    # ── 日誌初始化（config 載入前先用 INFO 啟動，避免 config 錯誤無日誌）
    # --verbose 是全域選項，在 args 中一定存在
    log_level = "DEBUG" if args.verbose else "INFO"

    # 載入設定
    try:
        config         = load_collector_config(args.config)
        stock_list_cfg = load_stock_list_config(args.stock_list)
    except (FileNotFoundError, ValueError) as e:
        # 設定載入失敗，logger 尚未初始化，直接輸出到 stderr
        print(f"[ERROR] Config 載入失敗：{e}", file=sys.stderr)
        sys.exit(1)

    # 正式初始化日誌（使用 config 中的 log_dir）
    setup_logger(config.global_cfg.log_dir, log_level)

    # validate 指令不需要 DB 或 API，直接回傳
    if args.command == "validate":
        cmd_validate(config, stock_list_cfg)
        return

    # status 指令
    if args.command == "status":
        cmd_status(config)
        return

    # 其他指令（backfill / incremental / phase）需要執行引擎
    asyncio.run(_run_collector(args, config, stock_list_cfg))


async def _run_collector(args, config, stock_list_cfg) -> None:
    """
    非同步執行主體：初始化所有元件後啟動 PhaseExecutor。

    只由 backfill / incremental / phase 子命令呼叫，
    args 一定含有 --stocks 與 --dry-run（來自 _exec_parent）。

    Args:
        args:           argparse 解析結果
        config:         CollectorConfig
        stock_list_cfg: StockListConfig
    """
    start_time = time.monotonic()
    command    = args.command

    # ── --phases 覆蓋（backfill / incremental 子命令）
    if command in ("backfill", "incremental") and getattr(args, "phases", None):
        try:
            config.execution.phases = [int(p.strip()) for p in args.phases.split(",")]
        except ValueError:
            logger.error(f"--phases 格式錯誤：{args.phases}（應為逗號分隔的整數，如 1,2,3）")
            sys.exit(1)

    # ── 單 Phase 模式
    if command == "phase":
        config.execution.phases = [args.phase_num]

    # ── --stocks 覆蓋（開發用）
    if args.stocks:
        stock_list_cfg.dev_enabled = True
        stock_list_cfg.static_ids  = [s.strip() for s in args.stocks.split(",")]
        logger.info(f"覆蓋股票清單（--stocks）：{stock_list_cfg.static_ids}")

    # ── 取得 FinMind Token（環境變數優先，其次 config）
    token = os.environ.get("FINMIND_TOKEN") or config.global_cfg.token
    if not token:
        logger.error(
            "找不到 FinMind Token。"
            "請設定環境變數 FINMIND_TOKEN 或在 collector.toml [global] 填入 token。"
        )
        sys.exit(1)

    # ── 初始化各元件
    db = create_writer()
    db.init_schema()

    rate_limiter = RateLimiter(
        calls_per_hour      = config.global_cfg.rate_limit.calls_per_hour,
        burst_size          = config.global_cfg.rate_limit.burst_size,
        min_interval_ms     = config.global_cfg.rate_limit.min_interval_ms,
        cooldown_on_429_sec = config.global_cfg.rate_limit.cooldown_on_429_sec,
    )

    sync_tracker = SyncTracker(db)

    # Rust Bridge（Phase 4）
    # database_url 沒傳 → RustBridge 自動讀 DATABASE_URL 環境變數
    # （對齊 Rust 端 #[arg(long, env = "DATABASE_URL")]）
    rust_bridge = RustBridge(config.global_cfg.rust_binary_path)

    async def rust_runner(mode: str, stock_ids: list[str] | None = None) -> None:
        """Phase 4 呼叫函式，傳入 PhaseExecutor"""
        await rust_bridge.run_phase4(mode=mode, stock_ids=stock_ids)

    # incremental 子命令對應 incremental 模式，其餘皆為 backfill
    mode = "incremental" if command == "incremental" else "backfill"

    try:
        async with FinMindClient(token, rate_limiter, config.global_cfg.retry) as client:
            executor = PhaseExecutor(
                config         = config,
                stock_list_cfg = stock_list_cfg,
                db             = db,
                client         = client,
                sync_tracker   = sync_tracker,
                rust_runner    = rust_runner,
                dry_run        = args.dry_run,
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
    print("✓ stock_list.toml 格式正確")
    print(f"  source.mode: {stock_list_cfg.source_mode}")
    print(f"  dev.enabled: {stock_list_cfg.dev_enabled}")
    if stock_list_cfg.static_ids:
        print(f"  靜態清單: {stock_list_cfg.static_ids}")


# =============================================================================
# 子命令：status
# =============================================================================

def cmd_status(config) -> None:
    """顯示 api_sync_progress 的狀態摘要"""
    db = create_writer()

    try:
# 檢查資料表是否存在（尚未執行 backfill 時為空）
        row = db.query_one(
            "SELECT table_name FROM information_schema.tables "
            "WHERE table_schema = 'public' AND table_name = 'api_sync_progress'"
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
            "completed":       "✓ 已完成",
            "empty":           "○ 空結果（正常）",
            "pending":         "- 待執行",
            "failed":          "✗ 失敗",
            "schema_mismatch": "⚠ Schema 不符",
        }

        for status, label in status_labels.items():
            count = summary.get(status, 0)
            print(f"  {label:<20} {count:>6} 筆")

        # 顯示最近的失敗詳細資訊
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
