// ch8_multiwave — Ch8 Multiwave 建構偵測(advisory mode)
//
// 對齊 m3Spec/neely_rules.md 第 8 章 §Multiwave 建構(line 1908-1912)+ §Complex
// Multiwaves / Macrowaves(line 1912)。
//
// **v4.4c 落地**(2026-05-19):
//   - 對 `NeelyPatternType::Combination { sub_kinds }` 偵測是否為 Multiwave 結構
//   - Multiwave = 兩個以上 corrective polywave 由 x-wave 連接(Double/Triple Three 等)
//   - Macrowave = Multiwave 在更大 Complex 結構中
//   - Advisory only:寫 AdvisoryFinding,不 invalidate scenario
//
// **辨識規則**(spec §8 Multiwave):
//   - sub_kinds 長度 ≥ 1 表示有 x-wave 連接(本實作每個 sub_kind 代表一段子修正)
//   - Triple* 變體屬 Multiwave 末段(對齊 reverse_logic「near completion」)
//   - Double* 變體屬 Multiwave 中段

use crate::output::{
    AdvisoryFinding, AdvisorySeverity, CombinationKind, NeelyPatternType, RuleId, Scenario,
};

/// 對 Combination scenario 偵測 Multiwave 建構。
pub fn detect(scenario: &Scenario) -> Vec<AdvisoryFinding> {
    let mut findings = Vec::new();
    let sub_kinds = match &scenario.pattern_type {
        NeelyPatternType::Combination { sub_kinds } => sub_kinds,
        _ => return findings,
    };

    if sub_kinds.is_empty() {
        return findings;
    }

    let is_triple = sub_kinds.iter().any(|k| {
        matches!(
            k,
            CombinationKind::TripleZigzag
                | CombinationKind::TripleCombination
                | CombinationKind::TripleThree
                | CombinationKind::TripleThreeCombination
                | CombinationKind::TripleThreeRunning
        )
    });

    let position_hint = if is_triple {
        "末段(Multiwave 接近完成,後續走勢即將反向)"
    } else {
        "中段(Multiwave 進行中,可能再延長一級到 Triple)"
    };

    findings.push(AdvisoryFinding {
        rule_id: RuleId::Ch8_Multiwave_Construction,
        severity: AdvisorySeverity::Info,
        message: format!(
            "Ch8 Multiwave 建構:Combination 含 {} 段 corrective {} 由 x-wave 連接 — {}(spec 8 章 line 1908-1912)",
            sub_kinds.len(),
            if is_triple { "(Triple 變體)" } else { "(Double 變體)" },
            position_hint
        ),
    });

    findings
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
    fn double_yields_mid_multiwave() {
        let scenario = make_scenario(NeelyPatternType::Combination {
            sub_kinds: vec![CombinationKind::DoubleZigzag],
        });
        let findings = detect(&scenario);
        assert!(findings.iter().any(|f| f.message.contains("中段")));
    }

    #[test]
    fn triple_yields_end_multiwave() {
        let scenario = make_scenario(NeelyPatternType::Combination {
            sub_kinds: vec![CombinationKind::TripleZigzag],
        });
        let findings = detect(&scenario);
        assert!(findings.iter().any(|f| f.message.contains("末段")));
    }
}
