// tw_cores — Cores 層 Monolithic Binary 入口
//
// 對齊 m3Spec/cores_overview.md §五(Monolithic Binary 部署模型)
//   - P0 / P1 / P2 一律單一 binary,inventory 自動註冊
//   - 改任一 Core 重編全部(實測 ~5 分鐘可接受)
//   - 無法 hot-fix 單一 Core,但台股一日一交易、batch 模式下沒有此需求
//
// v3.5 R4 C8 拆解(audit Layer 3 痛點 9 — 1693 行 monolith):
//   - cli.rs               CLI parsing(Cli + Command enum)
//   - dispatcher.rs        dispatch_indicator / dispatch_structural / dispatch_neely
//   - writers.rs           connect_pg / write_indicator_value / write_structural_snapshot
//                          / write_facts / resolve_stock_list
//   - run_environment.rs   6 environment cores(market-level)
//   - run_stock_cores.rs   17 stock-level cores per-stock loop
//   - summary.rs           CoreRunSummary + print_summary + loader_err_summary
//   - helpers.rs           parse_timeframe + extract_indicator_meta
//   - workflow.rs          CoreFilter(既有,workflows toml 解析)
//
// **不抽 generic dispatcher<C, W>** 對齊 cores_overview §十四「禁止抽象」+
// 既有 PR-9b 留言「ErasedCore trait wrapper(V3 才考慮)」。

use anyhow::Result;
use clap::Parser;
use fact_schema::{params_hash, WaveCore};
use std::sync::Arc;
use std::time::Instant;

mod cli;
mod dispatcher;
mod helpers;
mod run_environment;
mod run_stock_cores;
mod summary;
mod workflow;
mod writers;

use cli::{Cli, Command};
use dispatcher::dispatch_neely;
use helpers::parse_timeframe;
use run_environment::run_market_cores;
use run_stock_cores::run_stock_cores;
use summary::{print_summary, CoreRunSummary};
use workflow::CoreFilter;
use writers::{connect_pg, resolve_stock_list, write_facts, write_structural_snapshot};

#[tokio::main]
async fn main() -> Result<()> {
    // Load .env from cwd or any parent dir(silently ignored if not found)
    // 對齊 Python 端 db.create_writer 的 load_dotenv 行為,user 不用每次 PS window 手動 set env
    let _ = dotenvy::dotenv();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    match cli.command.unwrap_or(Command::ListCores) {
        Command::ListCores => list_cores(),
        Command::Run {
            stock_id,
            timeframe,
            write,
        } => run_neely_single(&stock_id, &timeframe, write).await,
        Command::RunBacktest {
            stocks,
            start,
            end,
            core,
            write,
            lookback_days,
            concurrency,
        } => run_backtest(stocks, start, end, core, write, lookback_days, concurrency).await,
        Command::RunAll {
            stocks,
            limit,
            timeframe,
            skip_market,
            skip_stock,
            write,
            concurrency,
            dirty,
            workflow,
        } => {
            if dirty && stocks.is_some() {
                anyhow::bail!("--dirty 與 --stocks 互斥(dirty 從 PG dirty queue 拉,stocks 是顯式清單)");
            }
            let filter = match workflow.as_deref() {
                None => CoreFilter::all_enabled(),
                Some(path) => CoreFilter::from_workflow_toml(path)?,
            };
            tracing::info!("workflow filter: {}", filter.count_summary());
            run_all(stocks, limit, &timeframe, skip_market, skip_stock, write, concurrency, dirty, &filter).await
        }
    }
}

// ---------------------------------------------------------------------------
// list-cores
// ---------------------------------------------------------------------------

fn list_cores() -> Result<()> {
    println!("== M3 cores binary(Stage 1-10 partial,Pipeline 走通)==");
    println!("workspace = rust_compute/(virtual root)");
    println!();

    let registry = core_registry::CoreRegistry::discover();
    println!("Linked cores(via inventory CoreRegistry):");
    for core in registry.cores() {
        println!(
            "  - {} v{} [{:?} / {}] — {}",
            core.name, core.version, core.kind, core.priority, core.description
        );
    }

    // 確保 dep crate 的 inventory::submit! 不被 dead-code 剃掉
    let _ = neely_core::NeelyCore::new();
    let _ = macd_core::MacdCore::new();
    let _ = rsi_core::RsiCore::new();
    let _ = kd_core::KdCore::new();
    let _ = adx_core::AdxCore::new();
    let _ = ma_core::MaCore::new();
    let _ = bollinger_core::BollingerCore::new();
    let _ = atr_core::AtrCore::new();
    let _ = obv_core::ObvCore::new();
    let _ = williams_r_core::WilliamsRCore::new();
    let _ = cci_core::CciCore::new();
    let _ = keltner_core::KeltnerCore::new();
    let _ = donchian_core::DonchianCore::new();
    let _ = vwap_core::VwapCore::new();
    let _ = mfi_core::MfiCore::new();
    let _ = coppock_core::CoppockCore::new();
    let _ = ichimoku_core::IchimokuCore::new();
    let _ = support_resistance_core::SupportResistanceCore::new();
    let _ = candlestick_pattern_core::CandlestickPatternCore::new();
    let _ = trendline_core::TrendlineCore::new();
    let _ = day_trading_core::DayTradingCore::new();
    let _ = institutional_core::InstitutionalCore::new();
    let _ = margin_core::MarginCore::new();
    let _ = foreign_holding_core::ForeignHoldingCore::new();
    let _ = shareholder_core::ShareholderCore::new();
    let _ = revenue_core::RevenueCore::new();
    let _ = valuation_core::ValuationCore::new();
    let _ = financial_statement_core::FinancialStatementCore::new();
    let _ = magic_formula_core::MagicFormulaCore::new();    // v3.4
    let _ = kalman_filter_core::KalmanFilterCore::new();    // v3.4
    let _ = kalman_forecast_core::KalmanForecastCore::new(); // v0.3 spine
    let _ = taiex_core::TaiexCore::new();
    let _ = us_market_core::UsMarketCore::new();
    let _ = exchange_rate_core::ExchangeRateCore::new();
    let _ = fear_greed_core::FearGreedCore::new();
    let _ = market_margin_core::MarketMarginCore::new();
    // v3.21 new cores(2026-05-17)
    let _ = loan_collateral_core::LoanCollateralCore::new();
    let _ = block_trade_core::BlockTradeCore::new();
    let _ = risk_alert_core::RiskAlertCore::new();
    let _ = commodity_macro_core::CommodityMacroCore::new();

    println!();
    println!("Stage 1-10 + Facts + PG IO + Inventory + run-all dispatch ✅(M3 PR-9a + v3.21 4 new)");
    println!("(對齊 m3Spec/cores_overview.md §五 + neely_core_architecture.md §七)");
    Ok(())
}

// ---------------------------------------------------------------------------
// run(既有 neely 單核單股 path,M3 PR-7 上線)
// ---------------------------------------------------------------------------

async fn run_neely_single(stock_id: &str, timeframe: &str, write: bool) -> Result<()> {
    let tf = parse_timeframe(timeframe)?;
    let pool = connect_pg(2).await?; // 單股單核,2 connections 足夠

    let mut params = neely_core::NeelyCoreParams::default();
    params.timeframe = tf;
    let series = ohlcv_loader::load_for_neely(&pool, stock_id, &params).await?;

    tracing::info!(
        stock_id,
        bars = series.bars.len(),
        "loaded OHLCV from Silver price_*_fwd"
    );

    let core = neely_core::NeelyCore::new();
    let output = core.compute(&series, params.clone())?;
    let facts = core.produce_facts(&output);

    println!();
    println!("== Stage summary ==");
    println!("stock_id:           {}", stock_id);
    println!("timeframe:          {:?}", tf);
    println!("bars loaded:        {}", series.bars.len());
    println!("monowaves:          {}", output.diagnostics.monowave_count);
    println!("candidates:         {}", output.diagnostics.candidate_count);
    println!(
        "validator pass/rej: {}/{}",
        output.diagnostics.validator_pass_count, output.diagnostics.validator_reject_count
    );
    println!("forest size:        {}", output.scenario_forest.len());
    println!("facts produced:     {}", facts.len());

    if write {
        let hash = params_hash(&params).unwrap_or_default();
        write_structural_snapshot(
            &pool,
            &output.stock_id,
            output.data_range.end,
            tf,
            "neely_core",
            core.version(),
            &hash,
            &serde_json::to_value(&output)?,
        )
        .await?;
        let n = write_facts(&pool, &facts).await?;
        println!();
        println!("== Wrote to PG ==");
        println!("structural_snapshots: 1 row UPSERT");
        println!("facts:                {}/{} new", n, facts.len());
    } else {
        println!();
        println!("(dry-run — 加 --write 落 PG)");
    }

    // 防 dispatch_neely 與 cli 在 list-cores 路徑被視為 dead code(目前 single
    // entry point 用 dispatch_neely;keep ref alive 避免 warning)
    let _ = dispatch_neely;

    Ok(())
}

// ---------------------------------------------------------------------------
// run-backtest(v0.3 spine phase 3 — causal one-pass forecast)
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
async fn run_backtest(
    stocks: String,
    start: String,
    end: Option<String>,
    core_name: String,
    write: bool,
    lookback_days: i32,
    concurrency: usize,
) -> Result<()> {
    use chrono::NaiveDate;
    use dispatcher::dispatch_forecast;
    use futures::stream::{self, StreamExt};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::sync::Mutex;

    if core_name != "kalman_forecast_core" {
        anyhow::bail!(
            "run-backtest: 目前只支援 kalman_forecast_core(其他 forecast core 後續 milestone 加入)"
        );
    }

    let start_d = NaiveDate::parse_from_str(&start, "%Y-%m-%d")
        .map_err(|e| anyhow::anyhow!("invalid --start (expect YYYY-MM-DD): {}", e))?;
    let end_d = match end {
        Some(s) => NaiveDate::parse_from_str(&s, "%Y-%m-%d")
            .map_err(|e| anyhow::anyhow!("invalid --end (expect YYYY-MM-DD): {}", e))?,
        None => chrono::Local::now().date_naive(),
    };
    if start_d > end_d {
        anyhow::bail!("run-backtest: --start ({}) > --end ({})", start_d, end_d);
    }

    let stock_ids: Vec<String> = stocks
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if stock_ids.is_empty() {
        anyhow::bail!("run-backtest: --stocks 不能為空");
    }

    let max_conns: u32 = (concurrency.max(2) + 4) as u32;
    let pool = connect_pg(max_conns).await?;

    // Trading days in [start_d, end_d]
    let trading_days: Vec<NaiveDate> = sqlx::query_as::<_, (NaiveDate,)>(
        r#"SELECT date FROM trading_date_ref
           WHERE market = 'TW' AND date BETWEEN $1 AND $2
           ORDER BY date"#,
    )
    .bind(start_d)
    .bind(end_d)
    .fetch_all(&pool)
    .await?
    .into_iter()
    .map(|(d,)| d)
    .collect();

    if trading_days.is_empty() {
        tracing::warn!(
            "run-backtest: 沒有 trading days in [{}, {}](trading_date_ref 為空?)",
            start_d, end_d
        );
        return Ok(());
    }

    let total_pairs = trading_days.len() * stock_ids.len();
    tracing::info!(
        "== Backtest: {} stocks × {} trading days = {} (stock, T) pairs (core={}, concurrency={}) ==",
        stock_ids.len(), trading_days.len(), total_pairs, core_name, concurrency
    );

    let total_start = Instant::now();
    let summary: Arc<Mutex<Vec<CoreRunSummary>>> = Arc::new(Mutex::new(Vec::new()));
    let progress = Arc::new(AtomicUsize::new(0));

    // Build the cartesian iterator (stock, T) — stream concurrent per pair.
    let pairs: Vec<(String, NaiveDate)> = stock_ids
        .iter()
        .flat_map(|sid| trading_days.iter().map(move |d| (sid.clone(), *d)))
        .collect();

    stream::iter(pairs)
        .for_each_concurrent(concurrency, |(sid, asof)| {
            let pool = pool.clone();
            let summary = summary.clone();
            let progress = progress.clone();
            async move {
                // PIT-aware loader — only events with date ≤ asof are applied
                let series = match ohlcv_loader::load_asof_daily(&pool, &sid, asof, lookback_days)
                    .await
                {
                    Ok(s) => s,
                    Err(e) => {
                        summary.lock().await.push(summary::loader_err_summary(
                            "kalman_forecast_core", &sid, "load_asof_daily", &e,
                        ));
                        let n = progress.fetch_add(1, Ordering::Relaxed) + 1;
                        if n % 200 == 0 || n == total_pairs {
                            tracing::info!("backtest progress: {}/{}", n, total_pairs);
                        }
                        return;
                    }
                };

                if series.bars.is_empty() {
                    // No data this early — skip silently
                    progress.fetch_add(1, Ordering::Relaxed);
                    return;
                }

                let res = dispatch_forecast(
                    &pool,
                    &kalman_forecast_core::KalmanForecastCore::new(),
                    &series,
                    kalman_forecast_core::KalmanForecastParams::default(),
                    write,
                    "kalman_forecast_core",
                    false,
                )
                .await;
                summary.lock().await.push(res);

                let n = progress.fetch_add(1, Ordering::Relaxed) + 1;
                if n % 200 == 0 || n == total_pairs {
                    tracing::info!("backtest progress: {}/{}", n, total_pairs);
                }
            }
        })
        .await;

    let final_summary = summary.lock().await.clone();
    print_summary(&final_summary, total_start.elapsed(), write);
    Ok(())
}

// ---------------------------------------------------------------------------
// run-all(M3 PR-9a 主入口)
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
async fn run_all(
    stocks: Option<String>,
    limit: Option<usize>,
    timeframe_str: &str,
    skip_market: bool,
    skip_stock: bool,
    write: bool,
    concurrency: usize,
    dirty: bool,
    filter: &CoreFilter,
) -> Result<()> {
    use futures::stream::{self, StreamExt};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::sync::Mutex;

    let tf = parse_timeframe(timeframe_str)?;
    // PR-9b:max_connections 對齊 concurrency,額外 +4 給 environment cores / 進度 query
    let max_conns: u32 = (concurrency.max(2) + 4) as u32;
    let pool = connect_pg(max_conns).await?;
    let total_start = Instant::now();
    let summary: Arc<Mutex<Vec<CoreRunSummary>>> = Arc::new(Mutex::new(Vec::new()));

    if !skip_market {
        tracing::info!("== Stage A: 6 environment cores(market-level run-once)==");
        let mut env_summary = Vec::new();
        run_market_cores(&pool, write, filter, &mut env_summary).await;
        summary.lock().await.extend(env_summary);
    } else {
        tracing::info!("--skip-market 已指定,跳過 environment cores");
    }

    if !skip_stock {
        let stock_ids = resolve_stock_list(&pool, stocks.as_deref(), limit, dirty).await?;
        let total = stock_ids.len();
        if dirty && total == 0 {
            tracing::info!("dirty queue 為空(price_daily_fwd.is_dirty=TRUE 無 rows),skip Stage B");
        } else if dirty {
            tracing::info!(
                "== Stage B: {} stocks × 17 cores(dirty queue, concurrency={}, timeframe={:?})==",
                total, concurrency, tf
            );
        } else {
            tracing::info!(
                "== Stage B: {} stocks × 17 cores(concurrency={}, timeframe={:?})==",
                total, concurrency, tf
            );
        }
        let progress = Arc::new(AtomicUsize::new(0));
        // PR-9b:per-stock task spawn 並行,sqlx pool 自動分配 connection
        // for_each_concurrent 限 N 個 future 同時 active;summary 用 Mutex 保護累加
        stream::iter(stock_ids)
            .for_each_concurrent(concurrency, |stock_id| {
                let pool = pool.clone();
                let summary = summary.clone();
                let progress = progress.clone();
                async move {
                    let mut local: Vec<CoreRunSummary> = Vec::new();
                    run_stock_cores(&pool, &stock_id, tf, write, filter, &mut local).await;
                    let n = progress.fetch_add(1, Ordering::Relaxed) + 1;
                    if n % 100 == 0 || n == total {
                        tracing::info!("progress: stock {}/{} ({})", n, total, stock_id);
                    }
                    summary.lock().await.extend(local);
                }
            })
            .await;
    } else {
        tracing::info!("--skip-stock 已指定,跳過 stock-level cores");
    }

    let final_summary = summary.lock().await.clone();
    print_summary(&final_summary, total_start.elapsed(), write);
    Ok(())
}
