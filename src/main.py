"""
main.py
--------
tw-stock-collector CLI 進入點。

使用方式：
  python src/main.py backfill [--phases 1,2,3,4] [--stocks 2330,2317]
  python src/main.py incremental [--stocks 2330,2317]
  python src/main.py phase 3
  python src/main.py silver phase 7a [--stocks 2330] [--full-rebuild]
  python src/main.py refresh                       # 一鍵串完 Bronze→Silver→M3 cores
  python src/main.py refresh --skip-cores          # 只到 Silver(無 Rust binary 時)
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
from bronze.phase_executor import PhaseExecutor
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

    # ── silver 子命令(Phase 7 dirty-driven Silver 計算層,blueprint v3.2)
    silver_parser = subparsers.add_parser(
        "silver",
        help="Silver 層計算(Phase 7a/7b/7c)",
    )
    silver_subparsers = silver_parser.add_subparsers(dest="silver_command", required=True)

    silver_phase_parser = silver_subparsers.add_parser(
        "phase",
        help="跑指定 Silver phase(7a / 7b / 7c)",
    )
    silver_phase_parser.add_argument(
        "phase_name",
        choices=["7a", "7b", "7c"],
        metavar="PHASE",
        help="Silver phase:7a(獨立 builder)/ 7b(跨表依賴)/ 7c(Rust 後復權)",
    )
    silver_phase_parser.add_argument(
        "--stocks",
        help="覆蓋股票清單,逗號分隔(market-level builder 一律忽略)",
    )
    silver_phase_parser.add_argument(
        "--full-rebuild",
        action="store_true",
        help="忽略 dirty queue,全表重算(目前唯一支援的模式)",
    )

    # ── cross_cores 子命令(Phase 8 跨股 Cross-Stock Cores,v3.5 R3 新層)
    cross_parser = subparsers.add_parser(
        "cross_cores",
        help="Cross-Stock Cores 計算(Phase 8;magic_formula 等跨股 ranking)",
    )
    cross_subparsers = cross_parser.add_subparsers(dest="cross_command", required=True)

    cross_phase_parser = cross_subparsers.add_parser(
        "phase",
        help="跑指定 cross-stock phase(目前只有 8)",
    )
    cross_phase_parser.add_argument(
        "phase_name",
        choices=["8"],
        metavar="PHASE",
        help="Cross-Stock phase:8(全跑所有 cross-stock cores)",
    )
    cross_phase_parser.add_argument(
        "--builder",
        help="只跑指定 cross-stock builder(預設全跑,逗號分隔)",
    )
    cross_phase_parser.add_argument(
        "--full-rebuild",
        action="store_true",
        help="重算 lookback window 全部 dates(預設只算 latest 1 day)",
    )
    cross_phase_parser.add_argument(
        "--lookback-days",
        type=int,
        help="full_rebuild 時往回幾天(預設由 builder 決定,magic_formula = 30)",
    )

    # ── refresh 子命令(一鍵手動更新最新資料,以防沒 scheduler)
    refresh_parser = subparsers.add_parser(
        "refresh",
        help="一鍵更新最新:incremental → silver 7c/7a/7b → tw_cores run-all --dirty",
    )
    refresh_parser.add_argument(
        "--stocks",
        help="覆蓋股票清單(逗號分隔);不指定 = 全市場",
    )
    refresh_parser.add_argument(
        "--skip-cores",
        action="store_true",
        help="跳過 M3 cores 階段(只跑 Bronze + Silver)",
    )
    refresh_parser.add_argument(
        "--skip-bronze",
        action="store_true",
        help="跳過 Bronze incremental(只跑 Silver + Cores,若已單獨跑過 incremental)",
    )

    # ── forecast 子命令(區間預測 spine,v0.3 spec)
    forecast_parser = subparsers.add_parser(
        "forecast",
        help="區間預測 spine:backtest / settle / score / manual",
    )
    forecast_subparsers = forecast_parser.add_subparsers(
        dest="forecast_command", required=True
    )

    # forecast backtest --core baseline --stocks 2330 --since 2020-01-01
    f_backtest = forecast_subparsers.add_parser(
        "backtest",
        help="跑指定 forecast core 的因果回測,寫進 forecast_log",
    )
    f_backtest.add_argument(
        "--core",
        default="baseline",
        choices=["baseline"],  # phase 1 only; kalman/log_channel 後續加
        help="forecast core 名(目前只有 baseline)",
    )
    f_backtest.add_argument(
        "--stocks",
        required=True,
        help="股票清單(逗號分隔,如 2330,2317);MVP 一次跑一檔較穩",
    )
    f_backtest.add_argument(
        "--since",
        required=True,
        help="起算 forecast_date(YYYY-MM-DD)",
    )
    f_backtest.add_argument(
        "--until",
        help="結束 forecast_date(YYYY-MM-DD;預設 today)",
    )
    f_backtest.add_argument(
        "--horizons",
        default="21,63,126",
        help="逗號分隔 horizon 天數(預設 21,63,126)",
    )
    f_backtest.add_argument(
        "--confidences",
        default="0.50,0.80,0.95",
        help="逗號分隔 confidence(預設 0.50,0.80,0.95)",
    )

    # forecast settle [--asof TODAY] [--core baseline]
    f_settle = forecast_subparsers.add_parser(
        "settle",
        help="結算所有已到期(forecast_date + horizon ≤ asof)的 forecast_log row",
    )
    f_settle.add_argument(
        "--asof",
        help="結算 asof 日期(YYYY-MM-DD;預設 today)",
    )
    f_settle.add_argument(
        "--core",
        help="只結算指定 source_core(預設全部)",
    )
    f_settle.add_argument(
        "--stocks",
        help="只結算指定 stock_id(逗號分隔;預設全部)",
    )

    # forecast conformalize --stocks 2330 --since 2022-01-01 --until TODAY
    f_conf = forecast_subparsers.add_parser(
        "conformalize",
        help="CQR 校準 raw forecasts(寫 source_core='kalman_cqr' calibrated=True)",
    )
    f_conf.add_argument(
        "--raw-core",
        default="kalman_forecast_core",
        help="raw forecast core 名(預設 kalman_forecast_core)",
    )
    f_conf.add_argument(
        "--target-core",
        default="kalman_cqr",
        help="輸出 source_core(預設 kalman_cqr)",
    )
    f_conf.add_argument(
        "--stocks",
        required=True,
        help="股票清單(逗號分隔,如 2330,2317)",
    )
    f_conf.add_argument(
        "--since",
        required=True,
        help="起算 asof_t(YYYY-MM-DD)",
    )
    f_conf.add_argument(
        "--until",
        help="結束 asof_t(YYYY-MM-DD;預設 today)",
    )
    f_conf.add_argument(
        "--horizons",
        default="21,63,126",
        help="逗號分隔 horizon 天數",
    )
    f_conf.add_argument(
        "--confidences",
        default="0.50,0.80,0.95",
        help="逗號分隔 confidence",
    )
    f_conf.add_argument(
        "--calibration-window",
        type=int,
        default=500,
        help="校準集大小(最近 N 個已結算 row;預設 500)",
    )
    f_conf.add_argument(
        "--min-calibration-size",
        type=int,
        default=30,
        help="最少校準集 size(< 此值不寫 row;預設 30)",
    )

    # forecast score [--core baseline] [--horizon 63] [--group-by source_core]
    f_score = forecast_subparsers.add_parser(
        "score",
        help="對已結算 row 算 pinball / sharpness / reliability",
    )
    f_score.add_argument("--core", help="只看指定 source_core")
    f_score.add_argument("--horizon", type=int, help="只看指定 horizon")
    f_score.add_argument("--stock", help="只看指定 stock_id")
    f_score.add_argument(
        "--since",
        help="只看 forecast_date >= 指定日(YYYY-MM-DD)",
    )
    f_score.add_argument(
        "--group-by",
        choices=["source_core", "horizon_days", "regime_tag"],
        help="分組統計(預設不分組)",
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

    # silver 指令(Phase 7 dirty-driven 計算層)
    if args.command == "silver":
        asyncio.run(_run_silver(args, config))
        return

    # cross_cores 指令(Phase 8 跨股 Cross-Stock Cores,v3.5 R3 新層)
    if args.command == "cross_cores":
        asyncio.run(_run_cross_cores(args, config))
        return

    # refresh 指令(一鍵手動更新最新資料)
    if args.command == "refresh":
        asyncio.run(_run_refresh(args, config, stock_list_cfg))
        return

    # forecast 指令(區間預測 spine,v0.3 spec)
    if args.command == "forecast":
        _run_forecast(args)
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
# 子命令:silver phase(Phase 7 dirty-driven Silver 計算層)
# =============================================================================

async def _run_silver(args, config) -> None:
    """跑 Silver Phase 7(7a / 7b / 7c)。

    7a / 7b 走 silver/builders/<name>.run(db, ...);
    7c 派 rust_bridge.run_phase4 給 tw_market_core 系列。

    NotImplementedError → status="skipped"(不中斷其他 builder)
    Exception → status="failed" + reason 紀錄(不中斷,對齊 cores_overview §7.5
    dirty 契約,失敗 builder 不 reset is_dirty 留下次重試)。
    """
    from silver.orchestrator import SilverOrchestrator

    start_time = time.monotonic()
    phase_name = args.phase_name
    stock_ids  = (
        [s.strip() for s in args.stocks.split(",")] if args.stocks else None
    )

    db = create_writer()
    db.init_schema()

    # 7c 才需要 rust_bridge;7a / 7b 不用
    rust_bridge = None
    if phase_name == "7c":
        rust_bridge = RustBridge(config.global_cfg.rust_binary_path)

    try:
        orch = SilverOrchestrator(db=db, rust_bridge=rust_bridge)
        logger.info(
            f"Silver started. phase={phase_name}, "
            f"stocks={stock_ids or 'all'}, full_rebuild={args.full_rebuild}"
        )
        result = await orch.run(
            phases       = [phase_name],
            stock_ids    = stock_ids,
            full_rebuild = args.full_rebuild,
        )

        # 印 status table
        print()
        print("=" * 70)
        print(f"Silver phase {phase_name} 結果")
        print("=" * 70)
        phase_result = result["results"].get(phase_name, {})
        if phase_name == "7c":
            print(f"  rust_bridge result: {phase_result}")
        else:
            print(f"{'builder':<22} {'status':<10} {'read':>8} {'wrote':>8} {'ms':>8}")
            print("-" * 60)
            for name, r in phase_result.items():
                status = r.get("status", "?")
                rd = r.get("rows_read", "-")
                wr = r.get("rows_written", "-")
                ms = r.get("elapsed_ms", "-")
                print(f"{name:<22} {status:<10} {str(rd):>8} {str(wr):>8} {str(ms):>8}")
            ok = sum(1 for r in phase_result.values() if r.get("status") == "ok")
            sk = sum(1 for r in phase_result.values() if r.get("status") == "skipped")
            fl = sum(1 for r in phase_result.values() if r.get("status") == "failed")
            total = len(phase_result)
            print("-" * 60)
            print(f"TOTAL: {ok}/{total} OK, {sk} skipped, {fl} failed")
        print()

    except Exception as e:
        logger.error(f"Silver aborted. phase={phase_name}, reason={e}")
        raise
    finally:
        db.close()

    elapsed = int(time.monotonic() - start_time)
    logger.info(f"Silver finished. phase={phase_name}, elapsed={elapsed}s")


# =============================================================================
# 子命令:cross_cores phase 8(Cross-Stock Cores,v3.5 R3 新層)
# =============================================================================

async def _run_cross_cores(args, config) -> None:
    """跑 Cross-Stock Cores Phase 8(目前只有 magic_formula)。

    跟 Silver 對比:
      - Silver per-stock builder 走 silver/orchestrator
      - Cross-Stock builder 走 cross_cores/orchestrator(輸入 universe + date)

    不走 dirty queue(全市場永遠 cross-rank latest date)。
    """
    from cross_cores.orchestrator import CrossStockOrchestrator

    start_time = time.monotonic()
    builders = (
        [b.strip() for b in args.builder.split(",")] if args.builder else None
    )

    db = create_writer()
    db.init_schema()

    try:
        orch = CrossStockOrchestrator(db=db)
        logger.info(
            f"Cross-Stock Cores started. phase=8, builders={builders or 'all'}, "
            f"full_rebuild={args.full_rebuild}"
        )
        result = await orch.run(
            builders     = builders,
            full_rebuild = args.full_rebuild,
            lookback_days= args.lookback_days,
        )

        # 印 status table
        print()
        print("=" * 70)
        print(f"Cross-Stock Cores phase 8 結果")
        print("=" * 70)
        print(f"{'builder':<22} {'status':<10} {'read':>8} {'wrote':>8} {'ms':>8}")
        print("-" * 60)
        for name, r in result["results"].items():
            status = r.get("status", "?")
            rd = r.get("rows_read", "-")
            wr = r.get("rows_written", "-")
            ms = r.get("elapsed_ms", "-")
            print(f"{name:<22} {status:<10} {str(rd):>8} {str(wr):>8} {str(ms):>8}")
        ok = sum(1 for r in result["results"].values() if r.get("status") == "ok")
        fl = sum(1 for r in result["results"].values() if r.get("status") == "failed")
        total = len(result["results"])
        print("-" * 60)
        print(f"TOTAL: {ok}/{total} OK, {fl} failed")
        print()

    except Exception as e:
        logger.error(f"Cross-Stock Cores aborted. phase=8, reason={e}")
        raise
    finally:
        db.close()

    elapsed = int(time.monotonic() - start_time)
    logger.info(f"Cross-Stock Cores finished. phase=8, elapsed={elapsed}s")


# =============================================================================
# 子命令:refresh(一鍵手動更新最新資料)
# =============================================================================

async def _run_refresh(args, config, stock_list_cfg) -> None:
    """一鍵更新最新:Bronze incremental → Silver 7c/7a/7b → M3 cores run-all --dirty。

    內建串完整 chain,以防沒 scheduler 排程。每段獨立 exception handling,前段失敗
    不阻擋後段(對齊 cores_overview §7.5 dirty 契約)。

    Steps:
        1. Bronze incremental(FinMind → Bronze 表)
        2. Silver 7c(Rust 後復權 price_*_fwd + price_limit_merge_events)
        3. Silver 7a(12 個獨立 builder)
        4. Silver 7b(financial_statement 跨表)
        5. M3 Cores `tw_cores run-all --write --dirty`(只跑 dirty stock)
    """
    import argparse as _argparse
    import os
    import subprocess
    from pathlib import Path

    start_time = time.monotonic()
    stocks = args.stocks
    step_results: list[tuple[str, str, float]] = []  # (step, status, elapsed_s)

    def _log_step(idx: int, total: int, label: str) -> None:
        logger.info("=" * 60)
        logger.info(f"[Refresh] Step {idx}/{total}: {label}")
        logger.info("=" * 60)

    def _record(step: str, status: str, t0: float) -> None:
        step_results.append((step, status, time.monotonic() - t0))

    # Steps: Bronze + Silver 7c/7a/7b + Cross-Stock 8 + M3 cores = 6 max
    total_steps = 6 if not args.skip_cores else 5
    if args.skip_bronze:
        total_steps -= 1
    cur = 0

    # Step 1: Bronze incremental
    if not args.skip_bronze:
        cur += 1
        _log_step(cur, total_steps, "Bronze incremental(FinMind → Bronze 表)")
        t0 = time.monotonic()
        try:
            bronze_args = _argparse.Namespace(
                command="incremental",
                phases=None,
                stocks=stocks,
                dry_run=False,
                verbose=args.verbose,
                config=args.config,
                stock_list=args.stock_list,
            )
            await _run_collector(bronze_args, config, stock_list_cfg)
            _record("bronze_incremental", "ok", t0)
        except Exception as e:
            logger.error(f"[Refresh] Bronze incremental 失敗: {e}")
            _record("bronze_incremental", "failed", t0)
    else:
        logger.info("[Refresh] 跳過 Bronze incremental(--skip-bronze)")

    # Step 2-4: Silver phases(7c 必須先跑,因為 7a/7b 讀 fwd 表)
    for phase_name in ("7c", "7a", "7b"):
        cur += 1
        _log_step(cur, total_steps, f"Silver phase {phase_name}")
        t0 = time.monotonic()
        try:
            silver_args = _argparse.Namespace(
                command="silver",
                silver_command="phase",
                phase_name=phase_name,
                stocks=stocks,
                full_rebuild=False,
                verbose=args.verbose,
                config=args.config,
                stock_list=args.stock_list,
            )
            await _run_silver(silver_args, config)
            _record(f"silver_{phase_name}", "ok", t0)
        except Exception as e:
            logger.error(f"[Refresh] Silver {phase_name} 失敗: {e}")
            _record(f"silver_{phase_name}", "failed", t0)

    # Step 5: Cross-Stock Cores Phase 8(v3.5 R3)
    cur += 1
    _log_step(cur, total_steps, "Cross-Stock Cores Phase 8 (magic_formula)")
    t0 = time.monotonic()
    try:
        cross_args = _argparse.Namespace(
            command="cross_cores",
            cross_command="phase",
            phase_name="8",
            builder=None,
            full_rebuild=False,
            lookback_days=None,
            stocks=stocks,
            verbose=args.verbose,
            config=args.config,
            stock_list=args.stock_list,
        )
        await _run_cross_cores(cross_args, config)
        _record("cross_cores_phase8", "ok", t0)
    except Exception as e:
        logger.error(f"[Refresh] Cross-Stock phase 8 失敗: {e}")
        _record("cross_cores_phase8", "failed", t0)

    # Step 6: M3 Cores(可選)
    if not args.skip_cores:
        cur += 1
        binary_dir = Path(config.global_cfg.rust_binary_path).parent
        tw_cores_name = "tw_cores.exe" if os.name == "nt" else "tw_cores"
        tw_cores_path = binary_dir / tw_cores_name
        _log_step(cur, total_steps, f"M3 Cores ({tw_cores_path.name} run-all --write --dirty)")
        t0 = time.monotonic()
        if not tw_cores_path.exists():
            logger.warning(
                f"[Refresh] tw_cores binary 不存在:{tw_cores_path}\n"
                f"  跑 `cd rust_compute && cargo build --release -p tw_cores` 編譯;跳過 M3 cores"
            )
            _record("m3_cores", "skipped(binary missing)", t0)
        else:
            cmd = [str(tw_cores_path), "run-all", "--write", "--dirty"]
            if stocks:
                cmd.extend(["--stocks", stocks])
            logger.info(f"[Refresh] 執行:{' '.join(cmd)}")
            try:
                result = subprocess.run(cmd, check=False)
                if result.returncode == 0:
                    _record("m3_cores", "ok", t0)
                else:
                    logger.error(f"[Refresh] tw_cores exit code {result.returncode}")
                    _record("m3_cores", f"exit={result.returncode}", t0)
            except Exception as e:
                logger.error(f"[Refresh] tw_cores 啟動失敗: {e}")
                _record("m3_cores", "failed", t0)
    else:
        logger.info("[Refresh] 跳過 M3 cores(--skip-cores)")

    # Summary
    elapsed = time.monotonic() - start_time
    print()
    print("=" * 70)
    print("Refresh 結果")
    print("=" * 70)
    print(f"{'step':<24} {'status':<24} {'elapsed':>10}")
    print("-" * 70)
    for step, status, t in step_results:
        print(f"{step:<24} {status:<24} {t:>8.1f}s")
    print("-" * 70)
    print(f"{'total':<24} {'':<24} {elapsed:>8.1f}s")
    print()
    ok = sum(1 for _, s, _ in step_results if s == "ok")
    print(f"OK: {ok}/{len(step_results)} steps")


# =============================================================================
# 子命令:forecast(區間預測 spine,v0.3 spec — backtest / settle / score)
# =============================================================================

def _run_forecast(args) -> None:
    """forecast 子命令 dispatcher。

    走 src.forecast 模組;同步流程(psycopg sync)。每 action 自己開 / 關 conn。
    """
    from datetime import date as _date, datetime as _dt
    import json as _json

    sub = args.forecast_command

    def _parse_date(s: str | None) -> _date | None:
        if s is None:
            return None
        return _dt.strptime(s, "%Y-%m-%d").date()

    if sub == "backtest":
        from forecast.backtest import run_backtest
        from forecast.baseline import make_baseline_forecast
        from forecast._db import get_connection

        # core 對映 forecast_fn
        core_to_fn = {
            "baseline": make_baseline_forecast,
        }
        if args.core not in core_to_fn:
            print(f"[ERROR] 未知 forecast core:{args.core}", file=sys.stderr)
            sys.exit(1)
        fn = core_to_fn[args.core]

        stocks = [s.strip() for s in args.stocks.split(",") if s.strip()]
        since = _parse_date(args.since)
        until = _parse_date(args.until) or _date.today()
        horizons = [int(h) for h in args.horizons.split(",") if h.strip()]
        confidences = [float(c) for c in args.confidences.split(",") if c.strip()]

        with get_connection() as conn:
            total = {"trading_days": 0, "attempted": 0, "written": 0, "skipped": 0}
            for stock_id in stocks:
                logger.info(
                    "forecast backtest stock=%s core=%s [%s, %s] horizons=%s confs=%s",
                    stock_id, args.core, since, until, horizons, confidences,
                )
                summary = run_backtest(
                    conn,
                    stock_id=stock_id,
                    forecast_fn=fn,
                    source_core=args.core,
                    start=since,
                    end=until,
                    horizons=horizons,
                    confidences=confidences,
                )
                for k in total:
                    total[k] += summary[k]
                print(f"{stock_id}: {summary}")
            print(f"\ntotal: {total}")

    elif sub == "settle":
        from forecast.settlement import resolve_pending
        from forecast._db import get_connection

        asof = _parse_date(args.asof) or _date.today()
        stocks = (
            [s.strip() for s in args.stocks.split(",") if s.strip()]
            if args.stocks else [None]
        )
        with get_connection() as conn:
            grand = {"settled": 0, "missing_realized": 0, "errored": 0}
            for sid in stocks:
                summary = resolve_pending(
                    conn, asof=asof, source_core=args.core, stock_id=sid,
                )
                for k in grand:
                    grand[k] += summary[k]
                tag = sid if sid else "ALL"
                print(f"settle asof={asof} stock={tag} core={args.core or 'ALL'} -> {summary}")
            print(f"\ntotal: {grand}")

    elif sub == "conformalize":
        from forecast.calibration import conformalize_batch
        from forecast._db import get_connection

        stocks = [s.strip() for s in args.stocks.split(",") if s.strip()]
        since = _parse_date(args.since)
        until = _parse_date(args.until) or _date.today()
        horizons = [int(h) for h in args.horizons.split(",") if h.strip()]
        confidences = [float(c) for c in args.confidences.split(",") if c.strip()]

        with get_connection() as conn:
            logger.info(
                "forecast conformalize stocks=%s raw=%s target=%s [%s, %s] horizons=%s confs=%s",
                stocks, args.raw_core, args.target_core, since, until, horizons, confidences,
            )
            summary = conformalize_batch(
                conn,
                raw_core=args.raw_core,
                target_core=args.target_core,
                stock_ids=stocks,
                start=since,
                end=until,
                horizons=horizons,
                confidences=confidences,
                calibration_window=args.calibration_window,
                min_calibration_size=args.min_calibration_size,
            )
            print(f"conformalize summary: {summary}")

    elif sub == "score":
        from forecast.scorer import score
        from forecast._db import fetch_resolved, get_connection

        since = _parse_date(args.since)
        with get_connection() as conn:
            rows = fetch_resolved(
                conn,
                source_core=args.core,
                horizon_days=args.horizon,
                stock_id=args.stock,
                since=since,
            )
        result = score(rows, group_by=args.group_by)
        # 整理輸出:reliability 是 [(c, cov)] 不適合 json.dumps default,轉 list
        def _serialize(v):
            if isinstance(v, dict):
                return {k: _serialize(x) for k, x in v.items()}
            if isinstance(v, tuple):
                return list(v)
            if isinstance(v, list):
                return [_serialize(x) for x in v]
            return v
        print(_json.dumps(_serialize(result), ensure_ascii=False, indent=2, default=str))

    else:
        print(f"[ERROR] 未知 forecast 子命令:{sub}", file=sys.stderr)
        sys.exit(1)


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
