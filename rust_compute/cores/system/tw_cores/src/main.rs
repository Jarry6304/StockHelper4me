// tw_cores — Cores 層 Monolithic Binary 入口
//
// 對齊 m2Spec/oldm2Spec/cores_overview.md §五(Monolithic Binary 部署模型)
//   - P0 / P1 / P2 一律單一 binary,inventory 自動註冊
//   - 改任一 Core 重編全部(實測 ~5 分鐘可接受)
//   - 無法 hot-fix 單一 Core,但台股一日一交易、batch 模式下沒有此需求
//
// 本 PR(M3 PR-1)範圍:純 skeleton,只 print 已連結的 Core 列表並退出
//
// 留後續 PR(對應 cores_overview §七 寫入分流):
//   - Stage 1-4 Pipeline 編排(orchestrator crate)
//   - Silver 層 loader 對接(`shared/ohlcv_loader/` 等)
//   - 寫入 `indicator_values` / `structural_snapshots` / `facts` 三表
//   - inventory `CoreRegistration` + `CoreRegistry::discover`

use anyhow::Result;
use clap::Parser;
use fact_schema::WaveCore;
use neely_core::NeelyCore;

#[derive(Parser, Debug)]
#[command(
    name = "tw_cores",
    version,
    about = "Cores 層 Monolithic Binary(M3 skeleton)"
)]
struct Cli {
    /// 列出已連結的 Core(skeleton 階段唯一支援的子命令)
    #[arg(long, default_value_t = true)]
    list_cores: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let _cli = Cli::parse();

    println!("== M3 cores binary(skeleton + Stage 1-8 partial)==");
    println!("workspace = rust_compute/(virtual root)");
    println!();
    println!("Linked cores:");

    let neely = NeelyCore::new();
    println!(
        "  - {} v{} (Wave Core, P0, Stage 1-8 partial impl)",
        neely.name(),
        neely.version()
    );

    println!();
    println!(
        "Stage 1: Pure Close + Wilder ATR-filtered monowave detection ✅"
    );
    println!(
        "Stage 2: Rule of Neutrality + Rule of Proportion             ✅"
    );
    println!(
        "Stage 3: Bottom-up Candidate Generator                       ✅"
    );
    println!(
        "Stage 4: Validator R1-R3 完整 + R4-R7/F/Z/T/W 22 條 Deferred  🟡"
    );
    println!(
        "Stage 5: Classifier(Impulse vs Diagonal / Zigzag 基本)       🟡"
    );
    println!(
        "Stage 6: Post-Constructive Validator skeleton                🟡"
    );
    println!(
        "Stage 7: Complexity Rule(差距 ≤ 1 級篩選)                    ✅"
    );
    println!(
        "Stage 8: Compaction(簡化 pass-through + Forest 上限保護)     🟡"
    );
    println!(
        "Stage 9-10: Missing Wave / Power Rating / Fibonacci / facts   ⏳ 後續 PR"
    );
    println!();
    println!(
        "(對齊 m2Spec/oldm2Spec/neely_core.md §七 Stage 1-10 Pipeline)"
    );

    Ok(())
}
