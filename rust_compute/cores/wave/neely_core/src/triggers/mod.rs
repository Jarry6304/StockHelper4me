// triggers — Stage 10c:Invalidation Triggers 生成
//
// 對齊 m2Spec/oldm2Spec/neely_core.md §三 / §七 Stage 10 / §9.2。
// Trigger 是 Neely 規則的逆向轉譯,寫進 Scenario.invalidation_triggers。
//
// 設計原則(§9.4):
//   - on_trigger 移除 ReduceProbability(機率語意),改 WeakenScenario
//
// **M3 PR-6 階段**(先實踐以後再改):
//   - 對 Impulse Scenario 自動加入「W2 不可跨過 W1 起點」的 R1-derived trigger
//   - 對 Diagonal Scenario 加入較鬆 trigger
//   - 完整 trigger 規則集留 PR-6b 對齊 Neely 書頁

use crate::output::{
    NeelyPatternType, OnTriggerAction, RuleId, Scenario, Trigger, TriggerType,
};

/// 從 Scenario 推算 invalidation triggers。
///
/// **M3 PR-6 階段** best-guess:
///   - Impulse:R1-derived trigger(W2 不跨 W1 起點)+ R3-derived trigger(W4 不重疊 W1)
///   - Diagonal:R1-derived trigger only(R3 在 Diagonal 是 deferred)
///   - 其他 pattern:暫不生成 trigger(留 PR-6b)
pub fn build_triggers(scenario: &Scenario) -> Vec<Trigger> {
    let mut triggers = Vec::new();

    // 從 wave_tree.children 取出 W1 起點 date 作為「事件參考點」
    // 注意:wave_tree 沒帶 price,完整 trigger 細節留 PR-6b 接 monowave_series
    if scenario.wave_tree.children.is_empty() {
        return triggers;
    }

    match &scenario.pattern_type {
        NeelyPatternType::Impulse => {
            // R1-derived:W2 endpoint 不可跨 W1 起點 → 跨過則 invalidate
            triggers.push(Trigger {
                trigger_type: TriggerType::PriceBreakBelow(0.0), // PR-6b 補實際 price
                on_trigger: OnTriggerAction::InvalidateScenario,
                rule_reference: RuleId::Core(1),
                neely_page: "R1 — Elliott Wave 通用規則(具體 Neely 書頁待 m3Spec/ 校準)".to_string(),
            });
            // R3-derived:W4 不重疊 W1
            triggers.push(Trigger {
                trigger_type: TriggerType::PriceBreakBelow(0.0), // PR-6b 補實際 W1 高 price
                on_trigger: OnTriggerAction::WeakenScenario, // R3 fail → 改 Diagonal,不直接 invalidate
                rule_reference: RuleId::Core(3),
                neely_page: "R3 — Elliott Wave 通用規則(具體 Neely 書頁待 m3Spec/ 校準)".to_string(),
            });
        }
        NeelyPatternType::Diagonal { .. } => {
            // Diagonal:R1-derived trigger only
            triggers.push(Trigger {
                trigger_type: TriggerType::PriceBreakBelow(0.0),
                on_trigger: OnTriggerAction::InvalidateScenario,
                rule_reference: RuleId::Core(1),
                neely_page: "R1 — Elliott Wave 通用規則(Diagonal 容許 W4-W1 重疊)".to_string(),
            });
        }
        // Zigzag / Flat / Triangle / Combination:trigger 規則留 PR-6b
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
