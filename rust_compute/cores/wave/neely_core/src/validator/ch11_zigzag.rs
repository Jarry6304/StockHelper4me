// ch11_zigzag.rs — Ch11 Zigzag wave-a/b/c 進階規則(advisory mode)
//                  + Appendix B 項 F(Zigzag c 在 Triangle 內例外)
//
// 對齊 m3Spec/neely_rules.md 第 11 章 line 2323-2345
//       + Appendix B 項 F + m3Spec/neely_core_architecture.md §9.3
//       (RuleId::Ch11_Zigzag_WaveByWave { wave: WaveAbc })。
//
// **v4.3d 落地**(2026-05-19):
//   - 對 `NeelyPatternType::Zigzag { sub_kind: ZigzagKind }` 觸發
//   - Advisory mode:違反 → Warning AdvisoryFinding 不 invalidate scenario
//   - Appendix B 項 F:scenario.in_triangle_context = true 時,c 範圍放寬(spec line 2338-2342)
//
// **規則覆蓋**(spec 2325-2342):
//   - Wave-a:必為 Impulse 結構(advisory only — 無法在此確認 :5 結構,只標示「規則存在」)
//             b 回測 ≤ 61.8% × a / b > 81% × a 觸發 Missing Wave Rule 警告
//   - Wave-b:幾乎任何修正,但不可為 Running Double/Triple
//   - Wave-c:
//       - 不在 Triangle 內 → c ∈ [61.8%, 161.8%] × a
//       - 在 Triangle 內(`in_triangle_context = true`) → c 任意方向放寬(Appendix B 項 F)
//
// **容差**:Fibonacci ±4%

use crate::advanced_rules::scenario_monowaves;
use crate::monowave::ClassifiedMonowave;
use crate::output::{
    AdvisoryFinding, AdvisorySeverity, NeelyPatternType, RuleId, Scenario, WaveAbc,
};

const FIB_TOL: f64 = 0.04;

/// Stage 7.5 入口:對 3-wave Zigzag scenario 跑 Ch11 wave-a/b/c 進階規則。
pub fn analyze(scenario: &Scenario, classified: &[ClassifiedMonowave]) -> Vec<AdvisoryFinding> {
    let mut findings = Vec::new();
    if !matches!(scenario.pattern_type, NeelyPatternType::Zigzag { .. }) {
        return findings;
    }
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
    let c_over_a = c_mag / a_mag;
    let in_triangle = scenario.in_triangle_context;

    // ── Wave-a 規則 ─────────────────────────────────────────────────────
    // b 回測 ≥ 81% × a → 警告 Missing Wave Rule(spec line 2329)
    if b_over_a >= 0.81 * (1.0 - FIB_TOL) {
        findings.push(finding(
            WaveAbc::A,
            AdvisorySeverity::Warning,
            format!(
                "Ch11 Zigzag wave-a:b 回測 {:.1}% × a ≥ 81% — 重檢 a 判讀,極可能 Missing Wave Rule(spec line 2329)",
                b_over_a * 100.0
            ),
        ));
    }

    // ── Wave-b 規則 ─────────────────────────────────────────────────────
    // b ≤ 61.8% × a(spec line 2332)— b 在 61.8-81% 區間屬 wave-b 內的 wave-a
    if b_over_a > 0.618 * (1.0 + FIB_TOL) && b_over_a < 0.81 {
        findings.push(finding(
            WaveAbc::B,
            AdvisorySeverity::Warning,
            format!(
                "Ch11 Zigzag wave-b:b 回測 {:.1}% × a 在 61.8-81% 區間 — 該回測可能是 wave-b 內部 wave-a(spec line 2328-2332)",
                b_over_a * 100.0
            ),
        ));
    } else if b_over_a <= 0.618 * (1.0 + FIB_TOL) {
        findings.push(finding(
            WaveAbc::B,
            AdvisorySeverity::Info,
            format!(
                "Ch11 Zigzag wave-b:b = {:.1}% × a ≤ 61.8%(spec line 2332)",
                b_over_a * 100.0
            ),
        ));
    }

    // ── Wave-c 規則 ─────────────────────────────────────────────────────
    if in_triangle {
        // Appendix B 項 F + spec line 2338-2342:c 任意方向放寬,不強制超出區間
        findings.push(finding(
            WaveAbc::C,
            AdvisorySeverity::Info,
            format!(
                "Ch11 Zigzag wave-c:Zigzag 在 Triangle 內(`in_triangle_context = true`),c = {:.1}% × a 任意方向放寬(Appendix B 項 F + spec line 2338-2342)",
                c_over_a * 100.0
            ),
        ));
        // 若 c 超出 [61.8%, 161.8%] 區間 → Triangle 形成的強烈訊號(spec line 2341)
        if c_over_a < 0.618 * (1.0 - FIB_TOL) || c_over_a > 1.618 * (1.0 + FIB_TOL) {
            findings.push(finding(
                WaveAbc::C,
                AdvisorySeverity::Strong,
                format!(
                    "Ch11 Zigzag wave-c:c = {:.1}% × a 超出 [61.8%, 161.8%] 區間 + in_triangle_context — 強烈暗示 1-2 個更高級 Triangle 形成中(spec line 2341)",
                    c_over_a * 100.0
                ),
            ));
        }
    } else {
        // 不在 Triangle 內:c 在 [61.8%, 161.8%] × a 區間(spec line 2337)
        let in_range = c_over_a >= 0.618 * (1.0 - FIB_TOL)
            && c_over_a <= 1.618 * (1.0 + FIB_TOL);
        if !in_range {
            findings.push(finding(
                WaveAbc::C,
                AdvisorySeverity::Warning,
                format!(
                    "Ch11 Zigzag wave-c:c = {:.1}% × a 不在 [61.8%, 161.8%] 區間(spec line 2337)— 暗示更大 Triangle 形成或 Elongated Zigzag",
                    c_over_a * 100.0
                ),
            ));
        }
        // c > 161.8% × a:Elongated Zigzag(spec line 2342)
        if c_over_a > 1.618 * (1.0 + FIB_TOL) {
            findings.push(finding(
                WaveAbc::C,
                AdvisorySeverity::Info,
                format!(
                    "Ch11 Zigzag wave-c:c = {:.1}% × a > 161.8% — Elongated Zigzag 變體(spec line 2342)",
                    c_over_a * 100.0
                ),
            ));
        }
    }

    findings
}

fn finding(wave: WaveAbc, severity: AdvisorySeverity, message: String) -> AdvisoryFinding {
    AdvisoryFinding {
        rule_id: RuleId::Ch11_Zigzag_WaveByWave { wave },
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
        }
    }

    fn make_scenario_zigzag(classified: &[ClassifiedMonowave], in_triangle: bool) -> Scenario {
        Scenario {
            id: "test".to_string(),
            wave_tree: WaveNode {
                label: "test".to_string(),
                start: classified.first().unwrap().monowave.start_date,
                end: classified.last().unwrap().monowave.end_date,
                children: Vec::new(),
            },
            pattern_type: NeelyPatternType::Zigzag {
                sub_kind: ZigzagKind::Single,
            },
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
            in_triangle_context: in_triangle,
            awaiting_l_label: false,
            monowave_structure_labels: Vec::new(),
            round_state: RoundState::Round1,
            pattern_isolation_anchors: Vec::new(),
            triplexity_detected: false,
        }
    }

    #[test]
    fn no_findings_for_non_zigzag() {
        let waves = vec![cmw(10.0, 5, 0), cmw(5.0, 3, 5), cmw(15.0, 4, 8)];
        let mut scenario = make_scenario_zigzag(&waves, false);
        scenario.pattern_type = NeelyPatternType::Impulse;
        let findings = analyze(&scenario, &waves);
        assert!(findings.is_empty());
    }

    #[test]
    fn zigzag_wave_a_warns_when_b_retraces_over_81pct() {
        // a=10, b=9 (90%) → Missing Wave Rule 警告
        let waves = vec![cmw(10.0, 5, 0), cmw(9.0, 3, 5), cmw(8.0, 4, 8)];
        let scenario = make_scenario_zigzag(&waves, false);
        let findings = analyze(&scenario, &waves);
        let a_warn = findings.iter().any(|f| {
            matches!(
                f.rule_id,
                RuleId::Ch11_Zigzag_WaveByWave { wave: WaveAbc::A }
            ) && matches!(f.severity, AdvisorySeverity::Warning)
                && f.message.contains("Missing Wave Rule")
        });
        assert!(a_warn);
    }

    #[test]
    fn zigzag_wave_b_warns_when_in_61_8_to_81_range() {
        // a=10, b=7 (70%) — 61.8-81% range → Warning「wave-b 內部 wave-a」
        let waves = vec![cmw(10.0, 5, 0), cmw(7.0, 3, 5), cmw(8.0, 4, 8)];
        let scenario = make_scenario_zigzag(&waves, false);
        let findings = analyze(&scenario, &waves);
        let b_warn = findings.iter().any(|f| {
            matches!(
                f.rule_id,
                RuleId::Ch11_Zigzag_WaveByWave { wave: WaveAbc::B }
            ) && matches!(f.severity, AdvisorySeverity::Warning)
                && f.message.contains("wave-a")
        });
        assert!(b_warn);
    }

    #[test]
    fn zigzag_wave_c_warns_when_outside_range_no_triangle() {
        // a=10, b=5, c=2 (c/a=20%) → 不在 [61.8%, 161.8%] → Warning
        let waves = vec![cmw(10.0, 5, 0), cmw(5.0, 3, 5), cmw(2.0, 4, 8)];
        let scenario = make_scenario_zigzag(&waves, false);
        let findings = analyze(&scenario, &waves);
        let c_warn = findings.iter().any(|f| {
            matches!(
                f.rule_id,
                RuleId::Ch11_Zigzag_WaveByWave { wave: WaveAbc::C }
            ) && matches!(f.severity, AdvisorySeverity::Warning)
                && f.message.contains("61.8")
        });
        assert!(c_warn);
    }

    #[test]
    fn zigzag_in_triangle_strong_when_c_outside_range() {
        // a=10, b=5, c=2 (c/a=20% < 61.8%) + in_triangle → Strong (Triangle 形成訊號)
        let waves = vec![cmw(10.0, 5, 0), cmw(5.0, 3, 5), cmw(2.0, 4, 8)];
        let scenario = make_scenario_zigzag(&waves, true);
        let findings = analyze(&scenario, &waves);
        let c_strong = findings.iter().any(|f| {
            matches!(
                f.rule_id,
                RuleId::Ch11_Zigzag_WaveByWave { wave: WaveAbc::C }
            ) && matches!(f.severity, AdvisorySeverity::Strong)
                && f.message.contains("Triangle")
        });
        assert!(c_strong);
    }

    #[test]
    fn zigzag_elongated_info_when_c_over_161_8() {
        // a=10, b=5, c=20 (c/a=200%) → Elongated info
        let waves = vec![cmw(10.0, 5, 0), cmw(5.0, 3, 5), cmw(20.0, 4, 8)];
        let scenario = make_scenario_zigzag(&waves, false);
        let findings = analyze(&scenario, &waves);
        let elongated = findings.iter().any(|f| {
            matches!(
                f.rule_id,
                RuleId::Ch11_Zigzag_WaveByWave { wave: WaveAbc::C }
            ) && matches!(f.severity, AdvisorySeverity::Info)
                && f.message.contains("Elongated Zigzag")
        });
        assert!(elongated);
    }

    #[test]
    fn zigzag_in_triangle_info_when_c_in_range() {
        // a=10, b=5, c=10 (c/a=100%) + in_triangle → Info (放寬,no warning)
        let waves = vec![cmw(10.0, 5, 0), cmw(5.0, 3, 5), cmw(10.0, 4, 8)];
        let scenario = make_scenario_zigzag(&waves, true);
        let findings = analyze(&scenario, &waves);
        // 在 Triangle 內 → 應有 c-wave Info 標示放寬,無 Warning
        let c_info = findings.iter().any(|f| {
            matches!(
                f.rule_id,
                RuleId::Ch11_Zigzag_WaveByWave { wave: WaveAbc::C }
            ) && matches!(f.severity, AdvisorySeverity::Info)
                && f.message.contains("Appendix B")
        });
        assert!(c_info);
        // 不應有 c-wave Warning
        let c_warn = findings.iter().any(|f| {
            matches!(
                f.rule_id,
                RuleId::Ch11_Zigzag_WaveByWave { wave: WaveAbc::C }
            ) && matches!(f.severity, AdvisorySeverity::Warning)
        });
        assert!(!c_warn);
    }
}
