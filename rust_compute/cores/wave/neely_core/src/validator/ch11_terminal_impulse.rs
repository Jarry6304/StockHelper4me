// ch11_terminal_impulse.rs — Ch11 Terminal Impulse Wave-by-Wave 變體規則(advisory mode)
//
// 對齊 m3Spec/neely_rules.md 第 11 章 line 2138-2189 + m3Spec/neely_core_architecture.md
// §9.3(RuleId::Ch11_Terminal_WaveByWave { ext, wave })。
//
// **v4.3b 落地**(2026-05-19):
//   - Terminal Impulse 原 Elliott 稱 Diagonal Triangle,結構序列同 Triangle (3-3-3-3-3)
//     但遵守 Impulse Essential Rules(spec 2140)
//   - 對應 `NeelyPatternType::Diagonal { sub_kind: DiagonalKind }`
//   - Advisory mode:對齊 NEoWave 原作 Ch11 = pattern characteristic 非 invariant constraint
//
// **覆蓋變體**(每變體核心規則,完整 spec 細化留 V4.x):
//   - 1st Wave Extended Terminal:wave-2/3/5/4 規則(spec 2142-2150)
//   - 1st Wave Non-Extended:wave-2 寬鬆(spec 2151-2154)
//   - 3rd Wave Extended:wave-3/2/4/5 規則(spec 2156-2164,罕中之罕)
//   - 5th Wave Extended:wave-5/3/4 規則 + Expanding Running Triangle 區別(spec 2166-2179)
//   - 5th Wave Non-Extended:wave-5/4 規則(spec 2181-2187)
//
// **與 Trending Impulse 主要差異**:
//   - Terminal 1st Ext W2 上限 61.8%(Trending 1st Ext 是 38.2%)
//   - Terminal 各 wave 為 :3 結構(Trending 為 :5)
//   - Terminal 通常為 c-wave of correction(spec 2154)

use crate::advanced_rules::scenario_monowaves;
use crate::monowave::ClassifiedMonowave;
use crate::output::{
    AdvisoryFinding, AdvisorySeverity, ImpulseExtension, NeelyPatternType, RuleId, Scenario,
    WaveNumber,
};

const APPROX_TOL: f64 = 0.10;
const FIB_TOL: f64 = 0.04;

/// Stage 7.5 入口:對 5-wave Diagonal scenario 跑 Ch11 Terminal Impulse 變體規則。
pub fn analyze(scenario: &Scenario, classified: &[ClassifiedMonowave]) -> Vec<AdvisoryFinding> {
    let mut findings = Vec::new();
    if !matches!(scenario.pattern_type, NeelyPatternType::Diagonal { .. }) {
        return findings;
    }
    let waves = scenario_monowaves(scenario, classified);
    if waves.len() < 5 {
        return findings;
    }

    let extension = detect_extension(&waves);

    match extension {
        ImpulseExtension::First => analyze_first_ext_terminal(&waves, &mut findings),
        ImpulseExtension::Third => analyze_third_ext_terminal(&waves, &mut findings),
        ImpulseExtension::Fifth => analyze_fifth_ext_terminal(&waves, &mut findings),
        ImpulseExtension::NonExtended => {
            // 5th Wave Non-Extended Terminal(spec 2181)— W5 短於 W3
            analyze_fifth_non_ext_terminal(&waves, &mut findings);
        }
    }

    findings
}

/// 從 5-wave magnitudes 推 Extension 位置。
fn detect_extension(waves: &[ClassifiedMonowave]) -> ImpulseExtension {
    let w1 = waves[0].metrics.magnitude;
    let w3 = waves[2].metrics.magnitude;
    let w5 = waves[4].metrics.magnitude;
    let max_mag = w1.max(w3).max(w5);
    if max_mag <= 1e-12 {
        return ImpulseExtension::NonExtended;
    }
    let extension_threshold = 1.236;
    // 三段比例接近時走 NonExtended
    if w1 / max_mag >= 1.0 / extension_threshold
        && w3 / max_mag >= 1.0 / extension_threshold
        && w5 / max_mag >= 1.0 / extension_threshold
    {
        return ImpulseExtension::NonExtended;
    }
    if (w1 - max_mag).abs() < 1e-9 {
        ImpulseExtension::First
    } else if (w3 - max_mag).abs() < 1e-9 {
        ImpulseExtension::Third
    } else {
        ImpulseExtension::Fifth
    }
}

// ── 1st Wave Extended Terminal(spec 2142-2150)──────────────────────────

fn analyze_first_ext_terminal(
    waves: &[ClassifiedMonowave],
    findings: &mut Vec<AdvisoryFinding>,
) {
    let w1 = waves[0].metrics.magnitude;
    let w2 = waves[1].metrics.magnitude;
    let w3 = waves[2].metrics.magnitude;
    let w4 = waves[3].metrics.magnitude;
    let w5 = waves[4].metrics.magnitude;

    if w1 <= 1e-12 || w2 <= 1e-12 {
        return;
    }

    // Rule: W2 ≤ 61.8% × W1(Terminal 1st Ext 寬鬆,spec 2145)
    let w2_retrace = w2 / w1;
    if w2_retrace > 0.618 * (1.0 + FIB_TOL) {
        findings.push(finding(
            ImpulseExtension::First,
            WaveNumber::W2,
            AdvisorySeverity::Warning,
            format!(
                "Ch11 Terminal 1st-Ext W2 retrace = {:.1}% × W1 > 61.8%(spec line 2145:Terminal 1st Ext 上限比 Trending 寬鬆但仍 ≤ 61.8%)",
                w2_retrace * 100.0
            ),
        ));
    } else {
        findings.push(finding(
            ImpulseExtension::First,
            WaveNumber::W2,
            AdvisorySeverity::Info,
            format!(
                "Ch11 Terminal 1st-Ext W2 = {:.1}% × W1 ≤ 61.8%(spec line 2145)",
                w2_retrace * 100.0
            ),
        ));
    }

    // Rule: W3 ≥ 38.2% × W1 + 典型 38.2-61.8%(spec 2146)
    let w3_to_w1 = w3 / w1;
    if w3_to_w1 < 0.382 * (1.0 - FIB_TOL) {
        findings.push(finding(
            ImpulseExtension::First,
            WaveNumber::W3,
            AdvisorySeverity::Warning,
            format!(
                "Ch11 Terminal 1st-Ext W3 = {:.1}% × W1 < 38.2%(spec line 2146 下限)",
                w3_to_w1 * 100.0
            ),
        ));
    }
    if w3_to_w1 > 1.0 {
        findings.push(finding(
            ImpulseExtension::First,
            WaveNumber::W3,
            AdvisorySeverity::Warning,
            format!(
                "Ch11 Terminal 1st-Ext W3 = {:.1}% × W1 > 100%(spec line 2146:不太大於 W1)",
                w3_to_w1 * 100.0
            ),
        ));
    }

    // Rule: W5 ≤ 99% × W3,常 38.2-61.8% × W3(spec 2147)
    if w3 > 1e-12 {
        let w5_to_w3 = w5 / w3;
        if w5_to_w3 > 0.99 * (1.0 + APPROX_TOL) {
            findings.push(finding(
                ImpulseExtension::First,
                WaveNumber::W5,
                AdvisorySeverity::Warning,
                format!(
                    "Ch11 Terminal 1st-Ext W5 = {:.1}% × W3 > 99%(spec line 2147:Terminal W5 上限)",
                    w5_to_w3 * 100.0
                ),
            ));
        } else if (0.382..=0.618).contains(&w5_to_w3) {
            findings.push(finding(
                ImpulseExtension::First,
                WaveNumber::W5,
                AdvisorySeverity::Info,
                format!(
                    "Ch11 Terminal 1st-Ext W5 = {:.1}% × W3 落在典型 38.2-61.8%(spec line 2147)",
                    w5_to_w3 * 100.0
                ),
            ));
        }
    }

    // Rule: W4 ≈ 61.8% × W2 價(spec 2148)
    let w4_to_w2 = w4 / w2;
    let near_618 = (w4_to_w2 - 0.618).abs() <= FIB_TOL;
    if near_618 {
        findings.push(finding(
            ImpulseExtension::First,
            WaveNumber::W4,
            AdvisorySeverity::Info,
            format!(
                "Ch11 Terminal 1st-Ext W4/W2 = {:.3} ≈ 0.618(spec line 2148 典型)",
                w4_to_w2
            ),
        ));
    }
}

// ── 3rd Wave Extended Terminal(spec 2156-2164,罕中之罕)──────────────

fn analyze_third_ext_terminal(
    waves: &[ClassifiedMonowave],
    findings: &mut Vec<AdvisoryFinding>,
) {
    let w1 = waves[0].metrics.magnitude;
    let w2 = waves[1].metrics.magnitude;
    let w3 = waves[2].metrics.magnitude;
    let w4 = waves[3].metrics.magnitude;
    let w5 = waves[4].metrics.magnitude;

    if w1 <= 1e-12 {
        return;
    }

    // Rule: W3 不可太大於 W1(spec 2158)— 設 ≤ 161.8% × W1
    let w3_to_w1 = w3 / w1;
    if w3_to_w1 > 1.618 * (1.0 + FIB_TOL) {
        findings.push(finding(
            ImpulseExtension::Third,
            WaveNumber::W3,
            AdvisorySeverity::Warning,
            format!(
                "Ch11 Terminal 3rd-Ext W3 = {:.1}% × W1 > 161.8%(spec line 2158:Terminal 3rd Ext W3 不可太大)",
                w3_to_w1 * 100.0
            ),
        ));
    }

    // Rule: W2 必回測 > 61.8% × W1(spec 2159)
    let w2_retrace = w2 / w1;
    if w2_retrace < 0.618 * (1.0 - FIB_TOL) {
        findings.push(finding(
            ImpulseExtension::Third,
            WaveNumber::W2,
            AdvisorySeverity::Warning,
            format!(
                "Ch11 Terminal 3rd-Ext W2 retrace = {:.1}% × W1 < 61.8%(spec line 2159:必回測 > 61.8%)",
                w2_retrace * 100.0
            ),
        ));
    }

    // Rule: W4 ≤ 38.2% × W3(preferably less,spec 2160)
    if w3 > 1e-12 {
        let w4_to_w3 = w4 / w3;
        if w4_to_w3 > 0.382 * (1.0 + FIB_TOL) {
            findings.push(finding(
                ImpulseExtension::Third,
                WaveNumber::W4,
                AdvisorySeverity::Warning,
                format!(
                    "Ch11 Terminal 3rd-Ext W4 = {:.1}% × W3 > 38.2%(spec line 2160:preferably less)",
                    w4_to_w3 * 100.0
                ),
            ));
        }

        // Rule: W5 ≤ 61.8% × W3(spec 2163)
        let w5_to_w3 = w5 / w3;
        if w5_to_w3 > 0.618 * (1.0 + FIB_TOL) {
            findings.push(finding(
                ImpulseExtension::Third,
                WaveNumber::W5,
                AdvisorySeverity::Warning,
                format!(
                    "Ch11 Terminal 3rd-Ext W5 = {:.1}% × W3 > 61.8%(spec line 2163)",
                    w5_to_w3 * 100.0
                ),
            ));
        }
    }
}

// ── 5th Wave Extended Terminal(spec 2166-2179)──────────────────────────

fn analyze_fifth_ext_terminal(
    waves: &[ClassifiedMonowave],
    findings: &mut Vec<AdvisoryFinding>,
) {
    let w1 = waves[0].metrics.magnitude;
    let w3 = waves[2].metrics.magnitude;
    let w4 = waves[3].metrics.magnitude;
    let w5 = waves[4].metrics.magnitude;

    if w1 <= 1e-12 {
        return;
    }

    // Rule: W3 ≤ 161.8% × W1(spec 2178)
    let w3_to_w1 = w3 / w1;
    if w3_to_w1 > 1.618 * (1.0 + FIB_TOL) {
        findings.push(finding(
            ImpulseExtension::Fifth,
            WaveNumber::W3,
            AdvisorySeverity::Warning,
            format!(
                "Ch11 Terminal 5th-Ext W3 = {:.1}% × W1 > 161.8%(spec line 2178)",
                w3_to_w1 * 100.0
            ),
        ));
    }

    // Rule: W5 ≥ 100% × (W1 + W3)(spec 2177)
    let w1_w3_sum = w1 + w3;
    if w5 < w1_w3_sum * (1.0 - APPROX_TOL) {
        findings.push(finding(
            ImpulseExtension::Fifth,
            WaveNumber::W5,
            AdvisorySeverity::Warning,
            format!(
                "Ch11 Terminal 5th-Ext W5 = {:.2} < (W1 + W3) = {:.2}(spec line 2177:5th-Ext 下限)",
                w5, w1_w3_sum
            ),
        ));
    }

    // Rule: W4 ≥ 50% × W3,可達 99%(spec 2179)
    if w3 > 1e-12 {
        let w4_to_w3 = w4 / w3;
        if w4_to_w3 < 0.5 * (1.0 - APPROX_TOL) {
            findings.push(finding(
                ImpulseExtension::Fifth,
                WaveNumber::W4,
                AdvisorySeverity::Info,
                format!(
                    "Ch11 Terminal 5th-Ext W4 = {:.1}% × W3 < 50%(spec line 2179 典型 ≥ 50%)",
                    w4_to_w3 * 100.0
                ),
            ));
        }
    }
}

// ── 5th Wave Non-Extended Terminal(spec 2181-2187)──────────────────────

fn analyze_fifth_non_ext_terminal(
    waves: &[ClassifiedMonowave],
    findings: &mut Vec<AdvisoryFinding>,
) {
    let w2 = waves[1].metrics.magnitude;
    let w3 = waves[2].metrics.magnitude;
    let w4 = waves[3].metrics.magnitude;
    let w5 = waves[4].metrics.magnitude;

    if w3 <= 1e-12 {
        return;
    }

    // Rule: W5 ≤ 61.8% × W3(spec 2183)
    let w5_to_w3 = w5 / w3;
    if w5_to_w3 > 0.618 * (1.0 + FIB_TOL) {
        findings.push(finding(
            ImpulseExtension::NonExtended,
            WaveNumber::W5,
            AdvisorySeverity::Warning,
            format!(
                "Ch11 Terminal 5th Non-Ext W5 = {:.1}% × W3 > 61.8%(spec line 2183)",
                w5_to_w3 * 100.0
            ),
        ));
    }

    // Rule: W4 < W2 (時/價,spec 2187)
    if w4 >= w2 {
        findings.push(finding(
            ImpulseExtension::NonExtended,
            WaveNumber::W4,
            AdvisorySeverity::Warning,
            format!(
                "Ch11 Terminal 5th Non-Ext W4 = {:.2} ≥ W2 = {:.2}(spec line 2187:W4 < W2)",
                w4, w2
            ),
        ));
    }
}

// ── Helper ───────────────────────────────────────────────────────────────

fn finding(
    ext: ImpulseExtension,
    wave: WaveNumber,
    severity: AdvisorySeverity,
    message: String,
) -> AdvisoryFinding {
    AdvisoryFinding {
        rule_id: RuleId::Ch11_Terminal_WaveByWave { ext, wave },
        severity,
        message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::monowave::ProportionMetrics;
    use crate::output::*;
    use chrono::NaiveDate;

    fn cmw(mag: f64, dur: usize, day_offset: i64) -> ClassifiedMonowave {
        let base = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        ClassifiedMonowave {
            monowave: Monowave {
                start_date: base + chrono::Duration::days(day_offset),
                end_date: base + chrono::Duration::days(day_offset + dur as i64 - 1),
                start_price: 100.0,
                end_price: 100.0 + mag,
                direction: MonowaveDirection::Up,
                bar_indices: (0, 0),
            },
            atr_at_start: 1.0,
            metrics: ProportionMetrics {
                magnitude: mag,
                duration_bars: dur,
                atr_relative: 1.0,
                slope_vs_45deg: 1.0,
            },
            structure_label_candidates: Vec::new(),
            polywave_size: 0,
        }
    }

    fn make_scenario_diagonal(classified: &[ClassifiedMonowave]) -> Scenario {
        Scenario {
            id: "test".to_string(),
            wave_tree: WaveNode {
                label: "test".to_string(),
                start: classified.first().unwrap().monowave.start_date,
                end: classified.last().unwrap().monowave.end_date,
                children: Vec::new(),
            },
            pattern_type: NeelyPatternType::Diagonal {
                sub_kind: DiagonalKind::Ending,
            },
            initial_direction: MonowaveDirection::Up,
            compacted_base_label: StructureLabel::Five,
            structure_label: "test".to_string(),
            complexity_level: ComplexityLevel::Simple,
            power_rating: PowerRating::Neutral,
            max_retracement: None,
            post_pattern_behavior: PostBehavior::Unconstrained,
            passed_rules: Vec::new(),
            deferred_rules: Vec::new(),
            rules_passed_count: 0,
            deferred_rules_count: 0,
            invalidation_triggers: Vec::new(),
            expected_fib_zones: Vec::new(),
            structural_facts: StructuralFacts::default(),
            advisory_findings: Vec::new(),
            in_triangle_context: false,
            awaiting_l_label: false,
            monowave_structure_labels: Vec::new(),
            round_state: RoundState::Round1,
            pattern_isolation_anchors: Vec::new(),
            triplexity_detected: false,
        }
    }

    #[test]
    fn no_findings_for_non_diagonal() {
        let waves = vec![
            cmw(10.0, 5, 0),
            cmw(5.0, 3, 5),
            cmw(15.0, 5, 8),
            cmw(7.0, 4, 13),
            cmw(12.0, 5, 17),
        ];
        let mut scenario = make_scenario_diagonal(&waves);
        scenario.pattern_type = NeelyPatternType::Impulse;
        let findings = analyze(&scenario, &waves);
        assert!(findings.is_empty());
    }

    #[test]
    fn terminal_1st_ext_w2_under_61_8_is_info() {
        // W1=30, W2=15 (50%) → Info (≤ 61.8%);spec line 2145
        let waves = vec![
            cmw(30.0, 5, 0),
            cmw(15.0, 3, 5),
            cmw(15.0, 5, 8),
            cmw(8.0, 4, 13),
            cmw(10.0, 5, 17),
        ];
        let scenario = make_scenario_diagonal(&waves);
        let findings = analyze(&scenario, &waves);
        let w2_info = findings.iter().find(|f| {
            matches!(
                f.rule_id,
                RuleId::Ch11_Terminal_WaveByWave {
                    ext: ImpulseExtension::First,
                    wave: WaveNumber::W2,
                }
            )
        });
        assert!(w2_info.is_some());
        assert!(matches!(w2_info.unwrap().severity, AdvisorySeverity::Info));
    }

    #[test]
    fn terminal_1st_ext_w2_over_61_8_warns() {
        // W1=30, W2=25 (83%) → Warning;spec line 2145
        let waves = vec![
            cmw(30.0, 5, 0),
            cmw(25.0, 3, 5),
            cmw(15.0, 5, 8),
            cmw(8.0, 4, 13),
            cmw(10.0, 5, 17),
        ];
        let scenario = make_scenario_diagonal(&waves);
        let findings = analyze(&scenario, &waves);
        let w2_warn = findings.iter().any(|f| {
            matches!(
                f.rule_id,
                RuleId::Ch11_Terminal_WaveByWave {
                    ext: ImpulseExtension::First,
                    wave: WaveNumber::W2,
                }
            ) && matches!(f.severity, AdvisorySeverity::Warning)
                && f.message.contains("61.8")
        });
        assert!(w2_warn);
    }

    #[test]
    fn terminal_3rd_ext_w2_too_shallow_warns() {
        // W1=10, W2=4 (40%) - W3=20 (Third Ext) → W2 < 61.8% warns
        let waves = vec![
            cmw(10.0, 5, 0),
            cmw(4.0, 3, 5),
            cmw(20.0, 8, 8),
            cmw(5.0, 4, 16),
            cmw(8.0, 5, 20),
        ];
        let scenario = make_scenario_diagonal(&waves);
        let findings = analyze(&scenario, &waves);
        let w2_warn = findings.iter().any(|f| {
            matches!(
                f.rule_id,
                RuleId::Ch11_Terminal_WaveByWave {
                    ext: ImpulseExtension::Third,
                    wave: WaveNumber::W2,
                }
            ) && matches!(f.severity, AdvisorySeverity::Warning)
                && f.message.contains("> 61.8%")
        });
        assert!(w2_warn);
    }

    #[test]
    fn terminal_5th_ext_w5_below_w1_w3_sum_warns() {
        // W1=8, W3=12, W5=15 (W5 < W1+W3 = 20) → Warning
        let waves = vec![
            cmw(8.0, 5, 0),
            cmw(5.0, 3, 5),
            cmw(12.0, 5, 8),
            cmw(8.0, 4, 13),
            cmw(15.0, 8, 17),
        ];
        let scenario = make_scenario_diagonal(&waves);
        let findings = analyze(&scenario, &waves);
        let w5_warn = findings.iter().any(|f| {
            matches!(
                f.rule_id,
                RuleId::Ch11_Terminal_WaveByWave {
                    ext: ImpulseExtension::Fifth,
                    wave: WaveNumber::W5,
                }
            ) && matches!(f.severity, AdvisorySeverity::Warning)
                && f.message.contains("W1 + W3")
        });
        assert!(w5_warn);
    }

    #[test]
    fn terminal_5th_non_ext_w5_over_61_8_warns() {
        // 3 segments similar → NonExtended:W1=10, W3=12, W5=10。
        // W5 = 10 > 61.8% × W3 = 7.4 → Warning
        let waves = vec![
            cmw(10.0, 5, 0),
            cmw(5.0, 3, 5),
            cmw(12.0, 5, 8),
            cmw(6.0, 4, 13),
            cmw(10.0, 5, 17),
        ];
        let scenario = make_scenario_diagonal(&waves);
        let findings = analyze(&scenario, &waves);
        let w5_warn = findings.iter().any(|f| {
            matches!(
                f.rule_id,
                RuleId::Ch11_Terminal_WaveByWave {
                    ext: ImpulseExtension::NonExtended,
                    wave: WaveNumber::W5,
                }
            ) && matches!(f.severity, AdvisorySeverity::Warning)
        });
        assert!(w5_warn);
    }

    #[test]
    fn terminal_5th_non_ext_w4_ge_w2_warns() {
        // 3 段相似 → NonExtended:W1=10, W3=10.5, W5=10.5。
        // W2=4, W4=8 (≥ W2) → Warning
        let waves = vec![
            cmw(10.0, 5, 0),
            cmw(4.0, 3, 5),
            cmw(10.5, 5, 8),
            cmw(8.0, 4, 13),
            cmw(10.5, 5, 17),
        ];
        let scenario = make_scenario_diagonal(&waves);
        let findings = analyze(&scenario, &waves);
        let w4_warn = findings.iter().any(|f| {
            matches!(
                f.rule_id,
                RuleId::Ch11_Terminal_WaveByWave {
                    ext: ImpulseExtension::NonExtended,
                    wave: WaveNumber::W4,
                }
            ) && matches!(f.severity, AdvisorySeverity::Warning)
                && f.message.contains("W4 < W2")
        });
        assert!(w4_warn);
    }
}
