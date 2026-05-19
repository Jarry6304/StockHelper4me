// ch11_trending_impulse.rs — Ch11 Trending Impulse Wave-by-Wave 變體規則(advisory mode)
//
// 對齊 m3Spec/neely_rules.md 第 11 章 line 2071-2137 + m3Spec/neely_core_architecture.md
// §9.3(RuleId::Ch11_Impulse_WaveByWave { ext, wave })。
//
// **v4.3a 落地**(2026-05-19):
//   - Advisory mode:對齊 NEoWave 原作 Ch11 = pattern characteristic 非 invariant constraint
//   - 違反規則 → AdvisoryFinding(Warning 或 Info severity);**不 invalidate scenario**
//   - 對應 RuleId 從 #[allow(dead_code)] 解凍 — 透過 advisory_findings.rule_id 真實 dispatch
//
// **覆蓋變體**(每變體取核心規則 — full spec 完整覆蓋留 V4.x 細化):
//   - 1st Wave Extended:5 條核心規則(spec 2073-2083)
//   - 3rd Wave Extended:6 條核心規則(spec 2090-2097)
//   - 5th Wave Extended:6 條核心規則(spec 2108-2117)
//   - 5th Wave Failure:核心特徵偵測(spec 2119-2129)
//   - Wave-4 共通規則(獨立於 Extension,spec 2131-2134)
//
// **容差**(architecture §4.2):一般近似 ±10%;Fibonacci ±4%

use crate::advanced_rules::scenario_monowaves;
use crate::monowave::ClassifiedMonowave;
use crate::output::{
    AdvisoryFinding, AdvisorySeverity, ImpulseExtension, NeelyPatternType, RuleId, Scenario,
    WaveNumber,
};

const APPROX_TOL: f64 = 0.10;
const FIB_TOL: f64 = 0.04;

/// Stage 7.5 入口:對 5-wave Impulse scenario 跑 Ch11 Trending Impulse 變體規則,
/// 回傳 advisory_findings 列(0 或多個)。
pub fn analyze(scenario: &Scenario, classified: &[ClassifiedMonowave]) -> Vec<AdvisoryFinding> {
    let mut findings = Vec::new();
    if !matches!(scenario.pattern_type, NeelyPatternType::Impulse) {
        return findings;
    }
    let waves = scenario_monowaves(scenario, classified);
    if waves.len() < 5 {
        return findings;
    }

    let extension = detect_extension(&waves);

    match extension {
        ImpulseExtension::First => analyze_first_ext(&waves, &mut findings),
        ImpulseExtension::Third => analyze_third_ext(&waves, &mut findings),
        ImpulseExtension::Fifth => analyze_fifth_ext(&waves, &mut findings),
        ImpulseExtension::NonExtended => {
            // spec 2099-2106:機率向 1st / 5th Extended 偏移;留 V4.x 細化
        }
    }

    check_wave4_common_rule(&waves, &mut findings);
    check_fifth_wave_failure(&waves, extension, &mut findings);

    findings
}

/// 從 5-wave magnitudes 推 Extension 位置(取最長的 W1/W3/W5)。
fn detect_extension(waves: &[ClassifiedMonowave]) -> ImpulseExtension {
    let w1 = waves[0].metrics.magnitude;
    let w3 = waves[2].metrics.magnitude;
    let w5 = waves[4].metrics.magnitude;
    let max_mag = w1.max(w3).max(w5);
    if max_mag <= 1e-12 {
        return ImpulseExtension::NonExtended;
    }
    // 三段比例接近 → NonExtended(任一段 < 1.236× max 視為非延伸)
    let extension_threshold = 1.236;
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

// ── 1st Wave Extended(spec 2073-2083)──────────────────────────────────

fn analyze_first_ext(waves: &[ClassifiedMonowave], findings: &mut Vec<AdvisoryFinding>) {
    let w1 = waves[0].metrics.magnitude;
    let w2 = waves[1].metrics.magnitude;
    let w3 = waves[2].metrics.magnitude;
    let w5 = waves[4].metrics.magnitude;

    if w1 <= 1e-12 {
        return;
    }

    // Rule: W2 ≤ 38.2% × W1(嚴 1st Ext)— spec 2075
    let w2_retrace = w2 / w1;
    findings.push(finding(
        ImpulseExtension::First,
        WaveNumber::W2,
        if w2_retrace > 0.382 * (1.0 + FIB_TOL) {
            AdvisorySeverity::Warning
        } else {
            AdvisorySeverity::Info
        },
        format!(
            "Ch11 1st-Ext W2 retrace = {:.1}% × W1 (spec line 2075:1st Ext 嚴格上限 38.2% ± 4%)",
            w2_retrace * 100.0
        ),
    ));

    // Rule: W3 < W1(W1 為三段最長)且 W3 ≥ 38.2% × W1 — spec 2077
    let w3_ratio = w3 / w1;
    if w3 >= w1 {
        findings.push(finding(
            ImpulseExtension::First,
            WaveNumber::W3,
            AdvisorySeverity::Warning,
            format!(
                "Ch11 1st-Ext W3 = {:.1}% × W1 ≥ 1.0(spec line 2077:W3 必短於 W1 — W1 為三段最長)",
                w3_ratio * 100.0
            ),
        ));
    } else if w3_ratio < 0.382 * (1.0 - FIB_TOL) {
        findings.push(finding(
            ImpulseExtension::First,
            WaveNumber::W3,
            AdvisorySeverity::Warning,
            format!(
                "Ch11 1st-Ext W3 = {:.1}% × W1 < 38.2%(spec line 2077:W3 下限)",
                w3_ratio * 100.0
            ),
        ));
    }

    // Rule: W5 必為 (W1, W3, W5) 中最短 — spec 2081
    if w5 >= w1 || w5 >= w3 {
        findings.push(finding(
            ImpulseExtension::First,
            WaveNumber::W5,
            AdvisorySeverity::Warning,
            format!(
                "Ch11 1st-Ext W5 = {:.2}(W1={:.2} / W3={:.2}) — W5 必為三段中最短(spec line 2081 剛性條件)",
                w5, w1, w3
            ),
        ));
    } else {
        // W5 典型 38.2-61.8% × W3
        let w5_ratio_to_w3 = w5 / w3;
        if w5_ratio_to_w3 >= 0.382 && w5_ratio_to_w3 <= 0.618 {
            findings.push(finding(
                ImpulseExtension::First,
                WaveNumber::W5,
                AdvisorySeverity::Info,
                format!(
                    "Ch11 1st-Ext W5 = {:.1}% × W3 落在典型 38.2-61.8% 區間(spec line 2081)",
                    w5_ratio_to_w3 * 100.0
                ),
            ));
        }
    }
}

// ── 3rd Wave Extended(spec 2090-2097)──────────────────────────────────

fn analyze_third_ext(waves: &[ClassifiedMonowave], findings: &mut Vec<AdvisoryFinding>) {
    let w1 = waves[0].metrics.magnitude;
    let w2 = waves[1].metrics.magnitude;
    let w3 = waves[2].metrics.magnitude;
    let w4 = waves[3].metrics.magnitude;
    let w5 = waves[4].metrics.magnitude;

    if w1 <= 1e-12 || w3 <= 1e-12 {
        return;
    }

    // Rule: W3 > 161.8% × W1 且為三段最長 — spec 2092
    let w3_ratio = w3 / w1;
    if w3_ratio < 1.618 * (1.0 - FIB_TOL) {
        findings.push(finding(
            ImpulseExtension::Third,
            WaveNumber::W3,
            AdvisorySeverity::Warning,
            format!(
                "Ch11 3rd-Ext W3 = {:.1}% × W1 < 161.8%(spec line 2092:W3 必 > 161.8% × W1)",
                w3_ratio * 100.0
            ),
        ));
    }

    // Rule: W2 可回測接近 99%(寬鬆;只 advisory Info) — spec 2094
    let w2_retrace = w2 / w1;
    if w2_retrace > 0.99 + APPROX_TOL {
        findings.push(finding(
            ImpulseExtension::Third,
            WaveNumber::W2,
            AdvisorySeverity::Warning,
            format!(
                "Ch11 3rd-Ext W2 retrace = {:.1}% × W1 > 99%(spec line 2094:W2 寬鬆上限)",
                w2_retrace * 100.0
            ),
        ));
    }

    // Rule: W4 不可回測 > 38.2-50% × W3 — spec 2095
    let w4_retrace = w4 / w3;
    if w4_retrace > 0.5 * (1.0 + APPROX_TOL) {
        findings.push(finding(
            ImpulseExtension::Third,
            WaveNumber::W4,
            AdvisorySeverity::Warning,
            format!(
                "Ch11 3rd-Ext W4 retrace = {:.1}% × W3 > 50%(spec line 2095:預期 ≤ 38.2-50%,超過暗示 W5 Failure)",
                w4_retrace * 100.0
            ),
        ));
    }

    // Rule: W5 約等於 W1,或 61.8% / 161.8% 關係 — spec 2096
    let w5_to_w1 = w5 / w1;
    let equality_match = (w5_to_w1 - 1.0).abs() <= APPROX_TOL
        || (w5_to_w1 - 0.618).abs() <= FIB_TOL
        || (w5_to_w1 - 1.618).abs() <= FIB_TOL;
    if !equality_match {
        findings.push(finding(
            ImpulseExtension::Third,
            WaveNumber::W5,
            AdvisorySeverity::Info,
            format!(
                "Ch11 3rd-Ext W5/W1 = {:.3}(不在 1.0 ± 10% / 0.618 ± 4% / 1.618 ± 4% 任一典型 — spec line 2096)",
                w5_to_w1
            ),
        ));
    }
}

// ── 5th Wave Extended(spec 2108-2117)──────────────────────────────────

fn analyze_fifth_ext(waves: &[ClassifiedMonowave], findings: &mut Vec<AdvisoryFinding>) {
    let w1 = waves[0].metrics.magnitude;
    let w2 = waves[1].metrics.magnitude;
    let w3 = waves[2].metrics.magnitude;
    let w4 = waves[3].metrics.magnitude;
    let w5 = waves[4].metrics.magnitude;

    if w1 <= 1e-12 || w3 <= 1e-12 {
        return;
    }

    // Rule: W3 通常 ≈ 161.8% × W1(典型 Fibonacci,非剛性) — spec 2110
    let w3_to_w1 = w3 / w1;
    let near_161 = (w3_to_w1 - 1.618).abs() <= FIB_TOL;
    findings.push(finding(
        ImpulseExtension::Fifth,
        WaveNumber::W3,
        if near_161 {
            AdvisorySeverity::Info
        } else {
            AdvisorySeverity::Info
        },
        format!(
            "Ch11 5th-Ext W3/W1 = {:.3}(典型 ≈ 1.618,spec line 2110 非剛性)",
            w3_to_w1
        ),
    ));

    // Rule: W1 < W3(W5 為延伸最長,W3 次長) — spec 2111
    if w1 >= w3 {
        findings.push(finding(
            ImpulseExtension::Fifth,
            WaveNumber::W1,
            AdvisorySeverity::Warning,
            format!(
                "Ch11 5th-Ext W1 = {:.2} ≥ W3 = {:.2}(spec line 2111:5th-Ext 預期 W1 < W3 < W5)",
                w1, w3
            ),
        ));
    }

    // Rule: W5 ≥ (W1 + W3) 整段價距 — spec 2112
    let w1_to_w3_span = w1 + w3; // 1-3 advance 全長
    if w5 < w1_to_w3_span * (1.0 - APPROX_TOL) {
        findings.push(finding(
            ImpulseExtension::Fifth,
            WaveNumber::W5,
            AdvisorySeverity::Warning,
            format!(
                "Ch11 5th-Ext W5 = {:.2} < (W1 + W3) = {:.2}(spec line 2112:W5 ≥ 1-3 全長加在 W4 終點)",
                w5, w1_to_w3_span
            ),
        ));
    }

    // Rule: W5 ≤ 261.8% × (W1 + W3) — spec 2113
    let w5_upper_limit = w1_to_w3_span * 2.618;
    if w5 > w5_upper_limit * (1.0 + FIB_TOL) {
        findings.push(finding(
            ImpulseExtension::Fifth,
            WaveNumber::W5,
            AdvisorySeverity::Warning,
            format!(
                "Ch11 5th-Ext W5 = {:.2} > 261.8% × 1-3 全長 = {:.2}(spec line 2113:5th-Ext 最大限)",
                w5, w5_upper_limit
            ),
        ));
    }

    // Rule: W4 為三段中最大者(價、時、複雜度) — spec 2114
    let w2_mag = w2;
    if w4 <= w2_mag {
        findings.push(finding(
            ImpulseExtension::Fifth,
            WaveNumber::W4,
            AdvisorySeverity::Warning,
            format!(
                "Ch11 5th-Ext W4 = {:.2} ≤ W2 = {:.2}(spec line 2114:5th-Ext 預期 W4 > W2 — 5th Ext Alternation 特徵)",
                w4, w2_mag
            ),
        ));
    }

    // Rule: W4 ≥ 50% × W3(典型) — spec 2114
    let w4_to_w3 = w4 / w3;
    if w4_to_w3 < 0.5 * (1.0 - APPROX_TOL) {
        findings.push(finding(
            ImpulseExtension::Fifth,
            WaveNumber::W4,
            AdvisorySeverity::Info,
            format!(
                "Ch11 5th-Ext W4 = {:.1}% × W3(spec line 2114 典型 ≥ 50%)",
                w4_to_w3 * 100.0
            ),
        ));
    }
}

// ── Wave-4 共通規則(spec 2131-2134)──────────────────────────────────

fn check_wave4_common_rule(waves: &[ClassifiedMonowave], findings: &mut Vec<AdvisoryFinding>) {
    let w3 = waves[2].metrics.magnitude;
    let w4 = waves[3].metrics.magnitude;
    if w3 <= 1e-12 {
        return;
    }
    // W4 ≤ 61.8% × W3(獨立於 Extension,除非 W5 將 Extend)
    let w4_retrace = w4 / w3;
    if w4_retrace > 0.618 * (1.0 + FIB_TOL) {
        findings.push(AdvisoryFinding {
            rule_id: RuleId::Ch11_Impulse_WaveByWave {
                ext: ImpulseExtension::NonExtended,
                wave: WaveNumber::W4,
            },
            severity: AdvisorySeverity::Warning,
            message: format!(
                "Ch11 Wave-4 共通規則:W4 retrace = {:.1}% × W3 > 61.8%(spec line 2133:除 5th-Ext 外極罕)",
                w4_retrace * 100.0
            ),
        });
    }
}

// ── 5th Wave Failure 偵測(spec 2119-2129)──────────────────────────────

fn check_fifth_wave_failure(
    waves: &[ClassifiedMonowave],
    extension: ImpulseExtension,
    findings: &mut Vec<AdvisoryFinding>,
) {
    let w4 = waves[3].metrics.magnitude;
    let w5 = waves[4].metrics.magnitude;
    // Failure 定義:W5 短於 W4(spec 2121)
    if w5 < w4 {
        // 限制:Failure 在 3rd Ext 最可能;1st Ext 罕見;5th Ext 不可能 — spec 2126
        let severity = match extension {
            ImpulseExtension::Third => AdvisorySeverity::Strong,
            ImpulseExtension::First => AdvisorySeverity::Warning,
            ImpulseExtension::Fifth => AdvisorySeverity::Warning, // spec 不可能 — 標警示
            ImpulseExtension::NonExtended => AdvisorySeverity::Info,
        };
        findings.push(AdvisoryFinding {
            rule_id: RuleId::Ch11_Impulse_WaveByWave {
                ext: extension,
                wave: WaveNumber::W5,
            },
            severity,
            message: format!(
                "Ch11 5th Wave Failure:W5 = {:.2} < W4 = {:.2}(spec line 2121)— 一般 Failure 只在 3rd Ext 可能;後續同級走勢必完全回測整段(spec 2129)",
                w5, w4
            ),
        });
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
        rule_id: RuleId::Ch11_Impulse_WaveByWave { ext, wave },
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
            },
            atr_at_start: 1.0,
            metrics: ProportionMetrics {
                magnitude: mag,
                duration_bars: dur,
                atr_relative: 1.0,
                slope_vs_45deg: 1.0,
            },
            structure_label_candidates: Vec::new(),
        }
    }

    fn make_scenario_impulse(classified: &[ClassifiedMonowave]) -> Scenario {
        Scenario {
            id: "test".to_string(),
            wave_tree: WaveNode {
                label: "test".to_string(),
                start: classified.first().unwrap().monowave.start_date,
                end: classified.last().unwrap().monowave.end_date,
                children: Vec::new(),
            },
            pattern_type: NeelyPatternType::Impulse,
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
    fn detect_extension_first_when_w1_longest() {
        let waves = vec![
            cmw(30.0, 5, 0),
            cmw(8.0, 3, 5),
            cmw(15.0, 5, 8),
            cmw(5.0, 4, 13),
            cmw(10.0, 5, 17),
        ];
        assert!(matches!(detect_extension(&waves), ImpulseExtension::First));
    }

    #[test]
    fn detect_extension_third_when_w3_longest() {
        let waves = vec![
            cmw(10.0, 5, 0),
            cmw(3.0, 3, 5),
            cmw(20.0, 8, 8),
            cmw(5.0, 4, 16),
            cmw(8.0, 5, 20),
        ];
        assert!(matches!(detect_extension(&waves), ImpulseExtension::Third));
    }

    #[test]
    fn detect_extension_fifth_when_w5_longest() {
        let waves = vec![
            cmw(8.0, 5, 0),
            cmw(3.0, 3, 5),
            cmw(10.0, 5, 8),
            cmw(4.0, 4, 13),
            cmw(20.0, 8, 17),
        ];
        assert!(matches!(detect_extension(&waves), ImpulseExtension::Fifth));
    }

    #[test]
    fn detect_extension_non_extended_when_three_segments_similar() {
        let waves = vec![
            cmw(10.0, 5, 0),
            cmw(3.0, 3, 5),
            cmw(11.0, 5, 8),
            cmw(4.0, 4, 13),
            cmw(10.5, 5, 17),
        ];
        assert!(matches!(detect_extension(&waves), ImpulseExtension::NonExtended));
    }

    #[test]
    fn first_ext_w2_retrace_over_38_2_warns() {
        // W1=30, W2=15 (50%, > 38.2%) → Warning
        let waves = vec![
            cmw(30.0, 5, 0),
            cmw(15.0, 3, 5),
            cmw(15.0, 5, 8),
            cmw(5.0, 4, 13),
            cmw(10.0, 5, 17),
        ];
        let scenario = make_scenario_impulse(&waves);
        let findings = analyze(&scenario, &waves);
        let w2_finding = findings.iter().find(|f| {
            matches!(
                f.rule_id,
                RuleId::Ch11_Impulse_WaveByWave {
                    ext: ImpulseExtension::First,
                    wave: WaveNumber::W2,
                }
            )
        });
        assert!(w2_finding.is_some());
        assert!(matches!(w2_finding.unwrap().severity, AdvisorySeverity::Warning));
    }

    #[test]
    fn first_ext_w5_not_shortest_warns() {
        // W1=30, W3=15, W5=20 (W5 > W3 → 不該)
        let waves = vec![
            cmw(30.0, 5, 0),
            cmw(8.0, 3, 5),
            cmw(15.0, 5, 8),
            cmw(5.0, 4, 13),
            cmw(20.0, 5, 17),
        ];
        let scenario = make_scenario_impulse(&waves);
        let findings = analyze(&scenario, &waves);
        let w5_warning = findings.iter().any(|f| {
            matches!(
                f.rule_id,
                RuleId::Ch11_Impulse_WaveByWave {
                    ext: ImpulseExtension::First,
                    wave: WaveNumber::W5,
                }
            ) && matches!(f.severity, AdvisorySeverity::Warning)
                && f.message.contains("最短")
        });
        assert!(w5_warning);
    }

    #[test]
    fn third_ext_w3_too_short_warns() {
        // W3 = 12, W1 = 10 → ratio 1.2 < 1.618 → Warning
        let waves = vec![
            cmw(10.0, 5, 0),
            cmw(5.0, 3, 5),
            cmw(12.0, 5, 8),
            cmw(3.0, 4, 13),
            cmw(8.0, 5, 17),
        ];
        // Force detect_extension to recognize as Third — make W3 longest of three
        // But 12 < 161.8% of 10 = 16.18 → W3 太短 → Warning
        let scenario = make_scenario_impulse(&waves);
        let findings = analyze(&scenario, &waves);
        // detect_extension 預期判 W3 為最長(非 W1/W5)
        let w3_warn = findings.iter().any(|f| {
            matches!(
                f.rule_id,
                RuleId::Ch11_Impulse_WaveByWave {
                    ext: ImpulseExtension::Third,
                    wave: WaveNumber::W3,
                }
            ) && matches!(f.severity, AdvisorySeverity::Warning)
                && f.message.contains("161.8")
        });
        assert!(w3_warn, "findings = {:?}", findings);
    }

    #[test]
    fn fifth_ext_w5_below_lower_limit_warns() {
        // W1=8, W3=15, W5=18 (W5 = 18 < (W1+W3) = 23 → Warning)
        let waves = vec![
            cmw(8.0, 5, 0),
            cmw(3.0, 3, 5),
            cmw(15.0, 5, 8),
            cmw(8.0, 4, 13),
            cmw(18.0, 8, 17),
        ];
        let scenario = make_scenario_impulse(&waves);
        let findings = analyze(&scenario, &waves);
        let w5_warn = findings.iter().any(|f| {
            matches!(
                f.rule_id,
                RuleId::Ch11_Impulse_WaveByWave {
                    ext: ImpulseExtension::Fifth,
                    wave: WaveNumber::W5,
                }
            ) && matches!(f.severity, AdvisorySeverity::Warning)
                && f.message.contains("1-3 全長")
        });
        assert!(w5_warn);
    }

    #[test]
    fn wave4_common_rule_warns_when_retrace_over_61_8() {
        // W3 = 20, W4 = 18 (90%) → Warning
        let waves = vec![
            cmw(30.0, 5, 0),
            cmw(8.0, 3, 5),
            cmw(20.0, 5, 8),
            cmw(18.0, 4, 13),
            cmw(15.0, 5, 17),
        ];
        let scenario = make_scenario_impulse(&waves);
        let findings = analyze(&scenario, &waves);
        let w4_common = findings.iter().any(|f| {
            matches!(
                f.rule_id,
                RuleId::Ch11_Impulse_WaveByWave {
                    ext: ImpulseExtension::NonExtended,
                    wave: WaveNumber::W4,
                }
            ) && matches!(f.severity, AdvisorySeverity::Warning)
                && f.message.contains("61.8")
        });
        assert!(w4_common);
    }

    #[test]
    fn fifth_wave_failure_strong_in_third_ext() {
        // 3rd Ext + W5 < W4 → Strong Failure
        let waves = vec![
            cmw(10.0, 5, 0),
            cmw(5.0, 3, 5),
            cmw(20.0, 8, 8),
            cmw(8.0, 4, 16),
            cmw(5.0, 5, 20), // W5=5 < W4=8 → Failure
        ];
        let scenario = make_scenario_impulse(&waves);
        let findings = analyze(&scenario, &waves);
        let failure = findings.iter().any(|f| {
            matches!(f.severity, AdvisorySeverity::Strong)
                && f.message.contains("5th Wave Failure")
        });
        assert!(failure);
    }

    #[test]
    fn no_findings_for_non_impulse() {
        let waves = vec![
            cmw(10.0, 5, 0),
            cmw(5.0, 3, 5),
            cmw(20.0, 8, 8),
            cmw(8.0, 4, 16),
            cmw(15.0, 5, 20),
        ];
        let mut scenario = make_scenario_impulse(&waves);
        scenario.pattern_type = NeelyPatternType::Zigzag {
            sub_kind: ZigzagKind::Single,
        };
        let findings = analyze(&scenario, &waves);
        assert!(findings.is_empty());
    }
}
