// ch8_xwave — Ch8 X-wave Internal Structure Detection(advisory mode)
//
// 對齊 m3Spec/neely_rules.md 第 8 章 §Standard / Non-Standard Complex Polywave
// (line 1852-1856)+ §X-wave internal structure。
//
// **v4.4b 落地**(2026-05-19):
//   - 對 `NeelyPatternType::Combination { sub_kinds }` 偵測 X-wave 內部結構
//   - X-wave 連接兩個 sub-correction(例:Double Zigzag = Zigzag + x + Zigzag)
//   - Advisory only:寫 AdvisoryFinding,不 invalidate scenario
//
// **X-wave 規則**(spec §8 X-wave):
//   - X-wave 結構通常為 :3(corrective)
//   - 大 X-wave 場景中所有構成段只能是 Flat (3-3-5) 或 Contracting Triangle
//     (對齊 spec Ch8 Table B 修正,2026-05 v1.5 補完)
//   - 小 X-wave 場景允許 Zigzag (5-3-5)
//
// **Best-guess 偵測**:
//   - DoubleZigzag / DoubleCombination / DoubleFlat / TripleZigzag / TripleCombination
//     → 有小 X-wave(allow Zigzag in components)
//   - DoubleThree / DoubleThreeCombination / DoubleThreeRunning / TripleThree
//     / TripleThreeCombination / TripleThreeRunning → 大 X-wave(只允許 Flat/Triangle)

use crate::output::{
    AdvisoryFinding, AdvisorySeverity, CombinationKind, NeelyPatternType, RuleId, Scenario,
};

/// 對 Combination scenario 偵測 X-wave internal structure。
pub fn detect(scenario: &Scenario) -> Vec<AdvisoryFinding> {
    let mut findings = Vec::new();
    let sub_kinds = match &scenario.pattern_type {
        NeelyPatternType::Combination { sub_kinds } => sub_kinds,
        _ => return findings,
    };

    if sub_kinds.is_empty() {
        return findings;
    }

    let large_x_wave = sub_kinds.iter().any(is_large_x_wave_kind);
    let small_x_wave = sub_kinds.iter().any(is_small_x_wave_kind);

    if large_x_wave {
        findings.push(AdvisoryFinding {
            rule_id: RuleId::Ch8_LargeXWave_NoZigzag,
            severity: AdvisorySeverity::Info,
            message: format!(
                "Ch8 Large X-wave 場景:Combination 含 {} 變體 — 所有構成段只能是 Flat (3-3-5) 或 Contracting Triangle(spec Ch8 Table B 修正,line 1856)",
                sub_kinds.len()
            ),
        });
    } else if small_x_wave {
        findings.push(AdvisoryFinding {
            rule_id: RuleId::Ch8_XWave_InternalStructure,
            severity: AdvisorySeverity::Info,
            message: format!(
                "Ch8 Small X-wave 場景:Combination 含 {} 變體 — 允許 Zigzag (5-3-5) 構成段(spec Ch8 Table A)",
                sub_kinds.len()
            ),
        });
    }

    findings
}

fn is_large_x_wave_kind(k: &CombinationKind) -> bool {
    matches!(
        k,
        CombinationKind::DoubleThree
            | CombinationKind::DoubleThreeCombination
            | CombinationKind::DoubleThreeRunning
            | CombinationKind::TripleThree
            | CombinationKind::TripleThreeCombination
            | CombinationKind::TripleThreeRunning
    )
}

fn is_small_x_wave_kind(k: &CombinationKind) -> bool {
    matches!(
        k,
        CombinationKind::DoubleZigzag
            | CombinationKind::DoubleCombination
            | CombinationKind::DoubleFlat
            | CombinationKind::TripleZigzag
            | CombinationKind::TripleCombination
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::*;
    use chrono::NaiveDate;

    fn make_scenario(pattern: NeelyPatternType) -> Scenario {
        let date = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        Scenario {
            id: "test".to_string(),
            wave_tree: WaveNode {
                label: "test".to_string(),
                start: date,
                end: date,
                children: Vec::new(),
            },
            pattern_type: pattern,
            initial_direction: MonowaveDirection::Up,
            compacted_base_label: StructureLabel::Three,
            structure_label: "test".to_string(),
            complexity_level: ComplexityLevel::Complex,
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
    fn no_findings_for_non_combination() {
        let scenario = make_scenario(NeelyPatternType::Impulse);
        assert!(detect(&scenario).is_empty());
    }

    #[test]
    fn small_x_wave_for_double_zigzag() {
        let scenario = make_scenario(NeelyPatternType::Combination {
            sub_kinds: vec![CombinationKind::DoubleZigzag],
        });
        let findings = detect(&scenario);
        assert!(findings.iter().any(|f| matches!(
            f.rule_id,
            RuleId::Ch8_XWave_InternalStructure
        )));
    }

    #[test]
    fn large_x_wave_for_double_three() {
        let scenario = make_scenario(NeelyPatternType::Combination {
            sub_kinds: vec![CombinationKind::DoubleThree],
        });
        let findings = detect(&scenario);
        assert!(findings.iter().any(|f| matches!(
            f.rule_id,
            RuleId::Ch8_LargeXWave_NoZigzag
        )));
    }
}
