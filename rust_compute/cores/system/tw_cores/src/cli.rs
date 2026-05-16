// cli.rs — CLI parsing(從 main.rs v3.5 R4 C8 抽出)

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "tw_cores",
    version,
    about = "Cores 層 Monolithic Binary"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// 列出已連結的 cores(對齊 inventory CoreRegistry::discover)
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
    /// 全市場 × 全 cores production run
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
        /// 跳過 environment cores(只跑 stock-level)
        #[arg(long, default_value_t = false)]
        skip_market: bool,
        /// 跳過 stock-level cores(只跑 environment)
        #[arg(long, default_value_t = false)]
        skip_stock: bool,
        /// 寫入 PG(indicator_values + structural_snapshots + facts)— 不指定僅 dry-run
        #[arg(long, default_value_t = false)]
        write: bool,
        /// Stage B per-stock 並行度(預設 32,需 ≤ PG max_connections - 4 buffer)
        #[arg(long, default_value_t = 32)]
        concurrency: usize,
        /// 只跑 dirty queue(對齊 silver/orchestrator.py:_fetch_dirty_fwd_stocks pattern)。
        /// 與 --stocks 互斥;與 --limit 可疊加
        #[arg(long, default_value_t = false)]
        dirty: bool,
        /// Workflow toml 路徑(動態決定跑哪些 cores)
        #[arg(long)]
        workflow: Option<PathBuf>,
    },
}
