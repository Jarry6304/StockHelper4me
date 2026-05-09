// neely_core(P0)— Wave Core skeleton
//
// 對齊 m3Spec/neely_core.md r2(2026-05-06)。
//
// 本 PR(M3 PR-1)範圍:
//   - struct / enum 合約定義(§五 §六 §八 §九 — Input / Params / Output / Scenario Forest)
//   - WaveCore trait 實作骨架(`compute()` 回 `unimplemented!`,`warmup_periods()` 對齊 §16)
//   - 14 個 sub-module 全部用空 mod 預留(§三 模組組成)
//
// 留後續 PR:
//   - Stage 1-10 Pipeline 實作(monowave / candidates / validator / ...)
//   - inventory 註冊機制(`CoreRegistration`)
//   - golden test / P0 Gate 五檔股票實測
//
// 不外部化 Neely 規則常數(§4.4 / §6.6):Fibonacci 比率、±4% 容差、Power Rating
// 查表全部寫死,不可從 toml 設定。

use anyhow::Result;
use fact_schema::{Fact, Timeframe, WaveCore};

pub mod candidates;
pub mod classifier;
pub mod compaction;
pub mod complexity;
pub mod config;
pub mod degree;
pub mod emulation;
pub mod facts;
pub mod fibonacci;
pub mod missing_wave;
pub mod monowave;
pub mod output;
pub mod post_validator;
pub mod power_rating;
pub mod triggers;
pub mod validator;

pub use config::{NeelyCoreParams, NeelyEngineConfig, OverflowStrategy};
pub use output::{NeelyCoreOutput, NeelyDiagnostics, OhlcvSeries};

pub struct NeelyCore;

impl NeelyCore {
    pub fn new() -> Self {
        NeelyCore
    }
}

impl Default for NeelyCore {
    fn default() -> Self {
        NeelyCore::new()
    }
}

impl WaveCore for NeelyCore {
    type Input = OhlcvSeries;
    type Params = NeelyCoreParams;
    type Output = NeelyCoreOutput;

    fn name(&self) -> &'static str {
        "neely_core"
    }

    fn version(&self) -> &'static str {
        // 隨 spec / 演算法版本變動。M3 PR-1 skeleton 階段固定 0.1.0,
        // 等 P0 Gate 五檔實測通過再 bump 到 1.0.0(spec §17.1 範例為 "1.0.0")。
        "0.1.0"
    }

    fn compute(&self, _input: &Self::Input, _params: Self::Params) -> Result<Self::Output> {
        // M3 PR-1 是純 skeleton:struct 合約 + trait 簽章先落地,
        // Stage 1-10 Pipeline 各 sub-module(monowave / validator / ...)留後續 PR 實作。
        // 直接 unimplemented! 比假裝回空 forest 更老實 — 上層誤呼叫會立刻爆,
        // 不會誤以為 neely 已經 work 但 forest 永遠是空的。
        unimplemented!(
            "neely_core::compute — M3 PR-1 skeleton, Stage 1-10 Pipeline 留後續 PR 實作"
        )
    }

    fn produce_facts(&self, _output: &Self::Output) -> Vec<Fact> {
        // §15 Fact 產出規則於 facts.rs 實作,本 skeleton 階段先回空 vec
        Vec::new()
    }

    fn warmup_periods(&self, params: &Self::Params) -> usize {
        // §16:Daily 500 / Weekly 250 / Monthly 120
        match params.timeframe {
            Timeframe::Daily => 500,
            Timeframe::Weekly => 250,
            Timeframe::Monthly => 120,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warmup_periods_matches_spec_section_16() {
        let core = NeelyCore::new();
        assert_eq!(
            core.warmup_periods(&NeelyCoreParams {
                timeframe: Timeframe::Daily,
                engine_config: NeelyEngineConfig::default(),
            }),
            500
        );
        assert_eq!(
            core.warmup_periods(&NeelyCoreParams {
                timeframe: Timeframe::Weekly,
                engine_config: NeelyEngineConfig::default(),
            }),
            250
        );
        assert_eq!(
            core.warmup_periods(&NeelyCoreParams {
                timeframe: Timeframe::Monthly,
                engine_config: NeelyEngineConfig::default(),
            }),
            120
        );
    }

    #[test]
    fn name_and_version_are_stable() {
        let core = NeelyCore::new();
        assert_eq!(core.name(), "neely_core");
        assert_eq!(core.version(), "0.1.0");
    }
}
