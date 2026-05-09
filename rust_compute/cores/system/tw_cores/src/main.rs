// tw_cores — Cores 層 Monolithic Binary 入口
//
// 對齊 m2Spec/oldm2Spec/cores_overview.md §五(Monolithic Binary 部署模型)
//   - P0 / P1 / P2 一律單一 binary,inventory 自動註冊(留 PR-8)
//   - 改任一 Core 重編全部(實測 ~5 分鐘可接受)
//   - 無法 hot-fix 單一 Core,但台股一日一交易、batch 模式下沒有此需求
//
// **M3 PR-7 範圍**:
//   - PG 連線 + ohlcv_loader 讀 Silver `price_*_fwd`
//   - 跑 NeelyCore.compute() 產 Forest + Facts
//   - 寫 `structural_snapshots` + `facts` 兩表
//   - CLI:--list-cores / --run --stock-id 2330 [--timeframe daily] [--write]
//
// 留後續 PR:
//   - inventory `CoreRegistration` + `CoreRegistry::discover`(留 PR-8)
//   - Workflow toml 編排(留 PR-8)
//   - indicator_values 表寫入(留 P1 後 IndicatorCore 上線時)

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use fact_schema::{Timeframe, WaveCore};
use neely_core::{NeelyCore, NeelyCoreParams};
use sqlx::postgres::{PgPool, PgPoolOptions};

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
    /// 列出已連結的 Core(skeleton 階段唯一支援)
    ListCores,
    /// 對指定 stock 跑 neely_core 完整 Stage 1-10 Pipeline
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
}

#[tokio::main]
async fn main() -> Result<()> {
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
        } => run_neely(&stock_id, &timeframe, write).await,
    }
}

fn list_cores() -> Result<()> {
    println!("== M3 cores binary(Stage 1-10 partial,Pipeline 走通)==");
    println!("workspace = rust_compute/(virtual root)");
    println!();

    // 從 inventory 自動發現所有編譯期註冊的 Core(對齊 cores_overview §五)
    let registry = core_registry::CoreRegistry::discover();
    println!("Linked cores(via inventory CoreRegistry):");
    for core in registry.cores() {
        println!(
            "  - {} v{} [{:?} / {}] — {}",
            core.name, core.version, core.kind, core.priority, core.description
        );
    }

    // 確保 dep crate 的 inventory::submit! 不被 dead-code 剃掉
    let _ = NeelyCore::new();
    let _ = day_trading_core::DayTradingCore::new();

    println!();
    println!("Stage 1: Pure Close + Wilder ATR-filtered monowave detection ✅");
    println!("Stage 2: Rule of Neutrality + Rule of Proportion             ✅");
    println!("Stage 3: Bottom-up Candidate Generator                       ✅");
    println!("Stage 4: Validator R1-R3 完整 + R4-R7/F/Z/T/W 22 條 Deferred  🟡");
    println!("Stage 5: Classifier(Impulse vs Diagonal / Zigzag 基本)       🟡");
    println!("Stage 6: Post-Constructive Validator skeleton                🟡");
    println!("Stage 7: Complexity Rule(差距 ≤ 1 級篩選)                    ✅");
    println!("Stage 8: Compaction(簡化 pass-through + Forest 上限保護)     🟡");
    println!("Stage 9: Missing Wave / Emulation skeleton                   🟡");
    println!("Stage 10: Power Rating + Fibonacci ratios + Triggers(基本)  🟡");
    println!("Facts:    produce_facts() 每 scenario 1 條 + forest summary  ✅");
    println!("PG IO:    ohlcv_loader 讀 Silver / 寫 snapshots+facts         ✅ PR-7");
    println!("Inventory: CoreRegistration + CoreRegistry::discover         ✅ PR-8");
    println!("Workflow: toml 範例 workflows/tw_stock_standard.toml(orchestrator dispatch 留 PR-9)");
    println!();
    println!("(對齊 m2Spec/oldm2Spec/neely_core.md §七 Stage 1-10 Pipeline)");
    Ok(())
}

async fn run_neely(stock_id: &str, timeframe: &str, write: bool) -> Result<()> {
    let tf = match timeframe.to_lowercase().as_str() {
        "daily" => Timeframe::Daily,
        "weekly" => Timeframe::Weekly,
        "monthly" => Timeframe::Monthly,
        other => anyhow::bail!("unknown timeframe '{}',expected daily/weekly/monthly", other),
    };

    let database_url = std::env::var("DATABASE_URL")
        .context("DATABASE_URL not set — set environment or .env before running")?;

    tracing::info!(stock_id, ?tf, "connecting to PG");
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await
        .context("failed to connect to PG")?;

    // Load OHLCV from Silver
    let mut params = NeelyCoreParams::default();
    params.timeframe = tf;
    let series = ohlcv_loader::load_for_neely(&pool, stock_id, &params).await?;

    tracing::info!(
        stock_id,
        bars = series.bars.len(),
        "loaded OHLCV from Silver price_*_fwd"
    );

    // Run Stage 1-10
    let core = NeelyCore::new();
    let output = core.compute(&series, params)?;

    tracing::info!(
        stock_id,
        forest_size = output.scenario_forest.len(),
        candidates = output.diagnostics.candidate_count,
        validator_pass = output.diagnostics.validator_pass_count,
        validator_reject = output.diagnostics.validator_reject_count,
        elapsed_ms = output.diagnostics.elapsed_ms,
        "compute() done"
    );

    // Produce facts
    let facts = core.produce_facts(&output);
    tracing::info!(facts_count = facts.len(), "produced facts");

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
    println!("compaction paths:   {}", output.diagnostics.compaction_paths);
    println!("forest size:        {}", output.scenario_forest.len());
    println!("overflow_triggered: {}", output.diagnostics.overflow_triggered);
    println!("elapsed_ms:         {}", output.diagnostics.elapsed_ms);
    println!("facts produced:     {}", facts.len());

    if write {
        write_outputs(&pool, &output, &facts).await?;
    } else {
        println!();
        println!("(dry-run — 加 --write 落 structural_snapshots + facts)");
    }

    Ok(())
}

async fn write_outputs(
    pool: &PgPool,
    output: &neely_core::NeelyCoreOutput,
    facts: &[fact_schema::Fact],
) -> Result<()> {
    let snapshot_json = serde_json::to_value(output).context("serialize NeelyCoreOutput")?;

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
    .bind(&output.stock_id)
    .bind(output.data_range.end)
    .bind(output.timeframe.as_str())
    .bind("neely_core")
    .bind("0.7.0")
    .bind("") // PR-8 改 NeelyCoreParams 的 params_hash
    .bind(&snapshot_json)
    .execute(pool)
    .await
    .context("insert structural_snapshots failed")?;

    let mut inserted = 0;
    for fact in facts {
        let metadata_json = serde_json::to_value(&fact.metadata).unwrap_or(serde_json::json!({}));
        let res = sqlx::query(
            r#"
            INSERT INTO facts
                (stock_id, fact_date, timeframe, source_core,
                 source_version, params_hash, statement, metadata)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT DO NOTHING
            "#,
        )
        .bind(&fact.stock_id)
        .bind(fact.fact_date)
        .bind(fact.timeframe.as_str())
        .bind(&fact.source_core)
        .bind(&fact.source_version)
        .bind(fact.params_hash.as_deref().unwrap_or(""))
        .bind(&fact.statement)
        .bind(&metadata_json)
        .execute(pool)
        .await
        .context("insert facts failed")?;
        inserted += res.rows_affected();
    }

    println!();
    println!("== Wrote to PG ==");
    println!("structural_snapshots: 1 row UPSERT");
    println!("facts:                {}/{} new (others ON CONFLICT DO NOTHING)", inserted, facts.len());

    Ok(())
}
