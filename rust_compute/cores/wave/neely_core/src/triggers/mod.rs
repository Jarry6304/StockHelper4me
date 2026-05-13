// triggers — Stage 10c:Invalidation Triggers 生成
//
// 對齊 m3Spec/neely_core_architecture.md §7.1 Stage 10 + §9.4 OnTriggerAction。
// Trigger 是 Neely 規則的逆向轉譯,寫進 Scenario.invalidation_triggers。
//
// 設計原則(architecture §9.4):
//   - on_trigger 不引入機率語意,只有 InvalidateScenario / WeakenScenario / PromoteAlternative
//
// **Phase 1 PR(r5)**:RuleId 改用 Ch5_Essential / Ch5_Overlap_* 對齊章節編碼
//   - 對 Impulse Scenario 自動加入「W2 不可跨過 W1 起點」R3 (Ch5_Essential(3))-derived trigger
//     + 「W4 不可進入 W2 區」Ch5_Overlap_Trending-derived trigger
//   - 對 Diagonal Scenario 加入較鬆 trigger(只 R3-derived,因 Overlap_Trending 已 fail)
//   - 完整 trigger 規則集留 P10(Stage 10 完整 + Ch12 Fibonacci / Waterfall)對齊 Neely 書頁

use crate::output::{
    NeelyPatternType, OnTriggerAction, RuleId, Scenario, Trigger, TriggerType,
};

/// 從 Scenario 推算 invalidation triggers。
///
/// **Phase 1 PR(r5)** best-guess:
/// - Impulse:Ch5_Essential(3)-derived trigger(W2 不跨 W1 起點)
///   + Ch5_Overlap_Trending-derived trigger(W4 不進入 W2 區)
/// - Diagonal:Ch5_Essential(3)-derived trigger only(Overlap_Trending 在 Diagonal 已 fail,不再 enforce)
/// - 其他 pattern:暫不生成 trigger(留 P10)
pub fn build_triggers(scenario: &Scenario) -> Vec<Trigger> {
    let mut triggers = Vec::new();

    // 從 wave_tree.children 取出 W1 起點 date 作為「事件參考點」
    // 注意:wave_tree 沒帶 price,完整 trigger 細節留 P10 接 monowave_series
    if scenario.wave_tree.children.is_empty() {
        return triggers;
    }

    match &scenario.pattern_type {
        NeelyPatternType::Impulse => {
            // Ch5_Essential(3)-derived:W2 endpoint 不可跨 W1 起點 → 跨過則 invalidate
            triggers.push(Trigger {
                trigger_type: TriggerType::PriceBreakBelow(0.0), // P10 補實際 price
                on_trigger: OnTriggerAction::InvalidateScenario,
                rule_reference: RuleId::Ch5_Essential(3),
                neely_page: "neely_rules.md §Impulsion 第 3 條(p.5-2~5-3)".to_string(),
            });
            // Ch5_Overlap_Trending-derived:W4 不進入 W2 區
            triggers.push(Trigger {
                trigger_type: TriggerType::PriceBreakBelow(0.0), // P10 補實際 W2 終點 price
                on_trigger: OnTriggerAction::WeakenScenario, // Overlap_Trending fail → 降級為 Diagonal
                rule_reference: RuleId::Ch5_Overlap_Trending,
                neely_page: "neely_rules.md §Overlap Rule(1326-1329 行)".to_string(),
            });
        }
        NeelyPatternType::Diagonal { .. } => {
            // Diagonal:Ch5_Essential(3) trigger only(Overlap 規則已不在 Diagonal 假設下 enforce)
            triggers.push(Trigger {
                trigger_type: TriggerType::PriceBreakBelow(0.0),
                on_trigger: OnTriggerAction::InvalidateScenario,
                rule_reference: RuleId::Ch5_Essential(3),
                neely_page: "neely_rules.md §Impulsion 第 3 條(Diagonal 仍須 W2 不過 W1 起點)".to_string(),
            });
        }
        // Zigzag / Flat / Triangle / Combination:trigger 規則留 P10
        _ => {}
    }

    triggers
}

/// 對 Forest 中所有 Scenario 套 triggers,直接更新 invalidation_triggers 欄位。
pub fn apply_to_forest(forest: &mut [Scenario]) {
    for scenario in forest.iter_mut() {
        scenario.invalidation_triggers = build_triggers(scenario);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::*;
    use chrono::NaiveDate;

    fn make_scenario(pattern: NeelyPatternType) -> Scenario {
        let date = NaiveDate::parse_from_str("2026-01-01", "%Y-%m-%d").unwrap();
        Scenario {
            id: "test".to_string(),
            wave_tree: WaveNode {
                label: "test".to_string(),
                start: date,
                end: date,
                children: vec![WaveNode {
                    label: "W1".to_string(),
                    start: date,
                    end: date,
                    children: Vec::new(),
                }],
            },
            pattern_type: pattern,
            initial_direction: MonowaveDirection::Up,
            compacted_base_label: StructureLabel::Five,
            structure_label: "test".to_string(),
            complexity_level: ComplexityLevel::Simple,
            power_rating: PowerRating::Neutral,
            max_retracement: 0.0,
            post_pattern_behavior: PostBehavior::Indeterminate,
            passed_rules: Vec::new(),
            deferred_rules: Vec::new(),
            rules_passed_count: 0,
            deferred_rules_count: 0,
            invalidation_triggers: Vec::new(),
            expected_fib_zones: Vec::new(),
            structural_facts: StructuralFacts::default(),
        }
    }

    #[test]
    fn impulse_gets_two_triggers() {
        let s = make_scenario(NeelyPatternType::Impulse);
        let triggers = build_triggers(&s);
        assert_eq!(triggers.len(), 2);
        // 第 1 個是 R1(InvalidateScenario)
        assert!(matches!(
            triggers[0].on_trigger,
            OnTriggerAction::InvalidateScenario
        ));
        // 第 2 個是 R3(WeakenScenario,降級為 Diagonal)
        assert!(matches!(
            triggers[1].on_trigger,
            OnTriggerAction::WeakenScenario
        ));
    }

    #[test]
    fn diagonal_gets_one_trigger() {
        let s = make_scenario(NeelyPatternType::Diagonal {
            sub_kind: DiagonalKind::Leading,
        });
        let triggers = build_triggers(&s);
        assert_eq!(triggers.len(), 1);
    }

    #[test]
    fn zigzag_gets_no_trigger_yet() {
        let s = make_scenario(NeelyPatternType::Zigzag {
            sub_kind: ZigzagKind::Single,
        });
        let triggers = build_triggers(&s);
        assert!(triggers.is_empty());
    }
}
