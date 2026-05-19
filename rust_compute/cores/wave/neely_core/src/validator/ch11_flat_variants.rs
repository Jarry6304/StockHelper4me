// ch11_flat_variants.rs — Ch11 Flat 七變體 wave-a/b/c 規則(advisory mode)
//
// 對齊 m3Spec/neely_rules.md 第 11 章 line 2191-2322 + m3Spec/neely_core_architecture.md
// §9.3(RuleId::Ch11_Flat_Variant_Rules { variant: FlatVariant, wave: WaveAbc })。
//
// **v4.3c 落地**(2026-05-19):
//   - 對 `NeelyPatternType::Flat { sub_kind: FlatKind }` 觸發
//   - 依 FlatKind 對應 FlatVariant 各跑該變體的 wave-a/b/c 核心規則
//   - Advisory mode:違反 → Warning AdvisoryFinding 不 invalidate scenario
//
// **覆蓋變體**(spec 2195-2321):
//   - B-Failure(b 弱 61.8-81% × a,c ≥ 61.8% × b)
//   - C-Failure(c < b 未完全回測 b)
//   - Common Flat(b 81-100% × a,c 必完全回測 b)
//   - Double Failure(b 弱 + c 未完全回測 b)
//   - Elongated(c 遠大於 a)
//   - Irregular(b > a 但 c 完全回測 b)
//   - IrregularStrongB(v4.1 加,b 123.6-138.2% × a)
//   - Irregular Failure(b > 138.2% × a)
//   - Running Correction(整段不退至 a 起點 — 對應 NeelyPatternType::RunningCorrection)
//
// **容差**:Fibonacci ±4%

use crate::advanced_rules::scenario_monowaves;
use crate::monowave::ClassifiedMonowave;
use crate::output::{
    AdvisoryFinding, AdvisorySeverity, FlatKind, FlatVariant, NeelyPatternType, RuleId, Scenario,
    WaveAbc,
};

const FIB_TOL: f64 = 0.04;

/// Stage 7.5 入口:對 3-wave Flat scenario 跑 Ch11 變體規則。
pub fn analyze(scenario: &Scenario, classified: &[ClassifiedMonowave]) -> Vec<AdvisoryFinding> {
    let mut findings = Vec::new();
    let flat_kind = match scenario.pattern_type {
        NeelyPatternType::Flat { sub_kind } => sub_kind,
        _ => return findings,
    };
    let waves = scenario_monowaves(scenario, classified);
    if waves.len() < 3 {
        return findings;
    }

    let a_mag = waves[0].metrics.magnitude;
    let b_mag = waves[1].metrics.magnitude;
    let c_mag = waves[2].metrics.magnitude;

    if a_mag <= 1e-12 {
        return findings;
    }

    let b_over_a = b_mag / a_mag;
    let c_over_b = if b_mag > 1e-12 { c_mag / b_mag } else { 0.0 };
    let c_over_a = c_mag / a_mag;

    let variant = flat_kind_to_variant(flat_kind);

    match flat_kind {
        FlatKind::BFailure => analyze_b_failure(b_over_a, c_over_b, &mut findings, variant),
        FlatKind::CFailure => analyze_c_failure(c_over_b, c_over_a, &mut findings, variant),
        FlatKind::Common => analyze_common(b_over_a, c_over_b, c_over_a, &mut findings, variant),
        FlatKind::DoubleFailure => analyze_double_failure(b_over_a, c_over_b, &mut findings, variant),
        FlatKind::Irregular | FlatKind::IrregularStrongB => {
            analyze_irregular(b_over_a, c_over_b, &mut findings, variant)
        }
        FlatKind::IrregularFailure => {
            analyze_irregular_failure(b_over_a, c_over_b, &mut findings, variant)
        }
        FlatKind::Elongated => analyze_elongated(b_over_a, c_over_a, &mut findings, variant),
    }

    findings
}

fn flat_kind_to_variant(kind: FlatKind) -> FlatVariant {
    match kind {
        FlatKind::Common => FlatVariant::Common,
        FlatKind::BFailure => FlatVariant::BFailure,
        FlatKind::CFailure => FlatVariant::CFailure,
        FlatKind::DoubleFailure => FlatVariant::DoubleFailure,
        FlatKind::Irregular | FlatKind::IrregularStrongB => FlatVariant::Irregular,
        FlatKind::IrregularFailure => FlatVariant::IrregularFailure,
        FlatKind::Elongated => FlatVariant::Elongated,
    }
}

// ── B-Failure(spec 2195-2206)────────────────────────────────────────────

fn analyze_b_failure(
    b_over_a: f64,
    c_over_b: f64,
    findings: &mut Vec<AdvisoryFinding>,
    variant: FlatVariant,
) {
    // Rule: b 在 61.8-81% × a(spec 2199)
    if !(0.618 * (1.0 - FIB_TOL)..=0.81 * (1.0 + FIB_TOL)).contains(&b_over_a) {
        findings.push(finding(
            variant,
            WaveAbc::B,
            AdvisorySeverity::Warning,
            format!(
                "Ch11 B-Failure b/a = {:.1}% 不在 61.8-81% 範圍(spec line 2199)",
                b_over_a * 100.0
            ),
        ));
    }
    // Rule: c ≥ 61.8% × b 且 c 必完全回測 b(spec 2200)
    if c_over_b < 0.618 * (1.0 - FIB_TOL) {
        findings.push(finding(
            variant,
            WaveAbc::C,
            AdvisorySeverity::Warning,
            format!(
                "Ch11 B-Failure c/b = {:.1}% < 61.8%(spec line 2200:c 必 ≥ 61.8% × b)",
                c_over_b * 100.0
            ),
        ));
    }
    if c_over_b < 1.0 {
        findings.push(finding(
            variant,
            WaveAbc::C,
            AdvisorySeverity::Warning,
            format!(
                "Ch11 B-Failure c/b = {:.1}% < 100%(spec line 2200:c 必完全回測 b — 否則歸 Double Failure)",
                c_over_b * 100.0
            ),
        ));
    }
}

// ── C-Failure(spec 2209-2223)────────────────────────────────────────────

fn analyze_c_failure(
    c_over_b: f64,
    c_over_a: f64,
    findings: &mut Vec<AdvisoryFinding>,
    variant: FlatVariant,
) {
    // Rule: c < b(本變體定義,spec 2209)
    if c_over_b >= 1.0 {
        findings.push(finding(
            variant,
            WaveAbc::C,
            AdvisorySeverity::Warning,
            format!(
                "Ch11 C-Failure c/b = {:.1}% ≥ 100%(spec line 2209:C-Failure 定義為 c < b)",
                c_over_b * 100.0
            ),
        ));
    }
    // Rule: c < 61.8% × b 視為極罕(spec 2215)
    if c_over_b < 0.618 * (1.0 - FIB_TOL) {
        findings.push(finding(
            variant,
            WaveAbc::C,
            AdvisorySeverity::Info,
            format!(
                "Ch11 C-Failure c/b = {:.1}% < 61.8% — 極罕情境(spec line 2215:該情境 b 必為時間最長段,a-c 時間相等)",
                c_over_b * 100.0
            ),
        ));
    }
    // Rule: c 應 ≈ 61.8% × a 或更小(spec 2217)
    if c_over_a > 0.618 * (1.0 + FIB_TOL) {
        findings.push(finding(
            variant,
            WaveAbc::C,
            AdvisorySeverity::Info,
            format!(
                "Ch11 C-Failure c/a = {:.1}% > 61.8%(spec line 2217 典型 ≈ 61.8% × a 或更小)",
                c_over_a * 100.0
            ),
        ));
    }
}

// ── Common Flat(spec 2225-2238)──────────────────────────────────────────

fn analyze_common(
    b_over_a: f64,
    c_over_b: f64,
    c_over_a: f64,
    findings: &mut Vec<AdvisoryFinding>,
    variant: FlatVariant,
) {
    // Rule: b ∈ [81%, 100%] × a(spec 2228)
    if !(0.81 * (1.0 - FIB_TOL)..=1.0 * (1.0 + FIB_TOL)).contains(&b_over_a) {
        findings.push(finding(
            variant,
            WaveAbc::B,
            AdvisorySeverity::Warning,
            format!(
                "Ch11 Common Flat b/a = {:.1}% 不在 81-100% 範圍(spec line 2228)",
                b_over_a * 100.0
            ),
        ));
    }
    // Rule: c 必完全回測 b(spec 2229)
    if c_over_b < 1.0 {
        findings.push(finding(
            variant,
            WaveAbc::C,
            AdvisorySeverity::Warning,
            format!(
                "Ch11 Common Flat c/b = {:.1}% < 100%(spec line 2229:c 必完全回測 b)",
                c_over_b * 100.0
            ),
        ));
    }
    // Rule: c 稍微超越 a 終點不超過 10-20%(spec 2230)
    if c_over_a > 1.2 {
        findings.push(finding(
            variant,
            WaveAbc::C,
            AdvisorySeverity::Info,
            format!(
                "Ch11 Common Flat c/a = {:.1}% > 120%(spec line 2230 典型超越不超 10-20%)",
                c_over_a * 100.0
            ),
        ));
    }
}

// ── Double Failure(spec 2240-2263)───────────────────────────────────────

fn analyze_double_failure(
    b_over_a: f64,
    c_over_b: f64,
    findings: &mut Vec<AdvisoryFinding>,
    variant: FlatVariant,
) {
    // Rule: b ≤ 81% × a(spec 2243)
    if b_over_a > 0.81 * (1.0 + FIB_TOL) {
        findings.push(finding(
            variant,
            WaveAbc::B,
            AdvisorySeverity::Warning,
            format!(
                "Ch11 Double Failure b/a = {:.1}% > 81%(spec line 2243)",
                b_over_a * 100.0
            ),
        ));
    }
    // Rule: c < 100% × b(spec 2244)
    if c_over_b >= 1.0 {
        findings.push(finding(
            variant,
            WaveAbc::C,
            AdvisorySeverity::Warning,
            format!(
                "Ch11 Double Failure c/b = {:.1}% ≥ 100%(spec line 2244:c 未完全回測 b 為定義)",
                c_over_b * 100.0
            ),
        ));
    }
}

// ── Elongated(spec 2265-2276)────────────────────────────────────────────

fn analyze_elongated(
    b_over_a: f64,
    c_over_a: f64,
    findings: &mut Vec<AdvisoryFinding>,
    variant: FlatVariant,
) {
    // Rule: b ≥ 61.8% × a,通常相似(spec 2268)
    if b_over_a < 0.618 * (1.0 - FIB_TOL) {
        findings.push(finding(
            variant,
            WaveAbc::B,
            AdvisorySeverity::Warning,
            format!(
                "Ch11 Elongated b/a = {:.1}% < 61.8%(spec line 2268:a/b 必相似 — b ≥ 61.8% × a)",
                b_over_a * 100.0
            ),
        ));
    }
    // Rule: c 遠大於 a(spec 2269)— c > a × 1.5 視為「遠大於」
    if c_over_a <= 1.0 {
        findings.push(finding(
            variant,
            WaveAbc::C,
            AdvisorySeverity::Warning,
            format!(
                "Ch11 Elongated c/a = {:.1}% ≤ 100%(spec line 2269:c 必遠大於 a)",
                c_over_a * 100.0
            ),
        ));
    } else if c_over_a < 1.5 {
        findings.push(finding(
            variant,
            WaveAbc::C,
            AdvisorySeverity::Info,
            format!(
                "Ch11 Elongated c/a = {:.1}% 接近 a — 邊界 Elongated(spec line 2269 預期 c 遠大於 a)",
                c_over_a * 100.0
            ),
        ));
    }
}

// ── Irregular(spec 2278-2287)────────────────────────────────────────────

fn analyze_irregular(
    b_over_a: f64,
    c_over_b: f64,
    findings: &mut Vec<AdvisoryFinding>,
    variant: FlatVariant,
) {
    // Rule: b > a 但 ≤ 138.2% × a(spec 2281)
    if b_over_a <= 1.0 {
        findings.push(finding(
            variant,
            WaveAbc::B,
            AdvisorySeverity::Warning,
            format!(
                "Ch11 Irregular b/a = {:.1}% ≤ 100%(spec 2278:Irregular 定義為 b > a)",
                b_over_a * 100.0
            ),
        ));
    }
    if b_over_a > 1.382 * (1.0 + FIB_TOL) {
        findings.push(finding(
            variant,
            WaveAbc::B,
            AdvisorySeverity::Warning,
            format!(
                "Ch11 Irregular b/a = {:.1}% > 138.2%(spec line 2281:超過則歸 Irregular Failure)",
                b_over_a * 100.0
            ),
        ));
    }
    // Rule: c 必完全回測 b(spec 2278)
    if c_over_b < 1.0 {
        findings.push(finding(
            variant,
            WaveAbc::C,
            AdvisorySeverity::Warning,
            format!(
                "Ch11 Irregular c/b = {:.1}% < 100%(spec 2278:c 必完全回測 b)",
                c_over_b * 100.0
            ),
        ));
    }
}

// ── Irregular Failure(spec 2289-2301)────────────────────────────────────

fn analyze_irregular_failure(
    b_over_a: f64,
    c_over_b: f64,
    findings: &mut Vec<AdvisoryFinding>,
    variant: FlatVariant,
) {
    // Rule: b > 138.2% × a(spec 2289 定義)
    if b_over_a <= 1.382 {
        findings.push(finding(
            variant,
            WaveAbc::B,
            AdvisorySeverity::Warning,
            format!(
                "Ch11 Irregular Failure b/a = {:.1}% ≤ 138.2%(spec 2289 定義為 b > 138.2% × a)",
                b_over_a * 100.0
            ),
        ));
    }
    // Rule: c 不可完全回測 b(spec 2294)
    if c_over_b >= 1.0 {
        findings.push(finding(
            variant,
            WaveAbc::C,
            AdvisorySeverity::Warning,
            format!(
                "Ch11 Irregular Failure c/b = {:.1}% ≥ 100%(spec line 2294:c 不可完全回測 b)",
                c_over_b * 100.0
            ),
        ));
    }
}

// ── Helper ───────────────────────────────────────────────────────────────

fn finding(
    variant: FlatVariant,
    wave: WaveAbc,
    severity: AdvisorySeverity,
    message: String,
) -> AdvisoryFinding {
    AdvisoryFinding {
        rule_id: RuleId::Ch11_Flat_Variant_Rules { variant, wave },
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

    fn make_scenario_flat(classified: &[ClassifiedMonowave], kind: FlatKind) -> Scenario {
        Scenario {
            id: "test".to_string(),
            wave_tree: WaveNode {
                label: "test".to_string(),
                start: classified.first().unwrap().monowave.start_date,
                end: classified.last().unwrap().monowave.end_date,
                children: Vec::new(),
            },
            pattern_type: NeelyPatternType::Flat { sub_kind: kind },
            initial_direction: MonowaveDirection::Up,
            compacted_base_label: StructureLabel::Three,
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
    fn no_findings_for_non_flat() {
        let waves = vec![cmw(10.0, 5, 0), cmw(7.0, 3, 5), cmw(8.0, 4, 8)];
        let mut scenario = make_scenario_flat(&waves, FlatKind::Common);
        scenario.pattern_type = NeelyPatternType::Impulse;
        let findings = analyze(&scenario, &waves);
        assert!(findings.is_empty());
    }

    #[test]
    fn b_failure_warns_when_c_not_full_retrace() {
        // a=10, b=7 (70%), c=5 (c/b=0.71 < 1.0) → Warning
        let waves = vec![cmw(10.0, 5, 0), cmw(7.0, 3, 5), cmw(5.0, 4, 8)];
        let scenario = make_scenario_flat(&waves, FlatKind::BFailure);
        let findings = analyze(&scenario, &waves);
        let c_warn = findings.iter().any(|f| {
            matches!(
                f.rule_id,
                RuleId::Ch11_Flat_Variant_Rules {
                    variant: FlatVariant::BFailure,
                    wave: WaveAbc::C,
                }
            ) && matches!(f.severity, AdvisorySeverity::Warning)
                && f.message.contains("完全回測")
        });
        assert!(c_warn);
    }

    #[test]
    fn c_failure_warns_when_c_ge_b() {
        // a=10, b=10, c=12 (c/b=1.2 ≥ 1) → Warning
        let waves = vec![cmw(10.0, 5, 0), cmw(10.0, 3, 5), cmw(12.0, 4, 8)];
        let scenario = make_scenario_flat(&waves, FlatKind::CFailure);
        let findings = analyze(&scenario, &waves);
        let c_warn = findings.iter().any(|f| {
            matches!(
                f.rule_id,
                RuleId::Ch11_Flat_Variant_Rules {
                    variant: FlatVariant::CFailure,
                    wave: WaveAbc::C,
                }
            ) && matches!(f.severity, AdvisorySeverity::Warning)
                && f.message.contains("c < b")
        });
        assert!(c_warn);
    }

    #[test]
    fn common_flat_clean_no_warnings() {
        // a=10, b=9 (90%), c=10 (c/b=1.11 ≥ 1) → 0 warnings on b & c
        let waves = vec![cmw(10.0, 5, 0), cmw(9.0, 3, 5), cmw(10.0, 4, 8)];
        let scenario = make_scenario_flat(&waves, FlatKind::Common);
        let findings = analyze(&scenario, &waves);
        let warnings_count = findings.iter().filter(|f| matches!(f.severity, AdvisorySeverity::Warning)).count();
        assert_eq!(warnings_count, 0, "findings = {:?}", findings);
    }

    #[test]
    fn elongated_warns_when_c_below_a() {
        // a=10, b=8, c=9 (c ≤ a) → Warning on c
        let waves = vec![cmw(10.0, 5, 0), cmw(8.0, 3, 5), cmw(9.0, 4, 8)];
        let scenario = make_scenario_flat(&waves, FlatKind::Elongated);
        let findings = analyze(&scenario, &waves);
        let c_warn = findings.iter().any(|f| {
            matches!(
                f.rule_id,
                RuleId::Ch11_Flat_Variant_Rules {
                    variant: FlatVariant::Elongated,
                    wave: WaveAbc::C,
                }
            ) && matches!(f.severity, AdvisorySeverity::Warning)
                && f.message.contains("≤ 100%")
        });
        assert!(c_warn);
    }

    #[test]
    fn irregular_failure_warns_when_b_below_138_2() {
        // a=10, b=12 (120%, < 138.2%), c=10 → Warning(IrregularFailure 定義 b > 138.2%)
        let waves = vec![cmw(10.0, 5, 0), cmw(12.0, 3, 5), cmw(10.0, 4, 8)];
        let scenario = make_scenario_flat(&waves, FlatKind::IrregularFailure);
        let findings = analyze(&scenario, &waves);
        let b_warn = findings.iter().any(|f| {
            matches!(
                f.rule_id,
                RuleId::Ch11_Flat_Variant_Rules {
                    variant: FlatVariant::IrregularFailure,
                    wave: WaveAbc::B,
                }
            ) && matches!(f.severity, AdvisorySeverity::Warning)
                && f.message.contains("138.2")
        });
        assert!(b_warn);
    }

    #[test]
    fn irregular_strong_b_uses_irregular_variant() {
        // FlatKind::IrregularStrongB → mapped to FlatVariant::Irregular(同 analyzer)
        // a=10, b=15 (150% > 138.2%) → 應 fire Warning on b(超範圍 → 該歸 IrregularFailure)
        let waves = vec![cmw(10.0, 5, 0), cmw(15.0, 3, 5), cmw(13.0, 4, 8)];
        let scenario = make_scenario_flat(&waves, FlatKind::IrregularStrongB);
        let findings = analyze(&scenario, &waves);
        let irreg = findings.iter().any(|f| {
            matches!(
                f.rule_id,
                RuleId::Ch11_Flat_Variant_Rules {
                    variant: FlatVariant::Irregular,
                    ..
                }
            )
        });
        assert!(irreg, "IrregularStrongB 應走 Irregular variant analyzer,findings = {:?}", findings);
    }

    #[test]
    fn double_failure_warns_when_c_full_retrace() {
        // a=10, b=7 (70%), c=8 (c/b=1.14 ≥ 1) → Warning(Double Failure 定義 c 未完全回測 b)
        let waves = vec![cmw(10.0, 5, 0), cmw(7.0, 3, 5), cmw(8.0, 4, 8)];
        let scenario = make_scenario_flat(&waves, FlatKind::DoubleFailure);
        let findings = analyze(&scenario, &waves);
        let c_warn = findings.iter().any(|f| {
            matches!(
                f.rule_id,
                RuleId::Ch11_Flat_Variant_Rules {
                    variant: FlatVariant::DoubleFailure,
                    wave: WaveAbc::C,
                }
            ) && matches!(f.severity, AdvisorySeverity::Warning)
                && f.message.contains("未完全回測")
        });
        assert!(c_warn);
    }
}
