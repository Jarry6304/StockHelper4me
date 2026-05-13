// triggers — Stage 10c:Invalidation Triggers 生成
//
// 對齊 m3Spec/neely_core_architecture.md §9.4 + neely_rules.md Ch12 EarlyWarning。
// Trigger 是 Neely 規則的逆向轉譯,寫進 Scenario.invalidation_triggers。
//
// 設計原則(§9.4):
//   - on_trigger 移除 ReduceProbability(機率語意),改 WeakenScenario
//
// **PR-3c-pre 階段(2026-05-13)**:
//   - r2 Diagonal → TerminalImpulse(§9.6 取代 Prechter 派術語)
//   - r2 RuleId::Core(N) → RuleId::Ch5Essential(N)(r5 §9.3 chapter-based)
//   - 對 Impulse Scenario:R1-derived + R3-derived triggers(W1 起點 / W1 高點不可破)
//   - 對 TerminalImpulse:R1-derived only(R3 在 Terminal 是 deferred,容許 W4-W1 重疊)
//   - 完整 trigger 規則集留 PR-6b-3 對齊 neely_rules.md Ch12

use crate::output::{
    NeelyPatternType, OnTriggerAction, RuleId, Scenario, Trigger, TriggerType,
};

/// 從 Scenario 推算 invalidation triggers。
///
/// **PR-3c-pre 階段** best-guess:
///   - Impulse:R1-derived trigger(W2 不跨 W1 起點)+ R3-derived trigger(W4 不重疊 W1)
///   - TerminalImpulse:R1-derived trigger only(R3 deferred,允許 W4-W1 重疊)
///   - 其他 pattern:暫不生成 trigger(留 PR-6b-3)
pub fn build_triggers(scenario: &Scenario) -> Vec<Trigger> {
    let mut triggers = Vec::new();

    // 從 wave_tree.children 取出 W1 起點 date 作為「事件參考點」
    // 注意:wave_tree 沒帶 price,完整 trigger 細節留 PR-6b-3 接 monowave_series
    if scenario.wave_tree.children.is_empty() {
        return triggers;
    }

    match &scenario.pattern_type {
        NeelyPatternType::Impulse => {
            // R1-derived:W2 endpoint 不可跨 W1 起點 → 跨過則 invalidate
            triggers.push(Trigger {
                trigger_type: TriggerType::PriceBreakBelow(0.0), // PR-6b-3 補實際 price
                on_trigger: OnTriggerAction::InvalidateScenario,
                rule_reference: RuleId::Ch5Essential(1),
                neely_page: "Ch5 Essential R1 — W2 不可完全回測 W1(具體書頁待 PR-6b-3)".to_string(),
            });
            // R3-derived:W4 不重疊 W1
            triggers.push(Trigger {
                trigger_type: TriggerType::PriceBreakBelow(0.0), // PR-6b-3 補實際 W1 高 price
                on_trigger: OnTriggerAction::WeakenScenario, // R3 fail → 改 TerminalImpulse,不直接 invalidate
                rule_reference: RuleId::Ch5Essential(3),
                neely_page: "Ch5 Essential R3 — W4 不可重疊 W1(具體書頁待 PR-6b-3)".to_string(),
            });
        }
        NeelyPatternType::TerminalImpulse => {
            // TerminalImpulse:R1-derived trigger only(R3 deferred)
            triggers.push(Trigger {
                trigger_type: TriggerType::PriceBreakBelow(0.0),
                on_trigger: OnTriggerAction::InvalidateScenario,
                rule_reference: RuleId::Ch5Essential(1),
                neely_page: "Ch5 Essential R1 — TerminalImpulse 容許 W4-W1 重疊".to_string(),
            });
        }
        // Zigzag / Flat / Triangle / Combination / RunningCorrection:trigger 規則留 PR-6b-3
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
            post_pattern_behavior: PostBehavior::Unconstrained,
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
        // 第 2 個是 R3(WeakenScenario,降級為 TerminalImpulse)
        assert!(matches!(
            triggers[1].on_trigger,
            OnTriggerAction::WeakenScenario
        ));
    }

    #[test]
    fn terminal_impulse_gets_one_trigger() {
        let s = make_scenario(NeelyPatternType::TerminalImpulse);
        let triggers = build_triggers(&s);
        assert_eq!(triggers.len(), 1);
    }

    #[test]
    fn zigzag_gets_no_trigger_yet() {
        let s = make_scenario(NeelyPatternType::Zigzag { sub_kind: ZigzagVariant::Normal });
        let triggers = build_triggers(&s);
        assert!(triggers.is_empty());
    }
}
