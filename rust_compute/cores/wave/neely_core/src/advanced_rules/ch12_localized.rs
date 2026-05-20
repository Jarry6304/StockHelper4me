// ch12_localized.rs — Ch12 Localized Progress Label Changes 偵測
//
// 對齊 m3Spec/neely_rules.md §Ch12 Localized Changes(原 P10+ 留空)。
//
// **語意**(spec §Ch12):
// 在更大級結構展開過程中,先前被標記為 5-wave Impulse 的子段(例如 wave-5 of larger
// pattern)可能在 Three Rounds 後續迭代中被「降級」為更大級的 wave-1(或 wave-c 等)。
// 這種 progress label 在區域層級的變動,本身是 NEoWave 動態 Three Rounds 過程的特徵 —
// 不是錯誤,而是 spec 明文 acknowledge 的 localized adjustment。
//
// **v4.2 P1.2 落地**(2026-05-19):
//   - Best-guess 偵測:scenario 同時為 5-wave Impulse + `in_triangle_context = true`
//     (表示該 scenario 本身被歸為更大 Triangle 的內部段,即「Impulse 被 demoted 為
//     Triangle 的某一 leg」)→ 觸發 Localized Changes
//   - 或者 scenario 的 compacted_base_label = `:5` 但 `monowave_structure_labels`
//     中含 `:c3` 或 `:x` 標籤(label 已被當下 round 改動)→ 觸發
//
// **Advisory 用途**:Info severity。LLM 看 narrative 理解「該 scenario 處於 Three Rounds
// 過程的中段,label 可能再變動」。

use crate::output::{
    AdvisoryFinding, AdvisorySeverity, NeelyPatternType, RuleId, Scenario, StructureLabel,
};

/// 偵測 scenario 是否符合 Localized Progress Label Changes 場景。
///
/// 回傳 `Some(AdvisoryFinding)` Info severity 表示偵測到 localized adjustment;
/// `None` 表示 label 結構穩定無變動。
pub fn detect_localized_changes(scenario: &Scenario) -> Option<AdvisoryFinding> {
    // Case A:Impulse 被歸為 Triangle 內部 leg(in_triangle_context = true)→ label 變動
    if matches!(scenario.pattern_type, NeelyPatternType::Impulse) && scenario.in_triangle_context {
        return Some(AdvisoryFinding {
            rule_id: RuleId::Ch12_LocalizedChanges,
            severity: AdvisorySeverity::Info,
            message: format!(
                "Ch12 Localized Changes:5-wave Impulse 在 in_triangle_context — 該 scenario 是更大 Triangle 的內部 leg,label 在 Three Rounds 過程中已 demoted(spec §12 Localized Changes)"
            ),
        });
    }

    // Case B:compacted_base_label = Five 但 monowave_structure_labels 中含 :x / :c3 / :L5
    // (表示初始 5-wave label 已被更新為「複雜結構元素」)
    if matches!(scenario.compacted_base_label, StructureLabel::Five) {
        let has_complex_labels = scenario.monowave_structure_labels.iter().any(|m| {
            m.labels.iter().any(|c| {
                matches!(
                    c.label,
                    StructureLabel::XC3
                        | StructureLabel::BC3
                        | StructureLabel::BF3
                        | StructureLabel::L5
                        | StructureLabel::SL3
                )
            })
        });
        if has_complex_labels {
            return Some(AdvisoryFinding {
                rule_id: RuleId::Ch12_LocalizedChanges,
                severity: AdvisorySeverity::Info,
                message: "Ch12 Localized Changes:compacted_base = :5 但 monowave 標籤含 :xc3 / :bc3 / :bf3 / :L5 / :sL3 — label 在 Three Rounds 中已局部變動".to_string(),
            });
        }
    }

    // awaiting_l_label = true 也是局部變動信號(L 標籤待補)
    if scenario.awaiting_l_label {
        return Some(AdvisoryFinding {
            rule_id: RuleId::Ch12_LocalizedChanges,
            severity: AdvisorySeverity::Info,
            message: "Ch12 Localized Changes:scenario.awaiting_l_label = true — :L3 / :L5 標籤待 Three Rounds 後續迭代補完".to_string(),
        });
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::*;
    use chrono::NaiveDate;

    fn make_scenario(pattern: NeelyPatternType, in_triangle: bool) -> Scenario {
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
            in_triangle_context: in_triangle,
            awaiting_l_label: false,
            monowave_structure_labels: Vec::new(),
            round_state: RoundState::Round1,
            pattern_isolation_anchors: Vec::new(),
            triplexity_detected: false,
        }
    }

    #[test]
    fn localized_fires_for_impulse_in_triangle_context() {
        let scenario = make_scenario(NeelyPatternType::Impulse, true);
        let f = detect_localized_changes(&scenario).expect("should fire");
        assert!(matches!(f.severity, AdvisorySeverity::Info));
        assert!(matches!(f.rule_id, RuleId::Ch12_LocalizedChanges));
    }

    #[test]
    fn localized_fires_for_complex_label_in_five_base() {
        let mut scenario = make_scenario(NeelyPatternType::Impulse, false);
        scenario.monowave_structure_labels.push(MonowaveStructureLabels {
            monowave_index: 0,
            classified_index: 0,
            labels: vec![StructureLabelCandidate {
                label: StructureLabel::L5,
                certainty: Certainty::Rare,
            }],
            pass1_only_labels: Vec::new(),
        });
        let f = detect_localized_changes(&scenario).expect("should fire");
        assert!(matches!(f.rule_id, RuleId::Ch12_LocalizedChanges));
    }

    #[test]
    fn localized_fires_for_awaiting_l_label() {
        let mut scenario = make_scenario(NeelyPatternType::Impulse, false);
        scenario.awaiting_l_label = true;
        let f = detect_localized_changes(&scenario).expect("should fire");
        assert!(matches!(f.rule_id, RuleId::Ch12_LocalizedChanges));
    }

    #[test]
    fn localized_returns_none_for_clean_impulse() {
        let scenario = make_scenario(NeelyPatternType::Impulse, false);
        assert!(detect_localized_changes(&scenario).is_none());
    }
}
