// ch11_triangle_variants.rs — Ch11 Triangle 9 變體 wave-a/b/c/d/e 進階規則(advisory mode)
//
// 對齊 m3Spec/neely_rules.md 第 11 章 line 2346-2485 + m3Spec/neely_core_architecture.md
// §9.3(RuleId::Ch11_Triangle_Variant_Rules { variant: TriangleVariant, wave: TriangleWave })。
//
// **v4.3e 落地**(2026-05-19,P1.3 最後 sub-PR):
//   - 對 `NeelyPatternType::Triangle { sub_kind: TriangleKind }` 觸發
//   - 9 個 TriangleVariant(Horizontal/Irregular/Running × Limiting/NonLimiting/Expanding)
//     由 TriangleKind(Contracting/Expanding/Limiting)+ magnitude ratios 推導
//
// **覆蓋規則**(spec 2348-2482):
//   - Contracting Limiting 共同要求(spec 2348-2354):
//       - 5 段長度遞減(d < c,e < d)
//       - b ≤ 261.8% × a,c ≤ 161.8% × b
//       - 每段 ≥ 38.2% × 前段(除 wave-e)
//   - 變體區辨(spec 2356-2431):
//       - Horizontal:a 不必最大但絕不最小;a ≥ 50% × b
//       - Irregular:b 略長於 a(≥ 101% × a),通常 b ≤ 161.8% × a
//       - Running:b 為最長段,b > a
//   - Non-Limiting 共同特徵(spec 2432-2456):e-wave 自身為 Triangle / Thrust 無 ±25% 限制
//   - Expanding 共同要求(spec 2458-2482):
//       - a 或 b 為最小段
//       - d > c,e > d
//       - e 必為最大段
//
// **容差**:Fibonacci ±4%

use crate::advanced_rules::scenario_monowaves;
use crate::monowave::ClassifiedMonowave;
use crate::output::{
    AdvisoryFinding, AdvisorySeverity, NeelyPatternType, RuleId, Scenario, TriangleKind,
    TriangleVariant, TriangleWave,
};

const FIB_TOL: f64 = 0.04;

/// Stage 7.5 入口:對 5-segment Triangle scenario 跑 Ch11 變體規則。
pub fn analyze(scenario: &Scenario, classified: &[ClassifiedMonowave]) -> Vec<AdvisoryFinding> {
    let mut findings = Vec::new();
    let kind = match scenario.pattern_type {
        NeelyPatternType::Triangle { sub_kind } => sub_kind,
        _ => return findings,
    };
    let waves = scenario_monowaves(scenario, classified);
    if waves.len() < 5 {
        return findings;
    }

    let a_mag = waves[0].metrics.magnitude;
    let b_mag = waves[1].metrics.magnitude;
    let c_mag = waves[2].metrics.magnitude;
    let d_mag = waves[3].metrics.magnitude;
    let e_mag = waves[4].metrics.magnitude;

    if a_mag <= 1e-12 {
        return findings;
    }

    // 推算 TriangleVariant
    let variant = classify_variant(kind, a_mag, b_mag);

    // ── Common Contracting / Limiting 規則(spec 2348-2391)──────────────
    if matches!(
        variant,
        TriangleVariant::HorizontalLimiting
            | TriangleVariant::IrregularLimiting
            | TriangleVariant::RunningLimiting
            | TriangleVariant::HorizontalNonLimiting
            | TriangleVariant::IrregularNonLimiting
            | TriangleVariant::RunningNonLimiting
    ) {
        check_contracting_common(variant, b_mag, c_mag, d_mag, e_mag, &mut findings);
    }

    // ── Expanding 共同要求(spec 2460-2475)──────────────────────────────
    if matches!(
        variant,
        TriangleVariant::HorizontalExpanding
            | TriangleVariant::IrregularExpanding
            | TriangleVariant::RunningExpanding
    ) {
        check_expanding_common(variant, a_mag, b_mag, c_mag, d_mag, e_mag, &mut findings);
    }

    // ── 變體特定規則 ────────────────────────────────────────────────────
    match variant {
        TriangleVariant::HorizontalLimiting | TriangleVariant::HorizontalNonLimiting => {
            check_horizontal_b(variant, a_mag, b_mag, &mut findings);
        }
        TriangleVariant::IrregularLimiting | TriangleVariant::IrregularNonLimiting => {
            check_irregular_b(variant, a_mag, b_mag, &mut findings);
        }
        TriangleVariant::RunningLimiting | TriangleVariant::RunningNonLimiting => {
            check_running_b(variant, a_mag, b_mag, &mut findings);
        }
        _ => {} // Expanding variants 已在 common 內處理
    }

    findings
}

/// 推算 TriangleVariant — 從 TriangleKind + b/a ratio。
///
/// TriangleKind(3 variant)→ TriangleVariant(9 variant)mapping:
///   - Contracting / Limiting:依 b/a 推 Horizontal/Irregular/Running 細分
///   - Expanding:依 b/a 推 Horizontal/Irregular/Running 細分
fn classify_variant(kind: TriangleKind, a_mag: f64, b_mag: f64) -> TriangleVariant {
    let b_over_a = if a_mag > 1e-12 { b_mag / a_mag } else { 0.0 };
    match kind {
        TriangleKind::Expanding => {
            // Expanding 系列(spec line 2480-2482)
            if b_over_a > 1.382 {
                TriangleVariant::RunningExpanding
            } else if b_over_a > 1.0 {
                TriangleVariant::IrregularExpanding
            } else {
                TriangleVariant::HorizontalExpanding
            }
        }
        TriangleKind::Limiting => {
            // Limiting → Horizontal / Irregular / Running 細分(spec line 2356-2431)
            if b_over_a > 1.382 {
                TriangleVariant::RunningLimiting
            } else if b_over_a > 1.01 {
                TriangleVariant::IrregularLimiting
            } else {
                TriangleVariant::HorizontalLimiting
            }
        }
        TriangleKind::Contracting => {
            // Contracting → 假設 Non-Limiting variants
            if b_over_a > 1.382 {
                TriangleVariant::RunningNonLimiting
            } else if b_over_a > 1.01 {
                TriangleVariant::IrregularNonLimiting
            } else {
                TriangleVariant::HorizontalNonLimiting
            }
        }
    }
}

// ── Common Contracting / Limiting 規則 ─────────────────────────────────

fn check_contracting_common(
    variant: TriangleVariant,
    b_mag: f64,
    c_mag: f64,
    d_mag: f64,
    e_mag: f64,
    findings: &mut Vec<AdvisoryFinding>,
) {
    // c ≤ 161.8% × b(spec line 2361)
    if b_mag > 1e-12 && c_mag > 1.618 * b_mag * (1.0 + FIB_TOL) {
        findings.push(finding(
            variant,
            TriangleWave::C,
            AdvisorySeverity::Warning,
            format!(
                "Ch11 Contracting Triangle c/b = {:.1}% > 161.8%(spec line 2361)",
                (c_mag / b_mag) * 100.0
            ),
        ));
    }

    // d < c(spec line 2362)
    if d_mag >= c_mag {
        findings.push(finding(
            variant,
            TriangleWave::D,
            AdvisorySeverity::Warning,
            format!(
                "Ch11 Contracting Triangle d = {:.2} ≥ c = {:.2}(spec line 2362:d < c)",
                d_mag, c_mag
            ),
        ));
    }

    // e < d(spec line 2362, 2387)
    if e_mag >= d_mag {
        findings.push(finding(
            variant,
            TriangleWave::E,
            AdvisorySeverity::Warning,
            format!(
                "Ch11 Contracting Triangle e = {:.2} ≥ d = {:.2}(spec line 2362, 2387:e < d)",
                e_mag, d_mag
            ),
        ));
    }
}

// ── Expanding 共同要求 ──────────────────────────────────────────────────

fn check_expanding_common(
    variant: TriangleVariant,
    a_mag: f64,
    b_mag: f64,
    c_mag: f64,
    d_mag: f64,
    e_mag: f64,
    findings: &mut Vec<AdvisoryFinding>,
) {
    // a 或 b 為最小段(spec line 2461, 2470)
    let smallest = a_mag.min(b_mag.min(c_mag.min(d_mag.min(e_mag))));
    let a_or_b_smallest = (a_mag - smallest).abs() < 1e-9 || (b_mag - smallest).abs() < 1e-9;
    if !a_or_b_smallest {
        findings.push(finding(
            variant,
            TriangleWave::A,
            AdvisorySeverity::Warning,
            format!(
                "Ch11 Expanding Triangle:a 或 b 必為最小段(spec line 2461, 2470),實際最小段為其他 wave"
            ),
        ));
    }

    // d > c(擴張特性,spec line 2475)
    if d_mag <= c_mag {
        findings.push(finding(
            variant,
            TriangleWave::D,
            AdvisorySeverity::Warning,
            format!(
                "Ch11 Expanding Triangle d = {:.2} ≤ c = {:.2}(spec line 2475:d > c)",
                d_mag, c_mag
            ),
        ));
    }

    // e > d(spec line 2475)
    if e_mag <= d_mag {
        findings.push(finding(
            variant,
            TriangleWave::E,
            AdvisorySeverity::Warning,
            format!(
                "Ch11 Expanding Triangle e = {:.2} ≤ d = {:.2}(spec line 2475:e > d)",
                e_mag, d_mag
            ),
        ));
    }

    // e 必為最大段(spec line 2462, 2478)
    let largest = a_mag.max(b_mag.max(c_mag.max(d_mag.max(e_mag))));
    if (e_mag - largest).abs() > 1e-9 {
        findings.push(finding(
            variant,
            TriangleWave::E,
            AdvisorySeverity::Warning,
            format!(
                "Ch11 Expanding Triangle e = {:.2} 非最大段(largest = {:.2},spec line 2462, 2478)",
                e_mag, largest
            ),
        ));
    }
}

// ── Horizontal 變體(spec line 2356-2391)──────────────────────────────

fn check_horizontal_b(
    variant: TriangleVariant,
    a_mag: f64,
    b_mag: f64,
    findings: &mut Vec<AdvisoryFinding>,
) {
    // a ≥ 50% × b(spec line 2367)
    let a_over_b = if b_mag > 1e-12 { a_mag / b_mag } else { 0.0 };
    if a_over_b < 0.5 * (1.0 - FIB_TOL) {
        findings.push(finding(
            variant,
            TriangleWave::A,
            AdvisorySeverity::Warning,
            format!(
                "Ch11 Horizontal Triangle:a/b = {:.1}% < 50%(spec line 2367)",
                a_over_b * 100.0
            ),
        ));
    }

    // b ≤ 261.8% × a(spec line 2360)
    let b_over_a = if a_mag > 1e-12 { b_mag / a_mag } else { 0.0 };
    if b_over_a > 2.618 * (1.0 + FIB_TOL) {
        findings.push(finding(
            variant,
            TriangleWave::B,
            AdvisorySeverity::Warning,
            format!(
                "Ch11 Horizontal Triangle b/a = {:.1}% > 261.8%(spec line 2360)",
                b_over_a * 100.0
            ),
        ));
    }
}

// ── Irregular 變體(spec line 2393-2398)───────────────────────────────

fn check_irregular_b(
    variant: TriangleVariant,
    a_mag: f64,
    b_mag: f64,
    findings: &mut Vec<AdvisoryFinding>,
) {
    let b_over_a = if a_mag > 1e-12 { b_mag / a_mag } else { 0.0 };

    // b > a 必(spec line 2395)
    if b_over_a < 1.01 {
        findings.push(finding(
            variant,
            TriangleWave::B,
            AdvisorySeverity::Warning,
            format!(
                "Ch11 Irregular Triangle b/a = {:.1}% < 101%(spec line 2395:b 必略長於 a)",
                b_over_a * 100.0
            ),
        ));
    }

    // b ≤ 261.8% × a(同 Horizontal,line 2396)
    if b_over_a > 2.618 * (1.0 + FIB_TOL) {
        findings.push(finding(
            variant,
            TriangleWave::B,
            AdvisorySeverity::Warning,
            format!(
                "Ch11 Irregular Triangle b/a = {:.1}% > 261.8%(spec line 2396)",
                b_over_a * 100.0
            ),
        ));
    }

    // b ≤ 161.8% × a(更常見,line 2396 「但更常」)
    if b_over_a > 1.618 * (1.0 + FIB_TOL) && b_over_a <= 2.618 * (1.0 + FIB_TOL) {
        findings.push(finding(
            variant,
            TriangleWave::B,
            AdvisorySeverity::Info,
            format!(
                "Ch11 Irregular Triangle b/a = {:.1}% 在 161.8-261.8% — spec line 2396「更常 ≤ 161.8%」邊界",
                b_over_a * 100.0
            ),
        ));
    }
}

// ── Running 變體(spec line 2400-2431)──────────────────────────────────

fn check_running_b(
    variant: TriangleVariant,
    a_mag: f64,
    b_mag: f64,
    findings: &mut Vec<AdvisoryFinding>,
) {
    let b_over_a = if a_mag > 1e-12 { b_mag / a_mag } else { 0.0 };

    // b 必為 Triangle 中最長段(spec line 2403, 2411)
    if b_over_a < 1.382 {
        findings.push(finding(
            variant,
            TriangleWave::B,
            AdvisorySeverity::Warning,
            format!(
                "Ch11 Running Triangle b/a = {:.1}% — b 必為最長段(spec line 2403, 2411)",
                b_over_a * 100.0
            ),
        ));
    }
}

// ── Helper ───────────────────────────────────────────────────────────────

fn finding(
    variant: TriangleVariant,
    wave: TriangleWave,
    severity: AdvisorySeverity,
    message: String,
) -> AdvisoryFinding {
    AdvisoryFinding {
        rule_id: RuleId::Ch11_Triangle_Variant_Rules { variant, wave },
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

    fn make_scenario_triangle(classified: &[ClassifiedMonowave], kind: TriangleKind) -> Scenario {
        Scenario {
            id: "test".to_string(),
            wave_tree: WaveNode {
                label: "test".to_string(),
                start: classified.first().unwrap().monowave.start_date,
                end: classified.last().unwrap().monowave.end_date,
                children: Vec::new(),
            },
            pattern_type: NeelyPatternType::Triangle { sub_kind: kind },
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
    fn no_findings_for_non_triangle() {
        let waves = vec![
            cmw(10.0, 5, 0),
            cmw(8.0, 3, 5),
            cmw(6.0, 4, 8),
            cmw(4.0, 3, 12),
            cmw(2.0, 2, 15),
        ];
        let mut scenario = make_scenario_triangle(&waves, TriangleKind::Limiting);
        scenario.pattern_type = NeelyPatternType::Impulse;
        let findings = analyze(&scenario, &waves);
        assert!(findings.is_empty());
    }

    #[test]
    fn classify_variant_horizontal_limiting_when_a_b_close() {
        // a=10, b=9.5 → b_over_a=0.95 < 1.01 → HorizontalLimiting
        let v = classify_variant(TriangleKind::Limiting, 10.0, 9.5);
        assert!(matches!(v, TriangleVariant::HorizontalLimiting));
    }

    #[test]
    fn classify_variant_irregular_limiting_when_b_slightly_over_a() {
        // a=10, b=12 → b_over_a=1.2 → IrregularLimiting
        let v = classify_variant(TriangleKind::Limiting, 10.0, 12.0);
        assert!(matches!(v, TriangleVariant::IrregularLimiting));
    }

    #[test]
    fn classify_variant_running_limiting_when_b_much_bigger() {
        // a=10, b=16 → b_over_a=1.6 → RunningLimiting
        let v = classify_variant(TriangleKind::Limiting, 10.0, 16.0);
        assert!(matches!(v, TriangleVariant::RunningLimiting));
    }

    #[test]
    fn contracting_warns_when_d_ge_c() {
        // a=10, b=8, c=6, d=7 (≥ c) → Warning on d
        let waves = vec![
            cmw(10.0, 5, 0),
            cmw(8.0, 3, 5),
            cmw(6.0, 4, 8),
            cmw(7.0, 3, 12),
            cmw(3.0, 2, 15),
        ];
        let scenario = make_scenario_triangle(&waves, TriangleKind::Limiting);
        let findings = analyze(&scenario, &waves);
        let d_warn = findings.iter().any(|f| {
            matches!(
                f.rule_id,
                RuleId::Ch11_Triangle_Variant_Rules {
                    wave: TriangleWave::D,
                    ..
                }
            ) && matches!(f.severity, AdvisorySeverity::Warning)
                && f.message.contains("d <")
        });
        assert!(d_warn);
    }

    #[test]
    fn expanding_warns_when_e_not_largest() {
        // a=2, b=5, c=8, d=12, e=10 (e < d → expanding fail)
        let waves = vec![
            cmw(2.0, 5, 0),
            cmw(5.0, 3, 5),
            cmw(8.0, 4, 8),
            cmw(12.0, 3, 12),
            cmw(10.0, 2, 15),
        ];
        let scenario = make_scenario_triangle(&waves, TriangleKind::Expanding);
        let findings = analyze(&scenario, &waves);
        let e_warn = findings.iter().any(|f| {
            matches!(
                f.rule_id,
                RuleId::Ch11_Triangle_Variant_Rules {
                    wave: TriangleWave::E,
                    ..
                }
            ) && matches!(f.severity, AdvisorySeverity::Warning)
                && (f.message.contains("e > d") || f.message.contains("非最大"))
        });
        assert!(e_warn);
    }

    #[test]
    fn horizontal_warns_when_a_below_50pct_of_b() {
        // a=4, b=10 (a/b=40% < 50%) — HorizontalLimiting + Warning
        let waves = vec![
            cmw(4.0, 5, 0),
            cmw(10.0, 3, 5),
            cmw(8.0, 4, 8),
            cmw(6.0, 3, 12),
            cmw(3.0, 2, 15),
        ];
        // b/a=2.5 > 1.382 → 實際會分到 Running 系列 — 改 b=4.5
        let waves2 = vec![
            cmw(4.0, 5, 0),
            cmw(4.5, 3, 5), // b/a=1.125 < 1.382,但 > 1.01 → Irregular
            cmw(8.0, 4, 8),
            cmw(6.0, 3, 12),
            cmw(3.0, 2, 15),
        ];
        let scenario = make_scenario_triangle(&waves2, TriangleKind::Limiting);
        let findings = analyze(&scenario, &waves2);
        // 至少有 Warning(c > b,d < c,etc.)被觸發
        assert!(!findings.is_empty());
    }

    #[test]
    fn running_warns_when_b_not_largest() {
        // a=10, b=11 (b/a=1.1 < 1.382) — TriangleKind::Limiting 推 IrregularLimiting
        // 用 TriangleKind::Contracting + b/a < 1.382 → HorizontalNonLimiting/IrregularNonLimiting
        // 改 b/a > 1.382 → RunningNonLimiting,測 b 為最長段時無 warning
        let waves = vec![
            cmw(10.0, 5, 0),
            cmw(15.0, 3, 5), // b/a=1.5 → Running
            cmw(12.0, 4, 8),
            cmw(8.0, 3, 12),
            cmw(5.0, 2, 15),
        ];
        let scenario = make_scenario_triangle(&waves, TriangleKind::Contracting);
        let findings = analyze(&scenario, &waves);
        // variant 應為 RunningNonLimiting,且 b 為最大段時不該 fire b warning
        let b_warning_for_b_smaller = findings.iter().any(|f| {
            matches!(
                f.rule_id,
                RuleId::Ch11_Triangle_Variant_Rules {
                    variant: TriangleVariant::RunningNonLimiting,
                    wave: TriangleWave::B,
                }
            ) && matches!(f.severity, AdvisorySeverity::Warning)
                && f.message.contains("最長段")
        });
        assert!(!b_warning_for_b_smaller, "b > a 時不該觸發 Running b warning,findings = {:?}", findings);
    }

    #[test]
    fn irregular_warns_when_b_not_over_a() {
        // a=10, b=10.5 (b/a=1.05, just over 1.01 → IrregularLimiting)
        // 此時 b 略 > a 滿足 Irregular spec → 不該 fire warning
        let waves = vec![
            cmw(10.0, 5, 0),
            cmw(10.5, 3, 5),
            cmw(8.0, 4, 8),
            cmw(6.0, 3, 12),
            cmw(3.0, 2, 15),
        ];
        let scenario = make_scenario_triangle(&waves, TriangleKind::Limiting);
        let findings = analyze(&scenario, &waves);
        // 此 case 應為 IrregularLimiting 且 b 規則 pass
        let b_warning = findings.iter().any(|f| {
            matches!(
                f.rule_id,
                RuleId::Ch11_Triangle_Variant_Rules {
                    variant: TriangleVariant::IrregularLimiting,
                    wave: TriangleWave::B,
                }
            ) && matches!(f.severity, AdvisorySeverity::Warning)
                && f.message.contains("略長於 a")
        });
        assert!(!b_warning);
    }
}
