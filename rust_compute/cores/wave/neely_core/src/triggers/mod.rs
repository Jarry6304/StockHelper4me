// triggers — Stage 10c:Invalidation Triggers 生成
//
// 對齊 m3Spec/neely_core_architecture.md §7.1 Stage 10c + §9.4 OnTriggerAction
//       + m3Spec/neely_rules.md §Impulsion + §Overlap Rule
//
// 設計原則(architecture §9.4):
//   - on_trigger 不引入機率語意,只有 InvalidateScenario / WeakenScenario / PromoteAlternative
//
// **Phase 10 PR(r5 alignment)— 接 monowave price**:
//   - Triggers 從 scenario.wave_tree.children 反查 monowave_series 取得實際 price
//   - Impulse Up:
//       * Ch5_Essential(3) trigger:`PriceBreakBelow(W1.start_price)`(W2 不可完全回測 W1)
//       * Ch5_Overlap_Trending trigger:`PriceBreakBelow(W2.end_price)`(W4 不進入 W2 區)
//   - Impulse Down:對稱方向(`PriceBreakAbove`)
//   - Diagonal:只生 Ch5_Essential(3) trigger(Overlap_Trending 在 Diagonal 已 fail)
//   - Zigzag / Flat / Triangle / Combination:trigger 規則留後續 PR 完整 Ch12

use crate::output::{
    Monowave, MonowaveDirection, NeelyPatternType, OnTriggerAction, RuleId, Scenario, Trigger,
    TriggerType, WaveNode,
};

/// 從 Scenario + monowave_series 推算 invalidation triggers。
///
/// **Phase 10**:用 wave_tree.children 對應的 monowave dates 反查實際 price。
/// 找不到對應 monowave → 該 trigger 用 placeholder 0.0(視為退化保險,不阻塞)。
pub fn build_triggers(scenario: &Scenario, monowaves: &[Monowave]) -> Vec<Trigger> {
    let mut triggers = Vec::new();

    if scenario.wave_tree.children.is_empty() {
        return triggers;
    }

    let is_up = matches!(scenario.initial_direction, MonowaveDirection::Up);

    match &scenario.pattern_type {
        NeelyPatternType::Impulse => {
            // Ch5_Essential(3):W2 endpoint 不可跨 W1 起點
            // Up impulse → 跌破 W1.start_price 失效
            // Down impulse → 漲破 W1.start_price 失效
            if let Some(w1) = scenario.wave_tree.children.first() {
                let w1_start_price = find_wave_start_price(w1, monowaves);
                triggers.push(Trigger {
                    trigger_type: directional_break(is_up, w1_start_price),
                    on_trigger: OnTriggerAction::InvalidateScenario,
                    rule_reference: RuleId::Ch5_Essential(3),
                    neely_page: "neely_rules.md §Impulsion 第 3 條(p.5-2~5-3)".to_string(),
                });
            }
            // Ch5_Overlap_Trending:W4 不進入 W2 區
            // Up impulse W4 enters W2 → 跌破 W2.end_price
            // Down impulse W4 enters W2 → 漲破 W2.end_price
            if let Some(w2) = scenario.wave_tree.children.get(1) {
                let w2_end_price = find_wave_end_price(w2, monowaves);
                triggers.push(Trigger {
                    trigger_type: directional_break(is_up, w2_end_price),
                    on_trigger: OnTriggerAction::WeakenScenario,
                    rule_reference: RuleId::Ch5_Overlap_Trending,
                    neely_page: "neely_rules.md §Overlap Rule(1326-1329 行)".to_string(),
                });
            }
        }
        NeelyPatternType::Diagonal { .. } => {
            // Diagonal:只生 Ch5_Essential(3) trigger(Overlap_Trending 已 fail,不再 enforce)
            if let Some(w1) = scenario.wave_tree.children.first() {
                let w1_start_price = find_wave_start_price(w1, monowaves);
                triggers.push(Trigger {
                    trigger_type: directional_break(is_up, w1_start_price),
                    on_trigger: OnTriggerAction::InvalidateScenario,
                    rule_reference: RuleId::Ch5_Essential(3),
                    neely_page: "neely_rules.md §Impulsion 第 3 條(Diagonal 仍須 W2 不過 W1 起點)"
                        .to_string(),
                });
            }
        }
        // Zigzag / Flat / Triangle / Combination:留後續 PR 動工(Ch12 Fibonacci/Waterfall + Ch5 變體規則)
        _ => {}
    }

    triggers
}

/// 對 Forest 中所有 Scenario 套 triggers,直接更新 invalidation_triggers 欄位。
pub fn apply_to_forest(forest: &mut [Scenario], monowaves: &[Monowave]) {
    for scenario in forest.iter_mut() {
        scenario.invalidation_triggers = build_triggers(scenario, monowaves);
    }
}

fn directional_break(is_up: bool, price: f64) -> TriggerType {
    if is_up {
        TriggerType::PriceBreakBelow(price)
    } else {
        TriggerType::PriceBreakAbove(price)
    }
}

fn find_wave_start_price(wave: &WaveNode, monowaves: &[Monowave]) -> f64 {
    monowaves
        .iter()
        .find(|m| m.start_date == wave.start && m.end_date == wave.end)
        .map(|m| m.start_price)
        .unwrap_or(0.0)
}

fn find_wave_end_price(wave: &WaveNode, monowaves: &[Monowave]) -> f64 {
    monowaves
        .iter()
        .find(|m| m.start_date == wave.start && m.end_date == wave.end)
        .map(|m| m.end_price)
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::*;
    use chrono::NaiveDate;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn mw(start: &str, end: &str, sp: f64, ep: f64, dir: MonowaveDirection) -> Monowave {
        Monowave {
            start_date: d(start),
            end_date: d(end),
            start_price: sp,
            end_price: ep,
            direction: dir,
        }
    }

    fn make_scenario(
        pattern: NeelyPatternType,
        dir: MonowaveDirection,
        children: Vec<(String, &str, &str)>,
    ) -> Scenario {
        let children: Vec<WaveNode> = children
            .into_iter()
            .map(|(label, s, e)| WaveNode {
                label,
                start: d(s),
                end: d(e),
                children: Vec::new(),
            })
            .collect();
        let date = d("2026-01-01");
        Scenario {
            id: "test".to_string(),
            wave_tree: WaveNode {
                label: "test".to_string(),
                start: date,
                end: date,
                children,
            },
            pattern_type: pattern,
            initial_direction: dir,
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
    fn impulse_up_triggers_use_w1_start_w2_end_prices_as_break_below() {
        let s = make_scenario(
            NeelyPatternType::Impulse,
            MonowaveDirection::Up,
            vec![
                ("W1".into(), "2026-01-01", "2026-01-05"),
                ("W2".into(), "2026-01-06", "2026-01-10"),
            ],
        );
        let monowaves = vec![
            mw("2026-01-01", "2026-01-05", 100.0, 120.0, MonowaveDirection::Up),
            mw("2026-01-06", "2026-01-10", 120.0, 110.0, MonowaveDirection::Down),
        ];
        let triggers = build_triggers(&s, &monowaves);
        assert_eq!(triggers.len(), 2);
        match triggers[0].trigger_type {
            TriggerType::PriceBreakBelow(p) => assert!((p - 100.0).abs() < 1e-9),
            _ => panic!("expected PriceBreakBelow(100.0) for W1.start_price"),
        }
        match triggers[1].trigger_type {
            TriggerType::PriceBreakBelow(p) => assert!((p - 110.0).abs() < 1e-9),
            _ => panic!("expected PriceBreakBelow(110.0) for W2.end_price"),
        }
    }

    #[test]
    fn impulse_down_triggers_use_break_above() {
        let s = make_scenario(
            NeelyPatternType::Impulse,
            MonowaveDirection::Down,
            vec![
                ("W1".into(), "2026-01-01", "2026-01-05"),
                ("W2".into(), "2026-01-06", "2026-01-10"),
            ],
        );
        let monowaves = vec![
            mw("2026-01-01", "2026-01-05", 200.0, 180.0, MonowaveDirection::Down),
            mw("2026-01-06", "2026-01-10", 180.0, 190.0, MonowaveDirection::Up),
        ];
        let triggers = build_triggers(&s, &monowaves);
        assert!(matches!(
            triggers[0].trigger_type,
            TriggerType::PriceBreakAbove(_)
        ));
        assert!(matches!(
            triggers[1].trigger_type,
            TriggerType::PriceBreakAbove(_)
        ));
    }

    #[test]
    fn diagonal_gets_one_trigger() {
        let s = make_scenario(
            NeelyPatternType::Diagonal {
                sub_kind: DiagonalKind::Leading,
            },
            MonowaveDirection::Up,
            vec![("W1".into(), "2026-01-01", "2026-01-05")],
        );
        let monowaves = vec![mw(
            "2026-01-01",
            "2026-01-05",
            50.0,
            55.0,
            MonowaveDirection::Up,
        )];
        let triggers = build_triggers(&s, &monowaves);
        assert_eq!(triggers.len(), 1);
        match triggers[0].trigger_type {
            TriggerType::PriceBreakBelow(p) => assert!((p - 50.0).abs() < 1e-9),
            _ => panic!("Diagonal Up should produce PriceBreakBelow(W1.start_price=50.0)"),
        }
    }

    #[test]
    fn zigzag_gets_no_trigger_yet() {
        let s = make_scenario(
            NeelyPatternType::Zigzag {
                sub_kind: ZigzagKind::Single,
            },
            MonowaveDirection::Up,
            vec![("W1".into(), "2026-01-01", "2026-01-05")],
        );
        let triggers = build_triggers(&s, &[]);
        assert!(triggers.is_empty());
    }

    #[test]
    fn monowave_not_found_falls_back_to_zero_price() {
        let s = make_scenario(
            NeelyPatternType::Impulse,
            MonowaveDirection::Up,
            vec![("W1".into(), "2026-01-01", "2026-01-05")],
        );
        // monowaves 為空 → 找不到對應 W1 → 退回 0.0
        let triggers = build_triggers(&s, &[]);
        assert_eq!(triggers.len(), 1);
        match triggers[0].trigger_type {
            TriggerType::PriceBreakBelow(p) => assert!(p.abs() < 1e-9),
            _ => panic!("expected PriceBreakBelow(0.0) fallback"),
        }
    }
}
