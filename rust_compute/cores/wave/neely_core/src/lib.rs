// neely_core(P0)— Wave Core
//
// 對齊 m3Spec/neely_core_architecture.md r5(2026-05-13)+ m3Spec/neely_rules.md
//       + m3Spec/cores_overview.md r4
//
// 已實作(Phase 1 PR — r5 spec alignment baseline):
//   - struct / enum 合約定義(architecture §5 §6 §8 §9 — Input / Params / Output / Scenario Forest)
//   - WaveCore trait 實作 + warmup_periods(architecture §13:Daily 500 / Weekly 250 / Monthly 120)
//   - **RuleId 章節編碼**(architecture §9.3)— 取代 r4 自編號;Phase 1 範圍宣告 Ch5_* + Engineering_*
//   - **Stage 1**:Monowave Detection — **Hybrid OHLC ((H+L)/2 mid_price)** + Wilder ATR-filtered reversal
//   - **Stage 2**:Rule of Neutrality(個股 ATR / TAIEX %)+ Rule of Proportion metrics
//   - **Stage 3**:Bottom-up Candidate Generator(滑窗 wave_count ∈ {3,5} + alternation filter + beam_width cap)
//   - **Stage 4**:Validator framework + Ch5 Essential R1-R7 完整 + Ch5_Overlap_Trending + Ch5_Overlap_Terminal
//                  完整;Ch5_Flat/Zigzag/Triangle/Equality/Alternation 9 條 Deferred(留 P4-P7)
//   - **Stage 5**:Classifier(5wave + Overlap_Trending pass / Overlap_Terminal fail → Impulse;
//                  Trending fail / Terminal pass → Diagonal { Leading/Ending heuristic };3wave → Zigzag Single)
//   - **Stage 6**:Post-Constructive Validator skeleton(預設 pattern_complete = true,留 P6)
//   - **Stage 7**:Complexity Rule 篩選(差距 ≤ 1 級為 anchor)
//   - **Stage 8**:Compaction 簡化版 pass-through + Forest 上限保護(BeamSearchFallback by power_rating)
//   - **Stage 9a**:Missing Wave 偵測 skeleton(預設 false,留 P9)
//   - **Stage 9b**:Emulation 辨識 skeleton(預設 false,留 P9)
//   - **Stage 10a**:Power Rating 查表(Impulse Up→Bullish / Down→Bearish 等 best-guess;留 P10 完整查表)
//   - **Stage 10b**:Fibonacci 投影 framework + ratios.rs 寫死 NEELY_FIB_RATIOS
//   - **Stage 10c**:Invalidation Triggers(Impulse Ch5_Essential(3) + Ch5_Overlap_Trending derived;
//                  Diagonal Ch5_Essential(3) derived)
//   - **produce_facts()**:每 Scenario 1 條結構性 Fact + 1 條 forest summary
//
// 留後續 PR(完整 r5 spec roadmap 詳見 plan file 多 PR roadmap):
//   - **P2**:Stage 0 Pre-Constructive Logic(~200 branch if-else,Ch3 Rule 1-7 × Cond × Cat)
//   - **P3**:Stage 3.5 Pattern Isolation + Zigzag DETOUR Test
//   - **P4**:Stage 4 Flat/Zigzag/Triangle 變體規則完整實作
//   - **P5**:Stage 5 Ch8 Complex Polywaves + Ch10 Power Ratings
//   - **P6**:Stage 6/7 Ch6 Post-Constructive + Ch7 Compaction Three Rounds
//   - **P7**:Stage 7.5 Channeling + Ch9 Advanced Rules
//   - **P8**:Stage 8 Compaction Three Rounds 遞迴改造
//   - **P9**:Stage 9 Missing Wave + Emulation 完整
//   - **P10**:Stage 10 完整 + Ch12 Fibonacci Internal/External + Waterfall
//   - **P11**:Stage 10.5 Reverse Logic
//   - **P12**:Stage 11/12 Degree Ceiling + cross_timeframe_hints
//   - **P13**:P0 Gate 六檔實測 + 校準
//
// 不外部化 Neely 規則常數(architecture §4.5 / §6.6):Fibonacci 比率、±4%/±5%/±10% 三檔容差、
// Power Rating 查表全部寫死,不可從 toml 設定。
//
// ## RuleId 範圍說明(v0.20.0 落地)
//
// 對齊 cores_overview §四(禁止抽象)+ §十四(prematurely declare 未實際 dispatch
// 的 RuleId 不該做),`RuleId` enum 限縮為三組 — 實際會 dispatch 進 RuleRejection:
//   - `Ch5_*` Essential / Overlap_Trending / Overlap_Terminal / Equality / Alternation /
//             Flat_* / Zigzag_* / Triangle_* / Channeling_*
//   - `Ch9_*` TrendlineTouchpoints / TimeRule / Independent / Simultaneous /
//             Exception_Aspect{1,2} / StructureIntegrity
//   - `Engineering_*` InsufficientData / ForestOverflow / CompactionTimeout
//
// 其他章節的規則「結果」改用 **domain-specific enums / fields**,不再宣告對應的 RuleId variant:
//   - Ch3 Pre-Constructive Logic → `StructureLabel` candidates(寫入 ClassifiedMonowave)
//   - Ch4 Three Rounds         → `Scenario.compacted_base_label` + `in_triangle_context`
//   - Ch6 Post-Constructive    → `pattern_complete: bool`(Stage 6 過濾 forest)
//   - Ch7 Compaction Reassessment → `Scenario.compacted_base_label`(:5 / :3)
//   - Ch8 Complex Polywaves    → `CombinationKind` enum 11 variants
//   - Ch10 Power Ratings       → `PowerRating` enum 7-level table
//   - Ch11 Wave-by-Wave 變體   → 直接寫進 RuleRejection.gap(無 RuleId variant)
//   - Ch12 Missing Wave / Emulation / Reverse Logic / Fibonacci →
//     `MissingWaveSuspect` / `EmulationKind` / `ReverseLogicObservation` / `FibZone`
//
// P0 Gate 後若 production SQL 需「按 Chapter 統計拒絕原因」,再批量補 missing variants。

use std::collections::HashMap;
use std::time::Instant;

use anyhow::Result;
use chrono::NaiveDate;
use fact_schema::{Fact, Timeframe, WaveCore};

use crate::output::TimeRange;

pub mod advanced_rules;
pub mod candidates;
pub mod classifier;
pub mod compaction;
pub mod complexity;
pub mod config;
pub mod cross_timeframe;
pub mod degree;
pub mod emulation;
pub mod facts;
pub mod fibonacci;
pub mod missing_wave;
pub mod monowave;
pub mod output;
pub mod pattern_isolation;
pub mod post_validator;
pub mod power_rating;
pub mod pre_constructive;
pub mod reverse_logic;
pub mod three_rounds;
pub mod triggers;
pub mod validator;

pub use config::{NeelyCoreParams, NeelyEngineConfig, OverflowStrategy};
pub use output::{NeelyCoreOutput, NeelyDiagnostics, OhlcvSeries};

// inventory 註冊(對齊 m2Spec/oldm2Spec/cores_overview.md §五 Monolithic Binary 部署模型)
inventory::submit! {
    core_registry::CoreRegistration::new(
        "neely_core",
        "0.21.0",
        core_registry::CoreKind::Wave,
        "P0",
        "Neely Wave Core(NEoWave 規則,Hybrid OHLC + Ch3 + Pattern Isolation + Ch5 變體 + Channeling + Ch6 + Ch7 + Ch9 + Ch10 + Three Rounds + Ch8/Ch12 Missing Wave + Ch12 Emulation + Reverse Logic + Degree Ceiling + cross_timeframe_hints)",
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
        // 隨 spec / 演算法版本變動。
        // 0.7.0 → 0.8.0(Phase 1 PR r5 spec alignment baseline:Hybrid OHLC +
        // RuleId 章節編碼重寫 + Ch5 Essential R1-R7 + Overlap_Trending/Terminal 完整)。
        // 0.8.0 → 0.9.0(Phase 2 PR:Stage 0 Ch3 Pre-Constructive Logic
        // 全 200+ branches if-else cascade,StructureLabel candidates 落地)。
        // 0.9.0 → 0.10.0(Phase 3 PR:Stage 3.5 Pattern Isolation 6-step procedure
        // + Zigzag DETOUR Test 落地,NeelyCoreOutput 加 pattern_bounds + detour_annotations)。
        // 0.10.0 → 0.11.0(Phase 4 PR:Stage 4 Ch5 Flat/Zigzag/Triangle/Equality/Alternation
        // 9 條變體規則完整實作,取代 Phase 1 Deferred stubs)。
        // 0.11.0 → 0.12.0(Phase 5 PR:Ch10 Power Rating 完整 r5 表 (±3..±3 7 級) +
        // Scenario.initial_direction 欄位 + Diagonal Leading/Ending heuristic 用相鄰 label context)。
        // 0.12.0 → 0.13.0(Phase 6 PR:Ch6 Post-Constructive 兩階段確認完整實作 +
        // Ch7 Compaction Reassessment base label (Scenario.compacted_base_label 新欄位))。
        // 0.13.0 → 0.14.0(Phase 7 PR:Stage 7.5 Channeling 5 條 trendlines (0-2/1-3/2-4/0-B/B-D) +
        // Ch9 Advanced Rules (Trendline Touchpoints / Time Rule / Exception Aspect 1/2 / Structure Integrity) +
        // Scenario.advisory_findings 新欄位)。
        // 0.14.0 → 0.15.0(Phase 8 PR:Three Rounds nested parent context (Triangle 內部段標 in_triangle_context)
        // + Round 3 暫停偵測 (Scenario.awaiting_l_label + NeelyCoreOutput.round3_pause) +
        // Power Rating 接通 in_triangle_context 例外)。
        // 0.15.0 → 0.16.0(Phase 9 PR:Stage 9a Missing Wave 完整偵測 (從 P2 MissingWaveBundle 萃取
        // + MissingWavePosition 分類) + Stage 9b Ch12 Emulation 完整偵測 (4 種 EmulationKind) +
        // CombinationKind enum 擴 11 variants 對齊 Ch8 Table A/B)。
        // 0.16.0 → 0.17.0(Phase 10 PR:Stage 10b Fibonacci Internal/External 分離投影 (取代
        // compute_expected_fib_zones 回空 vec 的 placeholder) + Stage 10c Triggers 接 monowave price
        // (W1.start_price for Ch5_Essential(3) / W2.end_price for Ch5_Overlap_Trending) +
        // Impulse Up/Down 對稱 PriceBreakBelow/Above 方向)。
        // 0.17.0 → 0.18.0(Phase 11 PR:Stage 10.5 Reverse Logic Rule (Neely Extension) — 多套合法
        // 計數時市場處於某更大形態中段,輸出 ReverseLogicObservation 含 suggested_filter_ids
        // (in_triangle_context / Triangle Limiting / Combination Double* 為中段候選不過濾,
        // Impulse / Diagonal / Triangle Contracting/Expanding / Combination Triple* 為完成候選))。
        // 0.18.0 → 0.19.0(Phase 12 PR:Stage 11 Degree Ceiling 依資料時間跨度推導本次分析能達到
        // 的最高 Degree(11 級體系 SubMicro..GrandSupercycle)+ Stage 12 cross_timeframe_hints
        // 為每 monowave 產 MonowaveSummary(structure_label_candidates / date_range / price_range)
        // 供 Aggregation Layer 跨 Timeframe 比對。NeelyCoreOutput 加 degree_ceiling +
        // cross_timeframe_hints 兩個非選用欄)。
        // 0.19.0 → 0.20.0(Phase 12.5 PR / spec audit alignment:NeelyCoreOutput.compaction_timeout
        // 上提至頂層(對稱 insufficient_data,對齊 spec §8.1「失敗旗標」段)+ NeelyDiagnostics 內
        // 保留雙寫向下相容 + RuleId scope 限縮 Ch5/Ch9/Engineering 三組(其餘章節用 domain-specific
        // enums 取代,對齊 cores_overview §四 禁止抽象 / §十四 prematurely declare 不該做))。
        // 0.20.0 → 0.21.0(P0 Gate v2 production 校準 — 2026-05-14 / 1264 stocks:
        // forest_max_size 1000 → 200(觀察 forest_size max=37 / p99=16 / p95=10,留 5× p99 餘量))。
        // 等 P0 Gate 六檔實測通過再 bump 到 1.0.0。
        "0.21.0"
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
        let mut classified = monowave::classify_monowaves(
            &input.bars,
            raw_monowaves,
            &input.stock_id,
            cfg,
        );
        stage_elapsed.insert(
            "stage_2_classify".to_string(),
            stage_2_start.elapsed().as_millis() as u64,
        );

        // ── Stage 0:Pre-Constructive Logic(Phase 2 PR — Ch3 Rule 1-7 if-else cascade)
        //    對齊 m3Spec/neely_core_architecture.md §7.1 Stage 0
        //    spec 把這 stage 編號為「Stage 0」是 Pipeline 邏輯位置(Stage 2 之後 / Stage 3 之前)
        let stage_0_start = Instant::now();
        pre_constructive::run(&mut classified);
        stage_elapsed.insert(
            "stage_0_preconstructive".to_string(),
            stage_0_start.elapsed().as_millis() as u64,
        );

        // ── Stage 3:Bottom-up Candidate Generator(M3 PR-3a)
        let stage_3_start = Instant::now();
        let wave_candidates = candidates::generate_candidates(&classified, cfg);
        stage_elapsed.insert(
            "stage_3_candidates".to_string(),
            stage_3_start.elapsed().as_millis() as u64,
        );

        // ── Stage 3.5:Pattern Isolation + Zigzag DETOUR Test(Phase 3 PR)
        //    對齊 m3Spec/neely_rules.md §Pattern Isolation Procedures + §Zigzag DETOUR Test
        //    本 stage 為「資訊性」— 結果寫入 NeelyCoreOutput.pattern_bounds /
        //    detour_annotations,Stage 4 Validator 暫不依賴(留 P5+ 串接)
        let stage_3_5_start = Instant::now();
        let pattern_bounds = pattern_isolation::run(&classified);
        let detour_annotations = pattern_isolation::run_detour(&wave_candidates, &classified);
        stage_elapsed.insert(
            "stage_3_5_pattern_isolation".to_string(),
            stage_3_5_start.elapsed().as_millis() as u64,
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

        // ── Stage 6:Post-Constructive Validator(Phase 6 PR — Ch6 兩階段確認)
        //    對齊 m3Spec/neely_rules.md §Ch6(1763-1797 行)
        let stage_6_start = Instant::now();
        scenarios.retain(|s| post_validator::post_validate(s, &classified).pattern_complete);
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

        // ── Stage 7.5:Channeling + Ch9 Advanced Rules(Phase 7 PR)
        //    對齊 m3Spec/neely_rules.md §Ch5 Channeling + §Ch9 Basic Neely Extensions
        //    諮詢性 stage — 寫 AdvisoryFinding 進 scenario.advisory_findings,不過濾 scenarios
        let stage_7_5_start = Instant::now();
        advanced_rules::run(&mut scenarios, &classified);
        stage_elapsed.insert(
            "stage_7_5_advanced_rules".to_string(),
            stage_7_5_start.elapsed().as_millis() as u64,
        );

        // ── Stage 8:Compaction(M3 PR-5,簡化 pass-through + Forest 上限保護)
        let stage_8_start = Instant::now();
        let compaction_result = compaction::compact(scenarios, cfg);
        stage_elapsed.insert(
            "stage_8_compaction".to_string(),
            stage_8_start.elapsed().as_millis() as u64,
        );
        let mut forest = compaction_result.forest;

        // ── Stage 8.5:Three Rounds nested context + Round 3 暫停偵測(Phase 8 PR)
        //    對齊 m3Spec/neely_rules.md §Three Rounds + §Ch10 三角內 Power = 0 例外
        let stage_8_5_start = Instant::now();
        let round3_pause = three_rounds::apply(&mut forest);
        stage_elapsed.insert(
            "stage_8_5_three_rounds".to_string(),
            stage_8_5_start.elapsed().as_millis() as u64,
        );

        // ── Stage 9a:Missing Wave 偵測(Phase 9 PR 完整實作)
        //    對齊 m3Spec/neely_rules.md §Pre-Constructive Logic missing wave 標記慣例(1054-1057 行)
        //    從 Phase 2 已標的 MissingWaveBundle certainty 萃取結構化資訊
        let stage_9a_start = Instant::now();
        let missing_wave_suspects = missing_wave::detect(&classified);
        stage_elapsed.insert(
            "stage_9a_missing_wave".to_string(),
            stage_9a_start.elapsed().as_millis() as u64,
        );

        // ── Stage 9b:Emulation 辨識(Phase 9 PR 完整 Ch12 實作)
        //    對齊 m3Spec/neely_rules.md §Ch8 Running 變體辨識(1902-1906 行)+ §Ch12 Emulation
        //    對 forest 中每 scenario 套 4 種 emulation kind 檢測
        let stage_9b_start = Instant::now();
        let emulation_suspects = emulation::detect_all(&forest, &classified);
        stage_elapsed.insert(
            "stage_9b_emulation".to_string(),
            stage_9b_start.elapsed().as_millis() as u64,
        );

        // 提前構建 monowave_series — Stage 10b/10c 需要從中反查 W1/W2 prices
        let monowave_series: Vec<_> = classified.iter().map(|c| c.monowave.clone()).collect();

        // ── Stage 10a:Power Rating 查表(M3 PR-6)
        let stage_10a_start = Instant::now();
        power_rating::apply_to_forest(&mut forest);
        stage_elapsed.insert(
            "stage_10a_power_rating".to_string(),
            stage_10a_start.elapsed().as_millis() as u64,
        );

        // ── Stage 10b:Fibonacci 投影(Phase 10 — Internal + External 從 monowave price 投影)
        let stage_10b_start = Instant::now();
        fibonacci::apply_to_forest(&mut forest, &monowave_series);
        stage_elapsed.insert(
            "stage_10b_fibonacci".to_string(),
            stage_10b_start.elapsed().as_millis() as u64,
        );

        // ── Stage 10c:Invalidation Triggers 生成(Phase 10 — 從 monowave price 填實際 W1/W2 break level)
        let stage_10c_start = Instant::now();
        triggers::apply_to_forest(&mut forest, &monowave_series);
        stage_elapsed.insert(
            "stage_10c_triggers".to_string(),
            stage_10c_start.elapsed().as_millis() as u64,
        );

        // ── Stage 10.5:Reverse Logic 觀察(Phase 11 — Neely Extension)
        //    對齊 m3Spec/neely_rules.md §Expansion of Possibilities(2598-2608 行)
        //    多套合法計數 → 市場處於更大形態中段
        let stage_10_5_start = Instant::now();
        let reverse_logic_observation = reverse_logic::observe(&forest);
        stage_elapsed.insert(
            "stage_10_5_reverse_logic".to_string(),
            stage_10_5_start.elapsed().as_millis() as u64,
        );

        // ── Stage 11:Degree Ceiling 推導(Phase 12 — architecture §8.5 / §13.3)
        //    依資料時間跨度自動推導本次分析能達到的最高 Degree
        let stage_11_start = Instant::now();
        let degree_ceiling = degree::compute_ceiling(&input.bars, input.timeframe);
        stage_elapsed.insert(
            "stage_11_degree_ceiling".to_string(),
            stage_11_start.elapsed().as_millis() as u64,
        );

        // ── Stage 12:cross_timeframe_hints 計算(Phase 12 — architecture §8.6 / §3.4)
        //    為每個 classified_monowave 產出摘要,供 Aggregation Layer 跨 Timeframe 比對
        let stage_12_start = Instant::now();
        let cross_timeframe_hints = cross_timeframe::compute_hints(&classified, input.timeframe);
        stage_elapsed.insert(
            "stage_12_cross_timeframe".to_string(),
            stage_12_start.elapsed().as_millis() as u64,
        );

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
                stage_elapsed_ms: stage_elapsed,
                elapsed_ms,
                ..Default::default()
            },
            rule_book_references: Vec::new(),
            insufficient_data: input.bars.len() < warmup,
            compaction_timeout: compaction_result.timeout_triggered,
            pattern_bounds,
            detour_annotations,
            round3_pause,
            missing_wave_suspects,
            emulation_suspects,
            reverse_logic_observation,
            degree_ceiling,
            cross_timeframe_hints,
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
        assert_eq!(core.version(), "0.21.0");
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
            "stage_0_preconstructive",
            "stage_3_candidates",
            "stage_3_5_pattern_isolation",
            "stage_4_validator",
            "stage_5_classifier",
            "stage_6_post_validator",
            "stage_7_complexity",
            "stage_7_5_advanced_rules",
            "stage_8_compaction",
            "stage_8_5_three_rounds",
            "stage_9a_missing_wave",
            "stage_9b_emulation",
            "stage_10a_power_rating",
            "stage_10b_fibonacci",
            "stage_10c_triggers",
            "stage_10_5_reverse_logic",
            "stage_11_degree_ceiling",
            "stage_12_cross_timeframe",
        ] {
            assert!(
                out.diagnostics.stage_elapsed_ms.contains_key(*stage_key),
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
