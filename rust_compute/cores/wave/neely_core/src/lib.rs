// neely_core(P0)— Wave Core
//
// 對齊 m2Spec/oldm2Spec/neely_core.md r2(2026-05-06)。
//
// 已實作(M3 PR-6 為止 — Stage 1-10 Pipeline 走通):
//   - struct / enum 合約定義(§五 §六 §八 §九 — Input / Params / Output / Scenario Forest)
//   - WaveCore trait 實作 + warmup_periods(§16:Daily 500 / Weekly 250 / Monthly 120)
//   - **Stage 1**:Monowave Detection(Pure Close + Wilder ATR-filtered reversal)
//   - **Stage 2**:Rule of Neutrality + Rule of Proportion 標註
//   - **Stage 3**:Bottom-up Candidate Generator(滑窗 wave_count ∈ {3,5} + alternation filter + beam_width cap)
//   - **Stage 4**:Validator framework + R1/R2/R3 完整實作(R4-R7 + F/Z/T/W 22 條 Deferred)
//   - **Stage 5**:Classifier 基本邏輯(5wave + R3 → Impulse/Diagonal;3wave → Zigzag Single)
//   - **Stage 6**:Post-Constructive Validator skeleton(預設 pattern_complete = true)
//   - **Stage 7**:Complexity Rule 篩選(差距 ≤ 1 級為 anchor)
//   - **Stage 8**:Compaction 簡化版 pass-through + Forest 上限保護(BeamSearchFallback by power_rating)
//   - **Stage 9a**:Missing Wave 偵測 skeleton(預設 false)
//   - **Stage 9b**:Emulation 辨識 skeleton(預設 false)
//   - **Stage 10a**:Power Rating 查表(Impulse Up→Bullish / Down→Bearish 等 best-guess)
//   - **Stage 10b**:Fibonacci 投影 framework + ratios.rs 寫死 NEELY_FIB_RATIOS(10 個比率)+
//     project_from_w1() helper(完整接 monowave price 留 PR-6b)
//   - **Stage 10c**:Invalidation Triggers 生成(Impulse R1/R3 derived;Diagonal R1 derived;
//     對齊 §9.4 OnTriggerAction:WeakenScenario 取代 ReduceProbability)
//   - **produce_facts()**:每 Scenario 1 條結構性 Fact + 1 條 forest summary Fact
//     (對齊 §6.1.1 機械式陳述,禁主觀詞彙)
//   - compute() 回 完整 NeelyCoreOutput:scenario_forest 含 power_rating / triggers /
//     fib_zones,diagnostics 含 13 個 stage 耗時 + 各種 flags
//
// 留後續 PR:
//   - Stage 4-7 完整規則細節(留 PR-3c / PR-4b,需 user 在 m3Spec/ 寫最新 neely_core spec 後 batch 補)
//   - Stage 8 進階:exhaustive 窮舉合法 compression paths(留 PR-5b)
//   - Stage 9-10 完整:Missing Wave / Emulation / Fibonacci 接 monowave price /
//     Power Rating 完整查表(留 PR-6b)
//   - PG 連接:`shared/ohlcv_loader/` + `tw_cores` binary 接 PG + alembic 落地三表(留 PR-7)
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

// inventory 註冊(對齊 m2Spec/oldm2Spec/cores_overview.md §五 Monolithic Binary 部署模型)
inventory::submit! {
    core_registry::CoreRegistration::new(
        "neely_core",
        "0.7.0",
        core_registry::CoreKind::Wave,
        "P0",
        "Neely Wave Core(NEoWave 規則,Stage 1-10 Pipeline + Scenario Forest)",
    )
}

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
        // 隨 spec / 演算法版本變動。M3 PR-6 partial(Stage 1-10 落地,Power Rating /
        // Fibonacci / Triggers 基本實作 + facts.rs produce_facts)階段 0.7.0,
        // 等 P0 Gate 五檔實測通過再 bump 到 1.0.0(spec §17.1 範例為 "1.0.0")。
        "0.7.0"
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

        // ── Stage 3:Bottom-up Candidate Generator(M3 PR-3a)
        let stage_3_start = Instant::now();
        let wave_candidates = candidates::generate_candidates(&classified, cfg);
        stage_elapsed.insert(
            "stage_3_candidates".to_string(),
            stage_3_start.elapsed().as_millis() as u64,
        );

        // ── Stage 4:Validator R1-R7 / F1-F2 / Z1-Z4 / T1-T10 / W1-W2(M3 PR-3b)
        let stage_4_start = Instant::now();
        let validation_reports = validator::validate_all(&wave_candidates, &classified);
        stage_elapsed.insert(
            "stage_4_validator".to_string(),
            stage_4_start.elapsed().as_millis() as u64,
        );

        let validator_pass_count = validation_reports.iter().filter(|r| r.overall_pass).count();
        let validator_reject_count = validation_reports.len().saturating_sub(validator_pass_count);
        let mut all_rejections: Vec<_> = Vec::new();
        for report in &validation_reports {
            for rej in &report.failed {
                all_rejections.push(rej.clone());
            }
        }

        // ── Stage 5:Classifier(M3 PR-4)
        let stage_5_start = Instant::now();
        let mut scenarios: Vec<_> = wave_candidates
            .iter()
            .zip(validation_reports.iter())
            .filter_map(|(cand, rep)| classifier::classify(cand, rep, &classified))
            .collect();
        stage_elapsed.insert(
            "stage_5_classifier".to_string(),
            stage_5_start.elapsed().as_millis() as u64,
        );

        // ── Stage 6:Post-Constructive Validator(M3 PR-4 skeleton)
        let stage_6_start = Instant::now();
        scenarios.retain(|s| post_validator::post_validate(s).pattern_complete);
        stage_elapsed.insert(
            "stage_6_post_validator".to_string(),
            stage_6_start.elapsed().as_millis() as u64,
        );

        // ── Stage 7:Complexity Rule 篩選(M3 PR-4)
        let stage_7_start = Instant::now();
        scenarios = complexity::apply_complexity_rule(scenarios);
        stage_elapsed.insert(
            "stage_7_complexity".to_string(),
            stage_7_start.elapsed().as_millis() as u64,
        );

        // ── Stage 8:Compaction(M3 PR-5,簡化 pass-through + Forest 上限保護)
        let stage_8_start = Instant::now();
        let compaction_result = compaction::compact(scenarios, cfg);
        stage_elapsed.insert(
            "stage_8_compaction".to_string(),
            stage_8_start.elapsed().as_millis() as u64,
        );
        let mut forest = compaction_result.forest;

        // ── Stage 9a:Missing Wave 偵測(M3 PR-6 skeleton)
        let stage_9a_start = Instant::now();
        let _ = missing_wave::apply_to_forest(&forest);
        stage_elapsed.insert(
            "stage_9a_missing_wave".to_string(),
            stage_9a_start.elapsed().as_millis() as u64,
        );

        // ── Stage 9b:Emulation 辨識(M3 PR-6 skeleton)
        let stage_9b_start = Instant::now();
        let _ = forest.iter().map(emulation::detect_emulation).count();
        stage_elapsed.insert(
            "stage_9b_emulation".to_string(),
            stage_9b_start.elapsed().as_millis() as u64,
        );

        // ── Stage 10a:Power Rating 查表(M3 PR-6)
        let stage_10a_start = Instant::now();
        power_rating::apply_to_forest(&mut forest);
        stage_elapsed.insert(
            "stage_10a_power_rating".to_string(),
            stage_10a_start.elapsed().as_millis() as u64,
        );

        // ── Stage 10b:Fibonacci 投影(M3 PR-6 skeleton — projection 留 PR-6b 接 monowave price)
        let stage_10b_start = Instant::now();
        fibonacci::apply_to_forest(&mut forest);
        stage_elapsed.insert(
            "stage_10b_fibonacci".to_string(),
            stage_10b_start.elapsed().as_millis() as u64,
        );

        // ── Stage 10c:Invalidation Triggers 生成(M3 PR-6 best-guess)
        let stage_10c_start = Instant::now();
        triggers::apply_to_forest(&mut forest);
        stage_elapsed.insert(
            "stage_10c_triggers".to_string(),
            stage_10c_start.elapsed().as_millis() as u64,
        );

        let monowave_series: Vec<_> = classified.iter().map(|c| c.monowave.clone()).collect();
        let forest_size = forest.len();
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
            scenario_forest: forest,
            monowave_series,
            diagnostics: NeelyDiagnostics {
                monowave_count: classified.len(),
                candidate_count: wave_candidates.len(),
                validator_pass_count,
                validator_reject_count,
                rejections: all_rejections,
                forest_size,
                compaction_paths: compaction_result.compaction_paths,
                overflow_triggered: compaction_result.overflow_triggered,
                compaction_timeout: compaction_result.timeout_triggered,
                stage_timings_ms: stage_elapsed,
                elapsed_ms,
                ..Default::default()
            },
            rule_book_references: Vec::new(),
            insufficient_data: input.bars.len() < warmup,
        })
    }

    fn produce_facts(&self, output: &Self::Output) -> Vec<Fact> {
        // §15 Fact 產出規則 — M3 PR-6 basic 實作:每 Scenario 1 條 + 1 條 forest summary
        facts::produce(output)
    }

    fn warmup_periods(&self, params: &Self::Params) -> usize {
        // §16:Daily 500 / Weekly 250 / Monthly 120 / Quarterly 60(2026-05-10 加 Quarterly)
        match params.timeframe {
            Timeframe::Daily => 500,
            Timeframe::Weekly => 250,
            Timeframe::Monthly => 120,
            Timeframe::Quarterly => 60,
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
        assert_eq!(core.version(), "0.7.0");
    }

    // -------------------------------------------------------------
    // Partial compute()(M3 PR-3b:跑到 Stage 4)
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
        assert_eq!(out.diagnostics.candidate_count, 0);
        assert_eq!(out.diagnostics.validator_pass_count, 0);
        assert_eq!(out.diagnostics.validator_reject_count, 0);
        assert!(out.diagnostics.rejections.is_empty());
        assert_eq!(out.diagnostics.forest_size, 0);
        assert!(out.insufficient_data, "0 bars < warmup 500 → insufficient");
        for stage_key in &[
            "stage_1_monowave",
            "stage_2_classify",
            "stage_3_candidates",
            "stage_4_validator",
            "stage_5_classifier",
            "stage_6_post_validator",
            "stage_7_complexity",
            "stage_8_compaction",
            "stage_9a_missing_wave",
            "stage_9b_emulation",
            "stage_10a_power_rating",
            "stage_10b_fibonacci",
            "stage_10c_triggers",
        ] {
            assert!(
                out.diagnostics.stage_timings_ms.contains_key(*stage_key),
                "stage timing key '{}' 應存在",
                stage_key
            );
        }
        // Compaction 旗標
        assert!(!out.diagnostics.overflow_triggered, "空輸入不應 overflow");
        assert!(!out.diagnostics.compaction_timeout);
    }

    #[test]
    fn produce_facts_returns_facts_for_non_empty_forest() {
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
        let facts = core.produce_facts(&out);
        // forest 至少 1 個 → produce_facts 至少 2 條(1 scenario + 1 summary)
        if !out.scenario_forest.is_empty() {
            assert!(facts.len() >= 2);
            assert!(facts.iter().all(|f| f.source_core == "neely_core"));
            assert!(facts.iter().all(|f| f.stock_id == "2330"));
        }
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
        // PR-4 階段 Stage 8 簡化:scenarios 直接成 forest(過 Stage 5-7 篩選)
        assert!(
            out.scenario_forest.len() <= 1,
            "U-D-U 3 monowaves 最多 1 個 scenario"
        );
        // Stage 3 candidate generator:3 個 alternating monowave → 1 個 wave_count=3 candidate
        assert_eq!(
            out.diagnostics.candidate_count, 1,
            "U-D-U 3 monowaves 應生 1 個 wave_count=3 candidate"
        );
        assert_eq!(
            out.diagnostics.validator_pass_count + out.diagnostics.validator_reject_count,
            1,
            "1 candidate 應跑完 1 個 validator report"
        );
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
