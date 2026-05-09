// neely_core(P0)— Wave Core
//
// 對齊 m2Spec/oldm2Spec/neely_core.md r2(2026-05-06)。
//
// 已實作(M3 PR-2):
//   - struct / enum 合約定義(§五 §六 §八 §九 — Input / Params / Output / Scenario Forest)
//   - WaveCore trait 實作 + warmup_periods(§16:Daily 500 / Weekly 250 / Monthly 120)
//   - **Stage 1**:Monowave Detection(Pure Close + Wilder ATR-filtered reversal)
//   - **Stage 2**:Rule of Neutrality + Rule of Proportion 標註
//   - compute() 回 partial NeelyCoreOutput:scenario_forest 暫空,
//     monowave_series 已填,diagnostics 含 Stage 1-2 耗時
//
// 留後續 PR:
//   - Stage 3:Bottom-up Candidate Generator(留 PR-3)
//   - Stage 4:Validator R1-R7 / F1-F2 / Z1-Z4 / T1-T10 / W1-W2(留 PR-3)
//   - Stage 5-7:Classifier / Post-Constructive Validator / Complexity Rule(留 PR-4)
//   - Stage 8:Compaction(exhaustive + beam_search fallback)+ Forest 上限保護(留 PR-5)
//   - Stage 9-10:Missing Wave / Emulation / Power Rating / Fibonacci / Triggers + facts(留 PR-6)
//   - inventory 註冊機制(`CoreRegistration`)+ Workflow toml(留 PR-8)
//   - P0 Gate 五檔股票實測 + 校準(留 PR-Gate)
//
// 不外部化 Neely 規則常數(§4.4 / §6.6):Fibonacci 比率、±4% 容差、Power Rating
// 查表全部寫死,不可從 toml 設定。

use std::collections::HashMap;
use std::time::Instant;

use anyhow::Result;
use chrono::NaiveDate;
use fact_schema::{Fact, Timeframe, WaveCore};

use crate::output::TimeRange;

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
        // 隨 spec / 演算法版本變動。M3 PR-2 partial(Stage 1-2 落地)階段 0.2.0,
        // 等 P0 Gate 五檔實測通過再 bump 到 1.0.0(spec §17.1 範例為 "1.0.0")。
        "0.2.0"
    }

    fn compute(&self, input: &Self::Input, params: Self::Params) -> Result<Self::Output> {
        // M3 PR-2:Stage 1-2 已實作。Stage 3-10 仍留後續 PR,
        // scenario_forest 暫回空 vec(對齊「無 confirmed scenario」狀態,
        // diagnostics.stage_elapsed_ms 標明只跑到 Stage 2)。
        let cfg = &params.engine_config;
        let mut stage_elapsed: HashMap<String, u64> = HashMap::new();
        let total_start = Instant::now();

        // ── Stage 1:Monowave Detection(Pure Close + ATR-filtered)
        let stage_1_start = Instant::now();
        let raw_monowaves = monowave::detect_monowaves(&input.bars, cfg.atr_period);
        stage_elapsed.insert(
            "stage_1_monowave".to_string(),
            stage_1_start.elapsed().as_millis() as u64,
        );

        // ── Stage 2:Rule of Neutrality + Rule of Proportion
        let stage_2_start = Instant::now();
        let classified = monowave::classify_monowaves(
            &input.bars,
            raw_monowaves,
            &input.stock_id,
            cfg,
        );
        stage_elapsed.insert(
            "stage_2_classify".to_string(),
            stage_2_start.elapsed().as_millis() as u64,
        );

        // ── Stage 3-10 留後續 PR(scenario_forest 暫空,候選 / Validator 不跑)

        let monowave_series: Vec<_> = classified.iter().map(|c| c.monowave.clone()).collect();
        let elapsed_ms = total_start.elapsed().as_millis() as u64;
        let warmup = self.warmup_periods(&params);

        let data_range = if input.bars.is_empty() {
            // 空輸入:給 placeholder NaiveDate(min)— compute 不應對空輸入做有意義計算
            let placeholder = NaiveDate::from_ymd_opt(1900, 1, 1).expect("static date");
            TimeRange { start: placeholder, end: placeholder }
        } else {
            TimeRange {
                start: input.bars.first().unwrap().date,
                end: input.bars.last().unwrap().date,
            }
        };

        Ok(NeelyCoreOutput {
            stock_id: input.stock_id.clone(),
            timeframe: input.timeframe,
            data_range,
            // Stage 8 才會產出。M3 PR-2 階段永遠空 vec
            scenario_forest: Vec::new(),
            monowave_series,
            diagnostics: NeelyDiagnostics {
                monowave_count: classified.len(),
                stage_elapsed_ms: stage_elapsed,
                elapsed_ms,
                ..Default::default()
            },
            rule_book_references: Vec::new(),
            insufficient_data: input.bars.len() < warmup,
        })
    }

    fn produce_facts(&self, _output: &Self::Output) -> Vec<Fact> {
        // §15 Fact 產出規則於 facts.rs 實作,留 PR-6 補完
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
        assert_eq!(core.version(), "0.2.0");
    }

    // -------------------------------------------------------------
    // Partial compute()(M3 PR-2:跑到 Stage 2)
    // -------------------------------------------------------------

    use crate::output::{MonowaveDirection, OhlcvBar};

    fn bar(d: &str, o: f64, h: f64, l: f64, c: f64) -> OhlcvBar {
        OhlcvBar {
            date: chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").unwrap(),
            open: o,
            high: h,
            low: l,
            close: c,
            volume: None,
        }
    }

    #[test]
    fn compute_empty_input_returns_placeholder_output() {
        let core = NeelyCore::new();
        let input = OhlcvSeries {
            stock_id: "2330".to_string(),
            timeframe: Timeframe::Daily,
            bars: vec![],
        };
        let out = core.compute(&input, NeelyCoreParams::default()).unwrap();
        assert_eq!(out.stock_id, "2330");
        assert_eq!(out.monowave_series.len(), 0);
        assert_eq!(out.scenario_forest.len(), 0);
        assert_eq!(out.diagnostics.monowave_count, 0);
        assert!(out.insufficient_data, "0 bars < warmup 500 → insufficient");
        assert!(out.diagnostics.stage_elapsed_ms.contains_key("stage_1_monowave"));
        assert!(out.diagnostics.stage_elapsed_ms.contains_key("stage_2_classify"));
    }

    #[test]
    fn compute_clean_zigzag_yields_three_classified_monowaves() {
        let core = NeelyCore::new();
        let input = OhlcvSeries {
            stock_id: "2330".to_string(),
            timeframe: Timeframe::Daily,
            bars: vec![
                bar("2026-01-01", 10.0, 10.5, 9.5, 10.0),
                bar("2026-01-02", 10.0, 11.5, 10.0, 11.0),
                bar("2026-01-03", 11.0, 13.0, 11.0, 13.0),
                bar("2026-01-04", 13.0, 13.0, 11.5, 11.5),
                bar("2026-01-05", 11.5, 11.5, 9.0, 9.0),
                bar("2026-01-06", 9.0, 11.0, 9.0, 11.0),
                bar("2026-01-07", 11.0, 13.5, 11.0, 13.5),
            ],
        };
        let out = core.compute(&input, NeelyCoreParams::default()).unwrap();
        assert_eq!(out.monowave_series.len(), 3);
        assert!(matches!(out.monowave_series[0].direction, MonowaveDirection::Up));
        assert!(matches!(out.monowave_series[1].direction, MonowaveDirection::Down));
        assert!(matches!(out.monowave_series[2].direction, MonowaveDirection::Up));
        // PR-2 階段 forest 必空(Stage 8 才產出)
        assert_eq!(out.scenario_forest.len(), 0);
        // 7 bars < warmup 500 → 仍標 insufficient(預期行為)
        assert!(out.insufficient_data);
        // data_range 對齊輸入第一 / 最後一筆
        assert_eq!(
            out.data_range.start,
            chrono::NaiveDate::parse_from_str("2026-01-01", "%Y-%m-%d").unwrap()
        );
        assert_eq!(
            out.data_range.end,
            chrono::NaiveDate::parse_from_str("2026-01-07", "%Y-%m-%d").unwrap()
        );
    }
}
