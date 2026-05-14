// tw_cores — Cores 層 Monolithic Binary 入口
//
// 對齊 m2Spec/oldm2Spec/cores_overview.md §五(Monolithic Binary 部署模型)
//   - P0 / P1 / P2 一律單一 binary,inventory 自動註冊
//   - 改任一 Core 重編全部(實測 ~5 分鐘可接受)
//   - 無法 hot-fix 單一 Core,但台股一日一交易、batch 模式下沒有此需求
//
// **M3 PR-9a 範圍**(本 PR 動工):
//   - `run-all` subcommand:全市場 × 全 22 cores production run
//   - 22 個硬編碼 dispatch arm(對齊 V2 禁止抽象原則,§十四)
//   - 5 個 environment cores run-once(market-level)
//   - 17 個 stock-level cores per-stock loop
//   - indicator_values 寫入(本 PR 補)+ structural_snapshots(既有 neely)+ facts
//   - per-core / per-stock 失敗不阻塞 batch
//
// 留 PR-9b:
//   - Workflow toml dispatch(取代 hardcoded)
//   - sqlx pool 並行 per-stock
//   - ErasedCore trait wrapper(V3 才考慮)

use anyhow::{Context, Result};
use chrono::{NaiveDate, Utc};
use clap::{Parser, Subcommand};
use fact_schema::{params_hash, Fact, IndicatorCore, Timeframe, WaveCore};
use serde::Serialize;
use sqlx::postgres::{PgPool, PgPoolOptions};
use std::path::PathBuf;
use std::time::Instant;

mod workflow;
use workflow::CoreFilter;

#[derive(Parser, Debug)]
#[command(
    name = "tw_cores",
    version,
    about = "Cores 層 Monolithic Binary"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// 列出已連結的 22 cores(對齊 inventory CoreRegistry::discover)
    #[command(name = "list-cores")]
    ListCores,
    /// 對指定 stock 跑 neely_core 完整 Stage 1-10 Pipeline
    #[command(name = "run")]
    Run {
        /// 股票代號(例 2330,或保留字 _index_taiex_)
        #[arg(long)]
        stock_id: String,
        /// 時間粒度:daily / weekly / monthly
        #[arg(long, default_value = "daily")]
        timeframe: String,
        /// 寫入 PG(structural_snapshots + facts)— 不指定僅 dry-run
        #[arg(long, default_value_t = false)]
        write: bool,
    },
    /// 全市場 × 全 22 cores production run
    #[command(name = "run-all")]
    RunAll {
        /// 指定股票清單(逗號分隔,例 2330,2317);不指定則拉 price_daily_fwd 全市場
        #[arg(long)]
        stocks: Option<String>,
        /// 限制前 N 檔(test / P0 Gate 用)
        #[arg(long)]
        limit: Option<usize>,
        /// 時間粒度(stock-level cores 用,環境 cores 自己決定)
        #[arg(long, default_value = "daily")]
        timeframe: String,
        /// 跳過 5 個 environment cores(只跑 stock-level)
        #[arg(long, default_value_t = false)]
        skip_market: bool,
        /// 跳過 17 個 stock-level cores(只跑 environment)
        #[arg(long, default_value_t = false)]
        skip_stock: bool,
        /// 寫入 PG(indicator_values + structural_snapshots + facts)— 不指定僅 dry-run
        #[arg(long, default_value_t = false)]
        write: bool,
        /// Stage B per-stock 並行度(預設 32,需 ≤ PG max_connections - 4 buffer)
        /// 串列跑用 1;全市場 1263 stocks 從 ~9min 降到 ~5min(PR-9d 升 16→32)
        #[arg(long, default_value_t = 32)]
        concurrency: usize,
        /// 只跑 dirty queue(SELECT DISTINCT stock_id FROM price_daily_fwd WHERE is_dirty=TRUE)
        /// 對齊 silver/orchestrator.py:_fetch_dirty_fwd_stocks pattern。
        /// 與 --stocks 互斥;與 --limit 可疊加(取前 N 個 dirty stocks)
        #[arg(long, default_value_t = false)]
        dirty: bool,
        /// Workflow toml 路徑(動態決定跑哪些 cores)
        /// 對齊 m3Spec/cores_overview.md §13.1 + workflows/tw_stock_standard.toml
        /// 未指定 → 全 23 cores 跑(對齊原 PR-9a 行為)
        #[arg(long)]
        workflow: Option<PathBuf>,
    },
}

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
    let _ = taiex_core::TaiexCore::new();
    let _ = us_market_core::UsMarketCore::new();
    let _ = exchange_rate_core::ExchangeRateCore::new();
    let _ = fear_greed_core::FearGreedCore::new();
    let _ = market_margin_core::MarketMarginCore::new();

    println!();
    println!("Stage 1-10 + Facts + PG IO + Inventory + run-all dispatch ✅(M3 PR-9a)");
    println!("(對齊 m2Spec/oldm2Spec/cores_overview.md §五 + neely_core.md §七)");
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

    Ok(())
}

// ---------------------------------------------------------------------------
// run-all(M3 PR-9a 主入口)
// ---------------------------------------------------------------------------

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
    use std::sync::Arc;
    use tokio::sync::Mutex;

    let tf = parse_timeframe(timeframe_str)?;
    // PR-9b:max_connections 對齊 concurrency,額外 +4 給 environment cores / 進度 query
    let max_conns: u32 = (concurrency.max(2) + 4) as u32;
    let pool = connect_pg(max_conns).await?;
    let total_start = Instant::now();
    let summary: Arc<Mutex<Vec<CoreRunSummary>>> = Arc::new(Mutex::new(Vec::new()));

    if !skip_market {
        tracing::info!("== Stage A: 5 environment cores(market-level run-once)==");
        let mut env_summary = Vec::new();
        run_market_cores(&pool, write, filter, &mut env_summary).await;
        summary.lock().await.extend(env_summary);
    } else {
        tracing::info!("--skip-market 已指定,跳過 5 environment cores");
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
        tracing::info!("--skip-stock 已指定,跳過 17 stock-level cores");
    }

    let final_summary = summary.lock().await.clone();
    print_summary(&final_summary, total_start.elapsed(), write);
    Ok(())
}

// ---------------------------------------------------------------------------
// Environment cores(5,run-once)
// ---------------------------------------------------------------------------

async fn run_market_cores(
    pool: &PgPool,
    write: bool,
    filter: &CoreFilter,
    summary: &mut Vec<CoreRunSummary>,
) {
    // 環境 cores 各自 warmup 不一,給 5 年(~1825 天)足量歷史
    const ENV_LOOKBACK_DAYS: i32 = 365 * 5;

    // 1. taiex_core
    if filter.is_enabled("taiex_core") {
    match environment_loader::load_taiex(pool, ENV_LOOKBACK_DAYS).await {
        Ok(series) => {
            let core = taiex_core::TaiexCore::new();
            summary.push(
                dispatch_indicator(pool, &core, &series, taiex_core::TaiexParams::default(), write)
                    .await,
            );
        }
        Err(e) => summary.push(loader_err_summary(
            "taiex_core",
            "_index_taiex_",
            "load_taiex",
            &e,
        )),
    }
    }

    // 2. us_market_core
    if filter.is_enabled("us_market_core") {
    match environment_loader::load_us_market_combined(pool, ENV_LOOKBACK_DAYS).await {
        Ok(combined) => {
            let core = us_market_core::UsMarketCore::new();
            summary.push(
                dispatch_indicator(
                    pool,
                    &core,
                    &combined,
                    us_market_core::UsMarketParams::default(),
                    write,
                )
                .await,
            );
        }
        Err(e) => summary.push(loader_err_summary(
            "us_market_core",
            "_index_us_market_",
            "load_us_market_combined",
            &e,
        )),
    }
    }

    // 3. exchange_rate_core(USD/TWD,後續 Params currency_pairs 多幣別留 follow-up)
    if filter.is_enabled("exchange_rate_core") {
    match environment_loader::load_exchange_rate(pool, "USD", ENV_LOOKBACK_DAYS).await {
        Ok(series) => {
            let core = exchange_rate_core::ExchangeRateCore::new();
            summary.push(
                dispatch_indicator(
                    pool,
                    &core,
                    &series,
                    exchange_rate_core::ExchangeRateParams::default(),
                    write,
                )
                .await,
            );
        }
        Err(e) => summary.push(loader_err_summary(
            "exchange_rate_core",
            "_global_",
            "load_exchange_rate",
            &e,
        )),
    }
    }

    // 4. fear_greed_core
    if filter.is_enabled("fear_greed_core") {
    match environment_loader::load_fear_greed(pool, ENV_LOOKBACK_DAYS).await {
        Ok(series) => {
            let core = fear_greed_core::FearGreedCore::new();
            summary.push(
                dispatch_indicator(
                    pool,
                    &core,
                    &series,
                    fear_greed_core::FearGreedParams::default(),
                    write,
                )
                .await,
            );
        }
        Err(e) => summary.push(loader_err_summary(
            "fear_greed_core",
            "_global_",
            "load_fear_greed",
            &e,
        )),
    }
    }

    // 5. market_margin_core
    if filter.is_enabled("market_margin_core") {
    match environment_loader::load_market_margin(pool, ENV_LOOKBACK_DAYS).await {
        Ok(series) => {
            let core = market_margin_core::MarketMarginCore::new();
            summary.push(
                dispatch_indicator(
                    pool,
                    &core,
                    &series,
                    market_margin_core::MarketMarginParams::default(),
                    write,
                )
                .await,
            );
        }
        Err(e) => summary.push(loader_err_summary(
            "market_margin_core",
            "_market_",
            "load_market_margin",
            &e,
        )),
    }
    }

    // 6. business_indicator_core(月頻;Silver 端 sentinel `_market_`,Core 端保留字 `_index_business_`)
    if filter.is_enabled("business_indicator_core") {
    match environment_loader::load_business_indicator(pool, ENV_LOOKBACK_DAYS).await {
        Ok(series) => {
            let core = business_indicator_core::BusinessIndicatorCore::new();
            summary.push(
                dispatch_indicator(
                    pool,
                    &core,
                    &series,
                    business_indicator_core::BusinessIndicatorParams::default(),
                    write,
                )
                .await,
            );
        }
        Err(e) => summary.push(loader_err_summary(
            "business_indicator_core",
            "_index_business_",
            "load_business_indicator",
            &e,
        )),
    }
    }
}

// ---------------------------------------------------------------------------
// Stock-level cores(17:1 Wave + 8 Indicator + 5 Chip + 3 Fundamental)
// ---------------------------------------------------------------------------

async fn run_stock_cores(
    pool: &PgPool,
    stock_id: &str,
    tf: Timeframe,
    write: bool,
    filter: &CoreFilter,
    summary: &mut Vec<CoreRunSummary>,
) {
    // Lookback 上限給足:6 年日線(覆蓋各 indicator warmup × 1.2 + 充足實際 series)
    const STOCK_LOOKBACK_DAYS: i32 = 365 * 6;
    const STOCK_LOOKBACK_MONTHS: i32 = 6 * 12 + 12;
    const STOCK_LOOKBACK_QUARTERS: i32 = 6 * 4 + 4;

    // ---- 1. Wave: neely_core(走 structural_snapshots,不寫 indicator_values)----
    if filter.is_enabled("neely_core") {
    let mut neely_params = neely_core::NeelyCoreParams::default();
    neely_params.timeframe = tf;
    match ohlcv_loader::load_for_neely(pool, stock_id, &neely_params).await {
        Ok(series) => summary.push(dispatch_neely(pool, stock_id, &series, neely_params, write).await),
        Err(e) => summary.push(loader_err_summary(
            "neely_core",
            stock_id,
            "load_for_neely",
            &e,
        )),
    }
    }

    // ---- 2-9. Indicator(P1 8 + P3 8 + P2 pattern 3 = 19)— 共用 OhlcvSeries ----
    // 19 cores 共用 ohlcv,若全 disabled 可整段 skip(節省 1 個 query)
    let any_indicator_enabled = [
        "macd_core", "rsi_core", "kd_core", "adx_core",
        "ma_core", "bollinger_core", "atr_core", "obv_core",
        "williams_r_core", "cci_core", "keltner_core", "donchian_core",
        "vwap_core", "mfi_core", "coppock_core", "ichimoku_core",
        "support_resistance_core", "candlestick_pattern_core", "trendline_core",
    ].iter().any(|n| filter.is_enabled(n));
    if any_indicator_enabled {
    let ohlcv_result = match tf {
        Timeframe::Daily => ohlcv_loader::load_daily(pool, stock_id, STOCK_LOOKBACK_DAYS).await,
        Timeframe::Weekly => ohlcv_loader::load_weekly(pool, stock_id, STOCK_LOOKBACK_DAYS / 7).await,
        Timeframe::Monthly => ohlcv_loader::load_monthly(pool, stock_id, STOCK_LOOKBACK_MONTHS).await,
        Timeframe::Quarterly => Err(anyhow::anyhow!(
            "Quarterly 不適用 OHLCV(season 報表專用 Timeframe);stock_level indicator cores 不該帶 Quarterly"
        )),
    };
    match ohlcv_result {
        Ok(ohlcv) => {
            // 每個 indicator core 獨立 dispatch,失敗不阻塞其他
            if filter.is_enabled("macd_core") {
            summary.push(
                dispatch_indicator(
                    pool,
                    &macd_core::MacdCore::new(),
                    &ohlcv,
                    macd_core::MacdParams::default(),
                    write,
                )
                .await,
            );
            }
            if filter.is_enabled("rsi_core") {
            summary.push(
                dispatch_indicator(
                    pool,
                    &rsi_core::RsiCore::new(),
                    &ohlcv,
                    rsi_core::RsiParams::default(),
                    write,
                )
                .await,
            );
            }
            if filter.is_enabled("kd_core") {
            summary.push(
                dispatch_indicator(
                    pool,
                    &kd_core::KdCore::new(),
                    &ohlcv,
                    kd_core::KdParams::default(),
                    write,
                )
                .await,
            );
            }
            if filter.is_enabled("adx_core") {
            summary.push(
                dispatch_indicator(
                    pool,
                    &adx_core::AdxCore::new(),
                    &ohlcv,
                    adx_core::AdxParams::default(),
                    write,
                )
                .await,
            );
            }
            if filter.is_enabled("ma_core") {
            summary.push(
                dispatch_indicator(
                    pool,
                    &ma_core::MaCore::new(),
                    &ohlcv,
                    ma_core::MaParams::default(),
                    write,
                )
                .await,
            );
            }
            if filter.is_enabled("bollinger_core") {
            summary.push(
                dispatch_indicator(
                    pool,
                    &bollinger_core::BollingerCore::new(),
                    &ohlcv,
                    bollinger_core::BollingerParams::default(),
                    write,
                )
                .await,
            );
            }
            if filter.is_enabled("atr_core") {
            summary.push(
                dispatch_indicator(
                    pool,
                    &atr_core::AtrCore::new(),
                    &ohlcv,
                    atr_core::AtrParams::default(),
                    write,
                )
                .await,
            );
            }
            if filter.is_enabled("obv_core") {
            summary.push(
                dispatch_indicator(
                    pool,
                    &obv_core::ObvCore::new(),
                    &ohlcv,
                    obv_core::ObvParams::default(),
                    write,
                )
                .await,
            );
            }
            // ---- P3 indicator cores(williams_r / cci / keltner / donchian / mfi / coppock / ichimoku)----
            if filter.is_enabled("williams_r_core") {
            summary.push(
                dispatch_indicator(
                    pool,
                    &williams_r_core::WilliamsRCore::new(),
                    &ohlcv,
                    williams_r_core::WilliamsRParams::default(),
                    write,
                )
                .await,
            );
            }
            if filter.is_enabled("cci_core") {
            summary.push(
                dispatch_indicator(
                    pool,
                    &cci_core::CciCore::new(),
                    &ohlcv,
                    cci_core::CciParams::default(),
                    write,
                )
                .await,
            );
            }
            if filter.is_enabled("keltner_core") {
            summary.push(
                dispatch_indicator(
                    pool,
                    &keltner_core::KeltnerCore::new(),
                    &ohlcv,
                    keltner_core::KeltnerParams::default(),
                    write,
                )
                .await,
            );
            }
            if filter.is_enabled("donchian_core") {
            summary.push(
                dispatch_indicator(
                    pool,
                    &donchian_core::DonchianCore::new(),
                    &ohlcv,
                    donchian_core::DonchianParams::default(),
                    write,
                )
                .await,
            );
            }
            if filter.is_enabled("mfi_core") {
            summary.push(
                dispatch_indicator(
                    pool,
                    &mfi_core::MfiCore::new(),
                    &ohlcv,
                    mfi_core::MfiParams::default(),
                    write,
                )
                .await,
            );
            }
            if filter.is_enabled("coppock_core") {
            summary.push(
                dispatch_indicator(
                    pool,
                    &coppock_core::CoppockCore::new(),
                    &ohlcv,
                    coppock_core::CoppockParams::default(),
                    write,
                )
                .await,
            );
            }
            if filter.is_enabled("ichimoku_core") {
            summary.push(
                dispatch_indicator(
                    pool,
                    &ichimoku_core::IchimokuCore::new(),
                    &ohlcv,
                    ichimoku_core::IchimokuParams::default(),
                    write,
                )
                .await,
            );
            }
            // ---- P2 pattern cores(support_resistance / candlestick_pattern)— 共用 ohlcv ----
            if filter.is_enabled("support_resistance_core") {
            summary.push(
                dispatch_indicator(
                    pool,
                    &support_resistance_core::SupportResistanceCore::new(),
                    &ohlcv,
                    support_resistance_core::SupportResistanceParams::default(),
                    write,
                )
                .await,
            );
            }
            if filter.is_enabled("candlestick_pattern_core") {
            summary.push(
                dispatch_indicator(
                    pool,
                    &candlestick_pattern_core::CandlestickPatternCore::new(),
                    &ohlcv,
                    candlestick_pattern_core::CandlestickPatternParams::default(),
                    write,
                )
                .await,
            );
            }
            // ---- trendline_core(P2,唯一耦合例外)— 跑 neely_core 取 monowave_series 餵入 ----
            if filter.is_enabled("trendline_core") {
            let mut tl_neely_params = neely_core::NeelyCoreParams::default();
            tl_neely_params.timeframe = tf;
            match neely_core::NeelyCore::new().compute(&ohlcv, tl_neely_params) {
                Ok(neely_out) => {
                    let tl_input = trendline_core::TrendlineInput {
                        ohlcv: ohlcv.clone(),
                        monowaves: neely_out.monowave_series.clone(),
                    };
                    summary.push(
                        dispatch_indicator(
                            pool,
                            &trendline_core::TrendlineCore::new(),
                            &tl_input,
                            trendline_core::TrendlineParams::default(),
                            write,
                        )
                        .await,
                    );
                }
                Err(e) => summary.push(loader_err_summary(
                    "trendline_core",
                    stock_id,
                    "neely_monowave",
                    &e,
                )),
            }
            }
            // ---- vwap_core(P3,需 anchor_date)— 預設用 series 第一個 bar 的日期 ----
            if filter.is_enabled("vwap_core") {
            let anchor = ohlcv.bars.first().map(|b| b.date);
            if let Some(anchor_date) = anchor {
                let mut vwap_params = vwap_core::VwapParams::default();
                vwap_params.anchor_date = Some(anchor_date);
                summary.push(
                    dispatch_indicator(
                        pool,
                        &vwap_core::VwapCore::new(),
                        &ohlcv,
                        vwap_params,
                        write,
                    )
                    .await,
                );
            } else {
                summary.push(loader_err_summary(
                    "vwap_core",
                    stock_id,
                    "empty_series",
                    &anyhow::anyhow!("ohlcv series 空,無法決定 vwap anchor_date"),
                ));
            }
            }
        }
        Err(e) => {
            for name in [
                "macd_core",
                "rsi_core",
                "kd_core",
                "adx_core",
                "ma_core",
                "bollinger_core",
                "atr_core",
                "obv_core",
                "williams_r_core",
                "cci_core",
                "keltner_core",
                "donchian_core",
                "vwap_core",
                "mfi_core",
                "coppock_core",
                "ichimoku_core",
                "support_resistance_core",
                "candlestick_pattern_core",
                "trendline_core",
            ] {
                if filter.is_enabled(name) {
                    summary.push(loader_err_summary(name, stock_id, "load_daily", &e));
                }
            }
        }
    }
    }

    // ---- 10. day_trading_core ----
    if filter.is_enabled("day_trading_core") {
    match chip_loader::load_day_trading(pool, stock_id, STOCK_LOOKBACK_DAYS).await {
        Ok(series) => summary.push(
            dispatch_indicator(
                pool,
                &day_trading_core::DayTradingCore::new(),
                &series,
                day_trading_core::DayTradingParams::default(),
                write,
            )
            .await,
        ),
        Err(e) => summary.push(loader_err_summary(
            "day_trading_core",
            stock_id,
            "load_day_trading",
            &e,
        )),
    }
    }

    // ---- 11. institutional_core ----
    if filter.is_enabled("institutional_core") {
    match chip_loader::load_institutional_daily(pool, stock_id, STOCK_LOOKBACK_DAYS).await {
        Ok(series) => summary.push(
            dispatch_indicator(
                pool,
                &institutional_core::InstitutionalCore::new(),
                &series,
                institutional_core::InstitutionalParams::default(),
                write,
            )
            .await,
        ),
        Err(e) => summary.push(loader_err_summary(
            "institutional_core",
            stock_id,
            "load_institutional_daily",
            &e,
        )),
    }
    }

    // ---- 12. margin_core ----
    if filter.is_enabled("margin_core") {
    match chip_loader::load_margin_daily(pool, stock_id, STOCK_LOOKBACK_DAYS).await {
        Ok(series) => summary.push(
            dispatch_indicator(
                pool,
                &margin_core::MarginCore::new(),
                &series,
                margin_core::MarginParams::default(),
                write,
            )
            .await,
        ),
        Err(e) => summary.push(loader_err_summary(
            "margin_core",
            stock_id,
            "load_margin_daily",
            &e,
        )),
    }
    }

    // ---- 13. foreign_holding_core ----
    if filter.is_enabled("foreign_holding_core") {
    match chip_loader::load_foreign_holding(pool, stock_id, STOCK_LOOKBACK_DAYS).await {
        Ok(series) => summary.push(
            dispatch_indicator(
                pool,
                &foreign_holding_core::ForeignHoldingCore::new(),
                &series,
                foreign_holding_core::ForeignHoldingParams::default(),
                write,
            )
            .await,
        ),
        Err(e) => summary.push(loader_err_summary(
            "foreign_holding_core",
            stock_id,
            "load_foreign_holding",
            &e,
        )),
    }
    }

    // ---- 14. shareholder_core(週頻 — Params::default() timeframe = Weekly)----
    if filter.is_enabled("shareholder_core") {
    match chip_loader::load_holding_shares_per(pool, stock_id, STOCK_LOOKBACK_DAYS).await {
        Ok(series) => summary.push(
            dispatch_indicator(
                pool,
                &shareholder_core::ShareholderCore::new(),
                &series,
                shareholder_core::ShareholderParams::default(),
                write,
            )
            .await,
        ),
        Err(e) => summary.push(loader_err_summary(
            "shareholder_core",
            stock_id,
            "load_holding_shares_per",
            &e,
        )),
    }
    }

    // ---- 15. revenue_core(月頻)----
    if filter.is_enabled("revenue_core") {
    match fundamental_loader::load_monthly_revenue(pool, stock_id, STOCK_LOOKBACK_MONTHS).await {
        Ok(series) => summary.push(
            dispatch_indicator(
                pool,
                &revenue_core::RevenueCore::new(),
                &series,
                revenue_core::RevenueParams::default(),
                write,
            )
            .await,
        ),
        Err(e) => summary.push(loader_err_summary(
            "revenue_core",
            stock_id,
            "load_monthly_revenue",
            &e,
        )),
    }
    }

    // ---- 16. valuation_core(日頻)----
    if filter.is_enabled("valuation_core") {
    match fundamental_loader::load_valuation_daily(pool, stock_id, STOCK_LOOKBACK_DAYS).await {
        Ok(series) => summary.push(
            dispatch_indicator(
                pool,
                &valuation_core::ValuationCore::new(),
                &series,
                valuation_core::ValuationParams::default(),
                write,
            )
            .await,
        ),
        Err(e) => summary.push(loader_err_summary(
            "valuation_core",
            stock_id,
            "load_valuation_daily",
            &e,
        )),
    }
    }

    // ---- 17. financial_statement_core(季頻)----
    if filter.is_enabled("financial_statement_core") {
    match fundamental_loader::load_financial_statement(pool, stock_id, STOCK_LOOKBACK_QUARTERS).await
    {
        Ok(series) => summary.push(
            dispatch_indicator(
                pool,
                &financial_statement_core::FinancialStatementCore::new(),
                &series,
                financial_statement_core::FinancialStatementParams::default(),
                write,
            )
            .await,
        ),
        Err(e) => summary.push(loader_err_summary(
            "financial_statement_core",
            stock_id,
            "load_financial_statement",
            &e,
        )),
    }
    }
}

// ---------------------------------------------------------------------------
// Generic dispatch helper(IndicatorCore + WaveCore 各一)
// ---------------------------------------------------------------------------

async fn dispatch_indicator<C>(
    pool: &PgPool,
    core: &C,
    input: &C::Input,
    params: C::Params,
    write: bool,
) -> CoreRunSummary
where
    C: IndicatorCore,
{
    let start = Instant::now();
    let core_name = core.name().to_string();
    let core_version = core.version().to_string();
    let hash = params_hash(&params).unwrap_or_default();

    match core.compute(input, params) {
        Ok(output) => {
            let facts = core.produce_facts(&output);
            let value_json = match serde_json::to_value(&output) {
                Ok(v) => v,
                Err(e) => {
                    return CoreRunSummary::err(
                        &core_name,
                        "",
                        format!("serialize output failed: {}", e),
                        start,
                    );
                }
            };
            let (stock_id, value_date, timeframe_str) = extract_indicator_meta(&value_json);

            let mut iv_written = 0u64;
            let mut fact_written = 0u64;
            if write {
                if !stock_id.is_empty() {
                    match write_indicator_value(
                        pool,
                        &stock_id,
                        value_date,
                        &timeframe_str,
                        &core_name,
                        &core_version,
                        &hash,
                        &value_json,
                    )
                    .await
                    {
                        Ok(()) => iv_written = 1,
                        Err(e) => tracing::warn!(
                            core = %core_name,
                            stock_id,
                            "write_indicator_value failed: {:#}",
                            e
                        ),
                    }
                }
                match write_facts(pool, &facts).await {
                    Ok(n) => fact_written = n,
                    Err(e) => tracing::warn!(core = %core_name, "write_facts failed: {:#}", e),
                }
            }

            CoreRunSummary {
                core: core_name,
                stock_id,
                status: "ok".to_string(),
                events: facts.len() as u64,
                iv_written,
                fact_written,
                elapsed_ms: start.elapsed().as_millis() as u64,
                error: None,
            }
        }
        Err(e) => CoreRunSummary::err(&core_name, "", format!("compute failed: {:#}", e), start),
    }
}

async fn dispatch_neely(
    pool: &PgPool,
    stock_id: &str,
    series: &neely_core::output::OhlcvSeries,
    params: neely_core::NeelyCoreParams,
    write: bool,
) -> CoreRunSummary {
    let start = Instant::now();
    let core = neely_core::NeelyCore::new();
    let hash = params_hash(&params).unwrap_or_default();
    let tf = params.timeframe;
    let core_version = core.version().to_string();

    match core.compute(series, params) {
        Ok(output) => {
            let facts = core.produce_facts(&output);
            let mut iv_written = 0u64; // neely 不寫 indicator_values
            let mut fact_written = 0u64;
            if write {
                let snapshot_json = match serde_json::to_value(&output) {
                    Ok(v) => v,
                    Err(e) => {
                        return CoreRunSummary::err(
                            "neely_core",
                            stock_id,
                            format!("serialize output failed: {}", e),
                            start,
                        );
                    }
                };
                match write_structural_snapshot(
                    pool,
                    &output.stock_id,
                    output.data_range.end,
                    tf,
                    "neely_core",
                    &core_version,
                    &hash,
                    &snapshot_json,
                )
                .await
                {
                    Ok(()) => iv_written = 1, // 借用欄位記 snapshot 寫 1 row
                    Err(e) => tracing::warn!(
                        core = "neely_core",
                        stock_id,
                        "write_structural_snapshot failed: {:#}",
                        e
                    ),
                }
                match write_facts(pool, &facts).await {
                    Ok(n) => fact_written = n,
                    Err(e) => {
                        tracing::warn!(core = "neely_core", "write_facts failed: {:#}", e)
                    }
                }
            }

            CoreRunSummary {
                core: "neely_core".to_string(),
                stock_id: stock_id.to_string(),
                status: "ok".to_string(),
                events: facts.len() as u64,
                iv_written,
                fact_written,
                elapsed_ms: start.elapsed().as_millis() as u64,
                error: None,
            }
        }
        Err(e) => CoreRunSummary::err(
            "neely_core",
            stock_id,
            format!("compute failed: {:#}", e),
            start,
        ),
    }
}

// ---------------------------------------------------------------------------
// PG IO helpers
// ---------------------------------------------------------------------------

async fn connect_pg(max_connections: u32) -> Result<PgPool> {
    let database_url = std::env::var("DATABASE_URL")
        .context("DATABASE_URL not set — set environment or .env before running")?;
    let pool = PgPoolOptions::new()
        .max_connections(max_connections)
        .connect(&database_url)
        .await
        .context("failed to connect to PG")?;
    Ok(pool)
}

#[allow(clippy::too_many_arguments)]
async fn write_indicator_value(
    pool: &PgPool,
    stock_id: &str,
    value_date: NaiveDate,
    timeframe: &str,
    source_core: &str,
    source_version: &str,
    params_hash_hex: &str,
    value_json: &serde_json::Value,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO indicator_values
            (stock_id, value_date, timeframe, source_core, source_version, params_hash, value)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        ON CONFLICT (stock_id, value_date, timeframe, source_core, params_hash)
        DO UPDATE SET
            source_version = EXCLUDED.source_version,
            value          = EXCLUDED.value,
            created_at     = NOW()
        "#,
    )
    .bind(stock_id)
    .bind(value_date)
    .bind(timeframe)
    .bind(source_core)
    .bind(source_version)
    .bind(params_hash_hex)
    .bind(value_json)
    .execute(pool)
    .await
    .context("insert indicator_values failed")?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn write_structural_snapshot(
    pool: &PgPool,
    stock_id: &str,
    snapshot_date: NaiveDate,
    timeframe: Timeframe,
    core_name: &str,
    source_version: &str,
    params_hash_hex: &str,
    snapshot_json: &serde_json::Value,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO structural_snapshots
            (stock_id, snapshot_date, timeframe, core_name,
             source_version, params_hash, snapshot)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        ON CONFLICT (stock_id, snapshot_date, timeframe, core_name, params_hash)
        DO UPDATE SET
            source_version = EXCLUDED.source_version,
            snapshot       = EXCLUDED.snapshot,
            created_at     = NOW()
        "#,
    )
    .bind(stock_id)
    .bind(snapshot_date)
    .bind(timeframe.as_str())
    .bind(core_name)
    .bind(source_version)
    .bind(params_hash_hex)
    .bind(snapshot_json)
    .execute(pool)
    .await
    .context("insert structural_snapshots failed")?;
    Ok(())
}

/// PR-9c batch INSERT 取代 per-event loop:用 UNNEST array bind 一次插入 N row。
/// per-event INSERT 是 stage 5 主要 bottleneck(2M facts × 每 row 1 round-trip
/// = 巨大 IO overhead);batch INSERT 降至 N stocks × N batches round-trip。
/// 8000 row / batch 上限對齊 PG 65535 placeholder 限制(這裡 8 array bind 沒有
/// placeholder 限制,但保留 8000 conservative,避免單筆 query 過大)。
async fn write_facts(pool: &PgPool, facts: &[Fact]) -> Result<u64> {
    if facts.is_empty() {
        return Ok(0);
    }
    const BATCH_SIZE: usize = 4000;
    let mut total_inserted = 0u64;
    for chunk in facts.chunks(BATCH_SIZE) {
        // 對齊 PG UNNEST 8 array bind:每個 array len = chunk.len()
        let stock_ids:      Vec<&str>             = chunk.iter().map(|f| f.stock_id.as_str()).collect();
        let fact_dates:     Vec<chrono::NaiveDate> = chunk.iter().map(|f| f.fact_date).collect();
        let timeframes:     Vec<&str>             = chunk.iter().map(|f| f.timeframe.as_str()).collect();
        let source_cores:   Vec<&str>             = chunk.iter().map(|f| f.source_core.as_str()).collect();
        let source_versions:Vec<&str>             = chunk.iter().map(|f| f.source_version.as_str()).collect();
        let params_hashes:  Vec<&str>             = chunk.iter().map(|f| f.params_hash.as_deref().unwrap_or("")).collect();
        let statements:     Vec<&str>             = chunk.iter().map(|f| f.statement.as_str()).collect();
        let metadatas:      Vec<&serde_json::Value> = chunk.iter().map(|f| &f.metadata).collect();

        let res = sqlx::query(
            r#"
            INSERT INTO facts
                (stock_id, fact_date, timeframe, source_core,
                 source_version, params_hash, statement, metadata)
            SELECT * FROM UNNEST(
                $1::text[], $2::date[], $3::text[], $4::text[],
                $5::text[], $6::text[], $7::text[], $8::jsonb[]
            )
            ON CONFLICT DO NOTHING
            "#,
        )
        .bind(&stock_ids)
        .bind(&fact_dates)
        .bind(&timeframes)
        .bind(&source_cores)
        .bind(&source_versions)
        .bind(&params_hashes)
        .bind(&statements)
        .bind(&metadatas)
        .execute(pool)
        .await
        .context("batch insert facts failed")?;
        total_inserted += res.rows_affected();
    }
    Ok(total_inserted)
}

async fn resolve_stock_list(
    pool: &PgPool,
    stocks: Option<&str>,
    limit: Option<usize>,
    dirty: bool,
) -> Result<Vec<String>> {
    if let Some(s) = stocks {
        let list: Vec<String> = s
            .split(',')
            .map(|x| x.trim().to_string())
            .filter(|x| !x.is_empty())
            .collect();
        if let Some(n) = limit {
            return Ok(list.into_iter().take(n).collect());
        }
        return Ok(list);
    }

    // dirty=true:對齊 silver/orchestrator.py:_fetch_dirty_fwd_stocks
    // 只拉 is_dirty=TRUE 的 stocks(走 PR #20 trigger 維護的 dirty queue)
    let sql = if dirty {
        r#"
        SELECT DISTINCT stock_id
        FROM price_daily_fwd
        WHERE market = 'TW' AND is_dirty = TRUE
        ORDER BY stock_id ASC
        "#
    } else {
        r#"
        SELECT DISTINCT stock_id
        FROM price_daily_fwd
        WHERE market = 'TW'
        ORDER BY stock_id ASC
        "#
    };
    let rows: Vec<(String,)> = sqlx::query_as(sql)
        .fetch_all(pool)
        .await
        .context("query price_daily_fwd distinct stock_id failed")?;
    let mut list: Vec<String> = rows.into_iter().map(|(s,)| s).collect();
    if let Some(n) = limit {
        list.truncate(n);
    }
    Ok(list)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn parse_timeframe(s: &str) -> Result<Timeframe> {
    match s.to_lowercase().as_str() {
        "daily" => Ok(Timeframe::Daily),
        "weekly" => Ok(Timeframe::Weekly),
        "monthly" => Ok(Timeframe::Monthly),
        other => anyhow::bail!("unknown timeframe '{}',expected daily/weekly/monthly", other),
    }
}

/// 從 Output JSON 抽 (stock_id, value_date, timeframe_str)。
/// 處理 ma_core series_by_spec / taiex_core series_by_index 例外:
/// fallback 從巢狀 series 結構拿最後 date。
fn extract_indicator_meta(output_json: &serde_json::Value) -> (String, NaiveDate, String) {
    let stock_id = output_json
        .get("stock_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let timeframe = output_json
        .get("timeframe")
        .and_then(|v| v.as_str())
        .unwrap_or("daily")
        .to_string();

    fn nested_last_date(output_json: &serde_json::Value, key: &str) -> Option<String> {
        output_json
            .get(key)
            .and_then(|v| v.as_array())
            .and_then(|outer| outer.iter().rev().find_map(|first| {
                // 取最後一個 entry,但若該 series 為空則往前找
                first.get("series")
                    .and_then(|s| s.as_array())
                    .and_then(|arr| arr.last())
                    .and_then(|p| p.get("date"))
                    .and_then(|d| d.as_str())
                    .map(String::from)
            }))
    }

    let last_date_str = output_json
        .get("series")
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.last())
        .and_then(|p| p.get("date"))
        .and_then(|d| d.as_str())
        .map(String::from)
        .or_else(|| nested_last_date(output_json, "series_by_spec"))    // ma_core
        .or_else(|| nested_last_date(output_json, "series_by_index"));  // taiex_core
    let last_date_str = last_date_str.as_deref();

    let last_date = last_date_str
        .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
        .unwrap_or_else(|| Utc::now().date_naive());

    (stock_id, last_date, timeframe)
}

fn loader_err_summary(
    core: &str,
    stock_id: &str,
    op: &str,
    e: &anyhow::Error,
) -> CoreRunSummary {
    CoreRunSummary {
        core: core.to_string(),
        stock_id: stock_id.to_string(),
        status: "loader_err".to_string(),
        events: 0,
        iv_written: 0,
        fact_written: 0,
        elapsed_ms: 0,
        error: Some(format!("{}: {:#}", op, e)),
    }
}

#[derive(Debug, Clone, Serialize)]
struct CoreRunSummary {
    core: String,
    stock_id: String,
    status: String,
    events: u64,
    iv_written: u64,
    fact_written: u64,
    elapsed_ms: u64,
    error: Option<String>,
}

impl CoreRunSummary {
    fn err(core: &str, stock_id: &str, msg: String, start: Instant) -> Self {
        Self {
            core: core.to_string(),
            stock_id: stock_id.to_string(),
            status: "err".to_string(),
            events: 0,
            iv_written: 0,
            fact_written: 0,
            elapsed_ms: start.elapsed().as_millis() as u64,
            error: Some(msg),
        }
    }
}

fn print_summary(summary: &[CoreRunSummary], total_elapsed: std::time::Duration, write: bool) {
    use std::collections::BTreeMap;
    println!();
    println!("== run-all summary ==");
    println!(
        "total elapsed: {:.1}s    write={}    rows={}",
        total_elapsed.as_secs_f64(),
        write,
        summary.len()
    );

    let mut by_core: BTreeMap<&str, (u64, u64, u64, u64, u64, u64)> = BTreeMap::new();
    // (ok_count, err_count, total_events, total_iv_written, total_fact_written, total_elapsed_ms)
    for r in summary {
        let entry = by_core.entry(&r.core).or_insert((0, 0, 0, 0, 0, 0));
        if r.status == "ok" {
            entry.0 += 1;
        } else {
            entry.1 += 1;
        }
        entry.2 += r.events;
        entry.3 += r.iv_written;
        entry.4 += r.fact_written;
        entry.5 += r.elapsed_ms;
    }

    println!();
    println!("{:<28} {:>6} {:>6} {:>9} {:>10} {:>10} {:>10}", "core", "ok", "err", "events", "iv_rows", "facts", "elapsed_s");
    println!("{}", "-".repeat(86));
    for (core, (ok, err, events, iv, facts, ms)) in &by_core {
        println!(
            "{:<28} {:>6} {:>6} {:>9} {:>10} {:>10} {:>10.1}",
            core,
            ok,
            err,
            events,
            iv,
            facts,
            *ms as f64 / 1000.0
        );
    }

    let errs: Vec<&CoreRunSummary> = summary.iter().filter(|r| r.status != "ok").collect();
    if !errs.is_empty() {
        println!();
        println!("== errors(前 20)==");
        for r in errs.iter().take(20) {
            println!(
                "  [{}] {} stock={} — {}",
                r.status,
                r.core,
                if r.stock_id.is_empty() { "-" } else { &r.stock_id },
                r.error.as_deref().unwrap_or("(no message)")
            );
        }
        if errs.len() > 20 {
            println!("  ... 其他 {} 條 error 省略", errs.len() - 20);
        }
    }
}
