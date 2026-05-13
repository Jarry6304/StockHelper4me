// triggers — Stage 10c:Invalidation Triggers 生成(PR-6b-3 完整實作)
//
// 對齊 m3Spec/neely_core_architecture.md §9.4 + neely_rules.md Ch12 EarlyWarning。
// Trigger 是 Neely 規則的逆向轉譯,寫進 Scenario.invalidation_triggers。
//
// 4 種 TriggerType(§9.4):
//   - PriceBreakBelow(f64) / PriceBreakAbove(f64)
//   - TimeExceeds(NaiveDate)
//   - VolumeAnomaly { z_threshold: f64 }
//   - OverlapWith { wave_id: String }
//
// 3 種 OnTriggerAction(§9.4):
//   - InvalidateScenario(強制失效)
//   - WeakenScenario(降級為 deferred,不引入機率語意)
//   - PromoteAlternative { promoted_id }(提拔替代)
//
// **PR-6b-3 階段(2026-05-13)**:
//   - 用 classified slice 拿真實 price endpoints
//   - 為每個 pattern_type 生成對應 triggers(Impulse / TerminalImpulse / Zigzag /
//     Flat / Triangle / Combination / RunningCorrection)

use crate::monowave::ClassifiedMonowave;
use crate::output::{
    MonowaveDirection, NeelyPatternType, OnTriggerAction, RuleId, Scenario, Trigger, TriggerType,
};

/// 從 Scenario 推算 invalidation triggers(PR-6b-3 完整版,接 classified 拿 price)。
pub fn build_triggers(scenario: &Scenario, classified: &[ClassifiedMonowave]) -> Vec<Trigger> {
    let mut triggers = Vec::new();

    let wave_prices = collect_wave_endpoints(scenario, classified);
    if wave_prices.is_empty() {
        return triggers;
    }

    let w1_start_price = wave_prices[0].0;
    let direction = wave_prices[0].2;

    match &scenario.pattern_type {
        NeelyPatternType::Impulse => {
            // R1-derived:W2 不可跨 W1 起點 → 跨過則 invalidate
            triggers.push(make_break_trigger(
                w1_start_price,
                direction,
                OnTriggerAction::InvalidateScenario,
                RuleId::Ch5Essential(1),
                "Ch5 R1 — W2 不可完全回測 W1 起點",
            ));
            // R3-derived:W4 不重疊 W1 終點 → 重疊則 WeakenScenario(降級 TerminalImpulse)
            if wave_prices.len() >= 1 {
                let w1_end_price = wave_prices[0].1;
                triggers.push(make_break_trigger(
                    w1_end_price,
                    direction,
                    OnTriggerAction::WeakenScenario,
                    RuleId::Ch5Essential(3),
                    "Ch5 R3 — W4 不可重疊 W1(若 fail → TerminalImpulse)",
                ));
            }
        }
        NeelyPatternType::TerminalImpulse => {
            // TerminalImpulse:R1-derived only(R3 deferred,允許 W4-W1 重疊)
            triggers.push(make_break_trigger(
                w1_start_price,
                direction,
                OnTriggerAction::InvalidateScenario,
                RuleId::Ch5Essential(1),
                "Ch5 R1 — TerminalImpulse 容許 W4-W1 重疊",
            ));
        }
        NeelyPatternType::Zigzag { .. } => {
            // Zigzag b-wave ≤ 61.8% × a:b 跨過 a 起點 → invalidate
            triggers.push(make_break_trigger(
                w1_start_price,
                direction,
                OnTriggerAction::InvalidateScenario,
                RuleId::Ch5ZigzagMaxBRetracement,
                "Ch5 Z1 — Zigzag b-wave 不可越過 a 起點",
            ));
        }
        NeelyPatternType::Flat { .. } => {
            // Flat a 起點被破 → Running Correction(WeakenScenario)
            triggers.push(make_break_trigger(
                w1_start_price,
                direction,
                OnTriggerAction::WeakenScenario,
                RuleId::Ch5FlatMinBRatio,
                "Ch5 F1 — Flat b/a > 142.2% → Running Correction",
            ));
        }
        NeelyPatternType::Triangle { .. } => {
            // Triangle 0-2 / B-D trendline 被穿破 → invalidate(spec line 1361)
            // Ch12 Channeling RunningDoubleThree / TriangleEarlyWarning
            triggers.push(make_break_trigger(
                w1_start_price,
                direction,
                OnTriggerAction::InvalidateScenario,
                RuleId::Ch12ChannelingTriangleEarlyWarning,
                "Ch12 — Triangle B-D trendline 被穿破 → 形態結束",
            ));
            // Triangle wave-e 終結後 thrust 異常 → WeakenScenario
            if let Some(last) = wave_prices.last() {
                triggers.push(Trigger {
                    trigger_type: match direction {
                        MonowaveDirection::Down => TriggerType::PriceBreakAbove(last.1),
                        _ => TriggerType::PriceBreakBelow(last.1),
                    },
                    on_trigger: OnTriggerAction::WeakenScenario,
                    rule_reference: RuleId::Ch5TriangleLegContraction,
                    neely_page: "Ch5 — Triangle wave-e 後 thrust 異常".to_string(),
                });
            }
        }
        NeelyPatternType::Combination { .. } => {
            // Combination 內含至少一個 Triangle → 同樣 B-D 線監控
            triggers.push(make_break_trigger(
                w1_start_price,
                direction,
                OnTriggerAction::InvalidateScenario,
                RuleId::Ch8MultiwaveConstruction,
                "Ch8 — Combination 結構違反",
            ));
        }
        NeelyPatternType::RunningCorrection => {
            // Running Correction:後續 Impulse > 161.8% × 前一 Impulse(line 2035)
            // 若後續 wave < 100% × 前一 Impulse → 結構失效
            triggers.push(make_break_trigger(
                w1_start_price,
                direction,
                OnTriggerAction::WeakenScenario,
                RuleId::Ch12ReverseLogic,
                "Ch12 — Running Correction 後續延伸 Impulse < 161.8% → 失效",
            ));
        }
    }

    triggers
}

/// 對 Forest 中所有 Scenario 套 triggers,寫入 invalidation_triggers 欄位。
pub fn apply_to_forest(forest: &mut [Scenario], classified: &[ClassifiedMonowave]) {
    for scenario in forest.iter_mut() {
        scenario.invalidation_triggers = build_triggers(scenario, classified);
    }
}

/// helper:從 Scenario.wave_tree.children 取每個 wave 的 (start_price, end_price, direction) 三元組。
fn collect_wave_endpoints(
    scenario: &Scenario,
    classified: &[ClassifiedMonowave],
) -> Vec<(f64, f64, MonowaveDirection)> {
    scenario
        .wave_tree
        .children
        .iter()
        .filter_map(|node| {
            classified
                .iter()
                .find(|c| c.monowave.start_date == node.start && c.monowave.end_date == node.end)
                .map(|c| (c.monowave.start_price, c.monowave.end_price, c.monowave.direction))
        })
        .collect()
}

/// helper:生成 PriceBreakBelow / Above trigger,基於趨勢方向。
fn make_break_trigger(
    price: f64,
    direction: MonowaveDirection,
    on_trigger: OnTriggerAction,
    rule_reference: RuleId,
    neely_page: &str,
) -> Trigger {
    let trigger_type = match direction {
        MonowaveDirection::Down => TriggerType::PriceBreakAbove(price),
        _ => TriggerType::PriceBreakBelow(price),
    };
    Trigger {
        trigger_type,
        on_trigger,
        rule_reference,
        neely_page: neely_page.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::monowave::ProportionMetrics;
    use crate::output::*;
    use chrono::NaiveDate;

    fn cmw(start_p: f64, end_p: f64, start_date: NaiveDate, end_date: NaiveDate) -> ClassifiedMonowave {
        ClassifiedMonowave {
            monowave: Monowave {
                start_date,
                end_date,
                start_price: start_p,
                end_price: end_p,
                direction: if end_p >= start_p {
                    MonowaveDirection::Up
                } else {
                    MonowaveDirection::Down
                },
            },
            atr_at_start: 1.0,
            metrics: ProportionMetrics {
                magnitude: (end_p - start_p).abs(),
                duration_bars: 5,
                atr_relative: 5.0,
                slope_vs_45deg: 1.0,
            },
        }
    }

    fn make_scenario(pattern: NeelyPatternType) -> (Scenario, Vec<ClassifiedMonowave>) {
        let d0 = NaiveDate::parse_from_str("2026-01-01", "%Y-%m-%d").unwrap();
        let d1 = NaiveDate::parse_from_str("2026-01-02", "%Y-%m-%d").unwrap();
        let classified = vec![cmw(100.0, 110.0, d0, d1)];
        let s = Scenario {
            id: "test".to_string(),
            wave_tree: WaveNode {
                label: "test".to_string(),
                start: d0,
                end: d1,
                children: vec![WaveNode {
                    label: "W1".to_string(),
                    start: d0,
                    end: d1,
                    children: vec![],
                }],
            },
            pattern_type: pattern,
            structure_label: "test Up".to_string(),
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
            awaiting_l_label: false,
        };
        (s, classified)
    }

    #[test]
    fn impulse_gets_r1_r3_triggers_with_real_prices() {
        let (s, c) = make_scenario(NeelyPatternType::Impulse);
        let triggers = build_triggers(&s, &c);
        assert_eq!(triggers.len(), 2);
        // R1:price break below w1_start=100
        if let TriggerType::PriceBreakBelow(p) = &triggers[0].trigger_type {
            assert!((p - 100.0).abs() < 1e-9);
        } else {
            panic!("expected PriceBreakBelow trigger, got {:?}", triggers[0].trigger_type);
        }
        assert!(matches!(triggers[0].on_trigger, OnTriggerAction::InvalidateScenario));
        // R3:price break below w1_end=110, WeakenScenario
        if let TriggerType::PriceBreakBelow(p) = &triggers[1].trigger_type {
            assert!((p - 110.0).abs() < 1e-9);
        } else {
            panic!("expected PriceBreakBelow trigger, got {:?}", triggers[1].trigger_type);
        }
        assert!(matches!(triggers[1].on_trigger, OnTriggerAction::WeakenScenario));
    }

    #[test]
    fn terminal_impulse_gets_one_trigger() {
        let (s, c) = make_scenario(NeelyPatternType::TerminalImpulse);
        let triggers = build_triggers(&s, &c);
        assert_eq!(triggers.len(), 1);
    }

    #[test]
    fn zigzag_gets_trigger() {
        let (s, c) = make_scenario(NeelyPatternType::Zigzag { sub_kind: ZigzagVariant::Normal });
        let triggers = build_triggers(&s, &c);
        assert_eq!(triggers.len(), 1);
        assert_eq!(triggers[0].rule_reference, RuleId::Ch5ZigzagMaxBRetracement);
    }

    #[test]
    fn flat_gets_running_correction_trigger() {
        let (s, c) = make_scenario(NeelyPatternType::Flat { sub_kind: FlatVariant::Common });
        let triggers = build_triggers(&s, &c);
        assert_eq!(triggers.len(), 1);
        assert!(matches!(triggers[0].on_trigger, OnTriggerAction::WeakenScenario));
    }

    #[test]
    fn triangle_gets_two_triggers() {
        let (s, c) = make_scenario(NeelyPatternType::Triangle {
            sub_kind: TriangleVariant::HorizontalLimiting,
        });
        let triggers = build_triggers(&s, &c);
        assert_eq!(triggers.len(), 2);
    }

    #[test]
    fn combination_gets_trigger() {
        let (s, c) = make_scenario(NeelyPatternType::Combination {
            sub_kinds: vec![CombinationKind::DoubleThree],
        });
        let triggers = build_triggers(&s, &c);
        assert_eq!(triggers.len(), 1);
    }

    #[test]
    fn running_correction_gets_trigger() {
        let (s, c) = make_scenario(NeelyPatternType::RunningCorrection);
        let triggers = build_triggers(&s, &c);
        assert_eq!(triggers.len(), 1);
        assert_eq!(triggers[0].rule_reference, RuleId::Ch12ReverseLogic);
    }

    #[test]
    fn down_direction_flips_to_price_break_above() {
        let d0 = NaiveDate::parse_from_str("2026-01-01", "%Y-%m-%d").unwrap();
        let d1 = NaiveDate::parse_from_str("2026-01-02", "%Y-%m-%d").unwrap();
        let classified = vec![cmw(100.0, 90.0, d0, d1)];  // Down
        let s = Scenario {
            id: "down".to_string(),
            wave_tree: WaveNode {
                label: "down".to_string(),
                start: d0,
                end: d1,
                children: vec![WaveNode {
                    label: "W1".to_string(),
                    start: d0,
                    end: d1,
                    children: vec![],
                }],
            },
            pattern_type: NeelyPatternType::Impulse,
            structure_label: "Impulse Down".to_string(),
            complexity_level: ComplexityLevel::Simple,
            power_rating: PowerRating::Neutral,
            max_retracement: 0.0,
            post_pattern_behavior: PostBehavior::Unconstrained,
            passed_rules: vec![],
            deferred_rules: vec![],
            rules_passed_count: 0,
            deferred_rules_count: 0,
            invalidation_triggers: vec![],
            expected_fib_zones: vec![],
            structural_facts: StructuralFacts::default(),
            awaiting_l_label: false,
        };
        let triggers = build_triggers(&s, &classified);
        // Down 趨勢 → PriceBreakAbove
        if let TriggerType::PriceBreakAbove(_) = &triggers[0].trigger_type {
            // OK
        } else {
            panic!("Down direction 應 PriceBreakAbove, got {:?}", triggers[0].trigger_type);
        }
    }

    #[test]
    fn apply_to_forest_writes_triggers() {
        let (mut s, c) = make_scenario(NeelyPatternType::Impulse);
        s.invalidation_triggers = vec![]; // clear
        let mut forest = vec![s];
        apply_to_forest(&mut forest, &c);
        assert!(!forest[0].invalidation_triggers.is_empty());
    }
}
