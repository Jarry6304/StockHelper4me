// triggers — Stage 10c:Invalidation Triggers 生成
//
// 對齊 m3Spec/neely_core_architecture.md §7.1 Stage 10c + §9.4 OnTriggerAction
//       + m3Spec/neely_rules.md §Impulsion + §Overlap Rule + Ch11 Zigzag(2328-2342 行)
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
//
// **v4.5.1(2026-05-19)Group 2.1 Zigzag triggers**:
//   - Zigzag wave-b 不可完全回測 wave-a → `InvalidateScenario`
//   - Zigzag wave-c 不過 wave-a 起點(c-wave 退化)→ `WeakenScenario`
// **v4.5.2(2026-05-19)Group 2.2 Flat triggers**:
//   - Flat wave-c 不過 wave-a 起點 → `InvalidateScenario`
//   - Expanded Flat (Irregular*/Elongated) wave-b 端點突破反向 → `WeakenScenario`
// **v4.5.3(2026-05-19)Group 2.3 Triangle triggers**:
//   - Contracting / Limiting Triangle wave-e 收斂線突破 → `InvalidateScenario`
//     (Expanding Triangle 本 PR 不加 wave-e trigger,屬未來細化)
// **v4.5.4(2026-05-19)Group 2.4 Combination + RunningCorrection triggers**:
//   - Combination 末段反向破 wave-a 起點 → `InvalidateScenario`
//   - RunningCorrection 末段反向破 wave-a 起點 → `InvalidateScenario`
//   - 同時加 `CombinationAsImpulse` emulation(spec 1905-1906 一般化)
//   - Match arm 變 exhaustive(移除 `_ => {}` catch-all)

use crate::output::{
    FlatVariant, Monowave, MonowaveDirection, NeelyPatternType, OnTriggerAction, RuleId, Scenario,
    Trigger, TriangleWave, TriggerType, WaveAbc, WaveNode,
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
        NeelyPatternType::Zigzag { .. } => {
            // v4.5.1 — Ch11_Zigzag_WaveByWave wave-b:wave-b 不可完全回測 wave-a 起點
            // (對齊 spec line 2328-2332:Zigzag wave-b 典型 ≤ 81% × wave-a,
            //  超過 100% 直接破 wave-a 起點即非 Zigzag)
            if let Some(wave_a) = scenario.wave_tree.children.first() {
                let wave_a_start = find_wave_start_price(wave_a, monowaves);
                triggers.push(Trigger {
                    trigger_type: directional_break(is_up, wave_a_start),
                    on_trigger: OnTriggerAction::InvalidateScenario,
                    rule_reference: RuleId::Ch11_Zigzag_WaveByWave { wave: WaveAbc::B },
                    neely_page: "neely_rules.md §Zigzag wave-b 規則(2328-2332 行)"
                        .to_string(),
                });
            }
            // v4.5.1 — Zigzag wave-c 不過 wave-b 端點(c-wave 退化前兆)
            // (對齊 spec line 2337-2342:wave-c 反向破 wave-b 端點 → 警示)
            if scenario.wave_tree.children.len() >= 3 {
                let wave_b = &scenario.wave_tree.children[1];
                let wave_b_end = find_wave_end_price(wave_b, monowaves);
                triggers.push(Trigger {
                    trigger_type: directional_break(is_up, wave_b_end),
                    on_trigger: OnTriggerAction::WeakenScenario,
                    rule_reference: RuleId::Ch11_Zigzag_WaveByWave { wave: WaveAbc::C },
                    neely_page: "neely_rules.md §Zigzag wave-c 規則(2337-2342 行)"
                        .to_string(),
                });
            }
        }
        NeelyPatternType::Flat { sub_kind } => {
            // v4.5.2 — Ch11_Flat_Variant_Rules wave-c:wave-c 不過 wave-a 起點
            // (對齊 spec line 2208:Common Flat wave-c ≥ 100% × b 且 ≥ 38.2% × a)
            if let Some(wave_a) = scenario.wave_tree.children.first() {
                let wave_a_start = find_wave_start_price(wave_a, monowaves);
                triggers.push(Trigger {
                    trigger_type: directional_break(is_up, wave_a_start),
                    on_trigger: OnTriggerAction::InvalidateScenario,
                    rule_reference: RuleId::Ch11_Flat_Variant_Rules {
                        variant: flat_variant_from_kind(*sub_kind),
                        wave: WaveAbc::C,
                    },
                    neely_page: "neely_rules.md §Flat wave-c 規則(2208 行;wave-c 不過 wave-a 起點)"
                        .to_string(),
                });
            }
            // v4.5.2 — Expanded Flat(Irregular* / Elongated)wave-b 端點突破反向 → WeakenScenario
            if matches!(
                sub_kind,
                crate::output::FlatKind::Irregular
                    | crate::output::FlatKind::IrregularStrongB
                    | crate::output::FlatKind::Elongated
            ) {
                if let Some(wave_b) = scenario.wave_tree.children.get(1) {
                    let wave_b_end = find_wave_end_price(wave_b, monowaves);
                    triggers.push(Trigger {
                        trigger_type: directional_break(is_up, wave_b_end),
                        on_trigger: OnTriggerAction::WeakenScenario,
                        rule_reference: RuleId::Ch11_Flat_Variant_Rules {
                            variant: flat_variant_from_kind(*sub_kind),
                            wave: WaveAbc::B,
                        },
                        neely_page: "neely_rules.md §Expanded Flat wave-b 端點(2235-2240 行)"
                            .to_string(),
                    });
                }
            }
        }
        NeelyPatternType::Triangle { sub_kind } => {
            // v4.5.3 — Ch11_Triangle_Variant_Rules wave-e:
            //   - Contracting / Limiting:wave-e ≤ wave-c(spec line 2453)
            //   - Expanding:wave-e > wave-d 為定義(本 PR 暫不加 expanding wave-e trigger)
            if scenario.wave_tree.children.len() >= 5 {
                let wave_c = &scenario.wave_tree.children[2];
                let wave_c_end = find_wave_end_price(wave_c, monowaves);
                // 收斂線突破:Contracting Triangle wave-e 不過 wave-c 端點 → 突破即無效
                if matches!(
                    sub_kind,
                    crate::output::TriangleKind::Contracting
                        | crate::output::TriangleKind::Limiting
                ) {
                    triggers.push(Trigger {
                        trigger_type: directional_break(is_up, wave_c_end),
                        on_trigger: OnTriggerAction::InvalidateScenario,
                        rule_reference: RuleId::Ch11_Triangle_Variant_Rules {
                            variant: triangle_variant_default(*sub_kind),
                            wave: TriangleWave::E,
                        },
                        neely_page: "neely_rules.md §Triangle wave-e ≤ wave-c(2453 行)"
                            .to_string(),
                    });
                }
            }
        }
        NeelyPatternType::Combination { .. } => {
            // v4.5.4 — Combination 末段反向破 wave-a 起點 → InvalidateScenario
            // (Combination 整體應維持向 wave-a 方向延展,wave-x 串接但不退回起點;
            //  對齊 spec line 1862-1869 Ch8 Combination 定義)
            if let Some(wave_a) = scenario.wave_tree.children.first() {
                let wave_a_start = find_wave_start_price(wave_a, monowaves);
                triggers.push(Trigger {
                    trigger_type: directional_break(is_up, wave_a_start),
                    on_trigger: OnTriggerAction::InvalidateScenario,
                    rule_reference: RuleId::Ch8_XWave_InternalStructure,
                    neely_page:
                        "neely_rules.md §Ch8 Combination 末段不退回 wave-a 起點(1862-1869 行)"
                            .to_string(),
                });
            }
        }
        NeelyPatternType::RunningCorrection => {
            // v4.5.4 — RunningCorrection:後續 Impulse > 161.8%(spec 2024-2037);
            // 反向 invalidation 觸發點同 Combination(末段不退起點)
            if let Some(wave_a) = scenario.wave_tree.children.first() {
                let wave_a_start = find_wave_start_price(wave_a, monowaves);
                triggers.push(Trigger {
                    trigger_type: directional_break(is_up, wave_a_start),
                    on_trigger: OnTriggerAction::InvalidateScenario,
                    rule_reference: RuleId::Ch6_Correction_BLarge_Stage2,
                    neely_page: "neely_rules.md §RunningCorrection 後續 Impulse(2024-2037 行)"
                        .to_string(),
                });
            }
        }
    }

    triggers
}

/// 將 FlatKind 轉成對應 FlatVariant(Ch11 規則 RuleId 用)。
///
/// FlatKind(8 variant)→ FlatVariant(10 variant)mapping:
/// IrregularStrongB → StrongB(FlatVariant 用 StrongB 命名);其餘 1:1 對映。
fn flat_variant_from_kind(kind: crate::output::FlatKind) -> FlatVariant {
    use crate::output::FlatKind;
    match kind {
        FlatKind::Common => FlatVariant::Common,
        FlatKind::BFailure => FlatVariant::BFailure,
        FlatKind::CFailure => FlatVariant::CFailure,
        FlatKind::DoubleFailure => FlatVariant::DoubleFailure,
        FlatKind::Irregular => FlatVariant::Irregular,
        FlatKind::IrregularStrongB => FlatVariant::StrongB,
        FlatKind::IrregularFailure => FlatVariant::IrregularFailure,
        FlatKind::Elongated => FlatVariant::Elongated,
    }
}

/// 將 TriangleKind 轉成 TriangleVariant 的 default(本 PR 暫用 Horizontal*;
/// 完整 9-variant 分類已在 ch11_triangle_variants.rs 內 classify_variant,
/// 但 trigger build 路徑暫不接 monowave magnitude 算 ratio,維持 placeholder)。
fn triangle_variant_default(kind: crate::output::TriangleKind) -> crate::output::TriangleVariant {
    use crate::output::{TriangleKind, TriangleVariant};
    match kind {
        TriangleKind::Contracting => TriangleVariant::HorizontalNonLimiting,
        TriangleKind::Expanding => TriangleVariant::HorizontalExpanding,
        TriangleKind::Limiting => TriangleVariant::HorizontalLimiting,
    }
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
            bar_indices: (0, 0),
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

    // v4.5.1 Zigzag triggers tests ----------------------------------------

    #[test]
    fn zigzag_up_gets_wave_b_break_below_wave_a_start() {
        let s = make_scenario(
            NeelyPatternType::Zigzag {
                sub_kind: ZigzagKind::Single,
            },
            MonowaveDirection::Up,
            vec![
                ("a".into(), "2026-01-01", "2026-01-05"),
                ("b".into(), "2026-01-06", "2026-01-10"),
                ("c".into(), "2026-01-11", "2026-01-15"),
            ],
        );
        let monowaves = vec![
            mw("2026-01-01", "2026-01-05", 100.0, 110.0, MonowaveDirection::Up),
            mw("2026-01-06", "2026-01-10", 110.0, 104.0, MonowaveDirection::Down),
            mw("2026-01-11", "2026-01-15", 104.0, 115.0, MonowaveDirection::Up),
        ];
        let triggers = build_triggers(&s, &monowaves);
        assert_eq!(triggers.len(), 2);
        // wave-b InvalidateScenario @ wave-a.start = 100.0
        match triggers[0].trigger_type {
            TriggerType::PriceBreakBelow(p) => assert!((p - 100.0).abs() < 1e-9),
            _ => panic!("expected PriceBreakBelow(100.0) for Zigzag wave-b break"),
        }
        assert!(matches!(
            triggers[0].on_trigger,
            OnTriggerAction::InvalidateScenario
        ));
        // wave-c WeakenScenario @ wave-b.end = 104.0
        match triggers[1].trigger_type {
            TriggerType::PriceBreakBelow(p) => assert!((p - 104.0).abs() < 1e-9),
            _ => panic!("expected PriceBreakBelow(104.0) for Zigzag wave-c weaken"),
        }
        assert!(matches!(
            triggers[1].on_trigger,
            OnTriggerAction::WeakenScenario
        ));
    }

    #[test]
    fn zigzag_down_gets_wave_b_break_above() {
        let s = make_scenario(
            NeelyPatternType::Zigzag {
                sub_kind: ZigzagKind::Single,
            },
            MonowaveDirection::Down,
            vec![
                ("a".into(), "2026-01-01", "2026-01-05"),
                ("b".into(), "2026-01-06", "2026-01-10"),
                ("c".into(), "2026-01-11", "2026-01-15"),
            ],
        );
        let monowaves = vec![
            mw("2026-01-01", "2026-01-05", 200.0, 180.0, MonowaveDirection::Down),
            mw("2026-01-06", "2026-01-10", 180.0, 195.0, MonowaveDirection::Up),
            mw("2026-01-11", "2026-01-15", 195.0, 170.0, MonowaveDirection::Down),
        ];
        let triggers = build_triggers(&s, &monowaves);
        assert_eq!(triggers.len(), 2);
        match triggers[0].trigger_type {
            TriggerType::PriceBreakAbove(p) => assert!((p - 200.0).abs() < 1e-9),
            _ => panic!("expected PriceBreakAbove(200.0) for Down Zigzag wave-b"),
        }
    }

    #[test]
    fn zigzag_short_children_only_emits_wave_b_trigger() {
        // 只有 wave-a 不足 3 children → 只生 wave-b trigger,不生 wave-c
        let s = make_scenario(
            NeelyPatternType::Zigzag {
                sub_kind: ZigzagKind::Single,
            },
            MonowaveDirection::Up,
            vec![("a".into(), "2026-01-01", "2026-01-05")],
        );
        let monowaves = vec![mw(
            "2026-01-01", "2026-01-05", 100.0, 110.0, MonowaveDirection::Up,
        )];
        let triggers = build_triggers(&s, &monowaves);
        assert_eq!(triggers.len(), 1);
    }

    // v4.5.2 Flat triggers tests ------------------------------------------

    #[test]
    fn flat_common_up_gets_wave_c_break_below_wave_a_start() {
        let s = make_scenario(
            NeelyPatternType::Flat {
                sub_kind: FlatKind::Common,
            },
            MonowaveDirection::Up,
            vec![
                ("a".into(), "2026-01-01", "2026-01-05"),
                ("b".into(), "2026-01-06", "2026-01-10"),
                ("c".into(), "2026-01-11", "2026-01-15"),
            ],
        );
        let monowaves = vec![
            mw("2026-01-01", "2026-01-05", 100.0, 110.0, MonowaveDirection::Up),
            mw("2026-01-06", "2026-01-10", 110.0, 101.0, MonowaveDirection::Down),
            mw("2026-01-11", "2026-01-15", 101.0, 112.0, MonowaveDirection::Up),
        ];
        let triggers = build_triggers(&s, &monowaves);
        // Common Flat 只生 wave-c (Invalidate) trigger,不生 Expanded wave-b WeakenTrigger
        assert_eq!(triggers.len(), 1);
        match triggers[0].trigger_type {
            TriggerType::PriceBreakBelow(p) => assert!((p - 100.0).abs() < 1e-9),
            _ => panic!("expected PriceBreakBelow(100.0) for Flat wave-c"),
        }
    }

    #[test]
    fn flat_irregular_up_gets_wave_b_weaken_plus_wave_c_invalidate() {
        let s = make_scenario(
            NeelyPatternType::Flat {
                sub_kind: FlatKind::Irregular,
            },
            MonowaveDirection::Up,
            vec![
                ("a".into(), "2026-01-01", "2026-01-05"),
                ("b".into(), "2026-01-06", "2026-01-10"),
                ("c".into(), "2026-01-11", "2026-01-15"),
            ],
        );
        let monowaves = vec![
            mw("2026-01-01", "2026-01-05", 100.0, 110.0, MonowaveDirection::Up),
            mw("2026-01-06", "2026-01-10", 110.0, 98.0, MonowaveDirection::Down),
            mw("2026-01-11", "2026-01-15", 98.0, 115.0, MonowaveDirection::Up),
        ];
        let triggers = build_triggers(&s, &monowaves);
        assert_eq!(triggers.len(), 2);
        // 第二個是 wave-b WeakenScenario @ wave-b.end = 98.0
        assert!(matches!(
            triggers[1].on_trigger,
            OnTriggerAction::WeakenScenario
        ));
    }

    // v4.5.3 Triangle triggers tests --------------------------------------

    #[test]
    fn contracting_triangle_up_gets_wave_e_break_below_wave_c_end() {
        let s = make_scenario(
            NeelyPatternType::Triangle {
                sub_kind: TriangleKind::Contracting,
            },
            MonowaveDirection::Up,
            vec![
                ("a".into(), "2026-01-01", "2026-01-05"),
                ("b".into(), "2026-01-06", "2026-01-10"),
                ("c".into(), "2026-01-11", "2026-01-15"),
                ("d".into(), "2026-01-16", "2026-01-20"),
                ("e".into(), "2026-01-21", "2026-01-25"),
            ],
        );
        let monowaves = vec![
            mw("2026-01-01", "2026-01-05", 100.0, 110.0, MonowaveDirection::Up),
            mw("2026-01-06", "2026-01-10", 110.0, 103.0, MonowaveDirection::Down),
            mw("2026-01-11", "2026-01-15", 103.0, 108.0, MonowaveDirection::Up),
            mw("2026-01-16", "2026-01-20", 108.0, 105.0, MonowaveDirection::Down),
            mw("2026-01-21", "2026-01-25", 105.0, 107.0, MonowaveDirection::Up),
        ];
        let triggers = build_triggers(&s, &monowaves);
        assert_eq!(triggers.len(), 1);
        match triggers[0].trigger_type {
            TriggerType::PriceBreakBelow(p) => assert!((p - 108.0).abs() < 1e-9),
            _ => panic!("expected PriceBreakBelow(108.0) for Contracting Triangle wave-e"),
        }
        assert!(matches!(
            triggers[0].on_trigger,
            OnTriggerAction::InvalidateScenario
        ));
    }

    #[test]
    fn expanding_triangle_does_not_emit_wave_e_trigger() {
        let s = make_scenario(
            NeelyPatternType::Triangle {
                sub_kind: TriangleKind::Expanding,
            },
            MonowaveDirection::Up,
            vec![
                ("a".into(), "2026-01-01", "2026-01-05"),
                ("b".into(), "2026-01-06", "2026-01-10"),
                ("c".into(), "2026-01-11", "2026-01-15"),
                ("d".into(), "2026-01-16", "2026-01-20"),
                ("e".into(), "2026-01-21", "2026-01-25"),
            ],
        );
        let monowaves: Vec<Monowave> = (0..5)
            .map(|i| {
                mw(
                    &format!("2026-01-{:02}", 1 + i * 5),
                    &format!("2026-01-{:02}", 5 + i * 5),
                    100.0 + i as f64,
                    105.0 + i as f64,
                    MonowaveDirection::Up,
                )
            })
            .collect();
        let triggers = build_triggers(&s, &monowaves);
        // Expanding Triangle 本 PR 暫不加 wave-e 突破 trigger
        assert!(triggers.is_empty());
    }

    // v4.5.4 Combination + RunningCorrection triggers tests ---------------

    #[test]
    fn combination_up_gets_wave_a_invalidate_trigger() {
        let s = make_scenario(
            NeelyPatternType::Combination {
                sub_kinds: vec![CombinationKind::DoubleZigzag],
            },
            MonowaveDirection::Up,
            vec![("a".into(), "2026-01-01", "2026-01-05")],
        );
        let monowaves = vec![mw(
            "2026-01-01", "2026-01-05", 50.0, 60.0, MonowaveDirection::Up,
        )];
        let triggers = build_triggers(&s, &monowaves);
        assert_eq!(triggers.len(), 1);
        assert!(matches!(
            triggers[0].on_trigger,
            OnTriggerAction::InvalidateScenario
        ));
        match triggers[0].trigger_type {
            TriggerType::PriceBreakBelow(p) => assert!((p - 50.0).abs() < 1e-9),
            _ => panic!("expected PriceBreakBelow(50.0) for Combination wave-a"),
        }
    }

    #[test]
    fn running_correction_down_gets_wave_a_invalidate_trigger() {
        let s = make_scenario(
            NeelyPatternType::RunningCorrection,
            MonowaveDirection::Down,
            vec![("a".into(), "2026-01-01", "2026-01-05")],
        );
        let monowaves = vec![mw(
            "2026-01-01", "2026-01-05", 200.0, 180.0, MonowaveDirection::Down,
        )];
        let triggers = build_triggers(&s, &monowaves);
        assert_eq!(triggers.len(), 1);
        match triggers[0].trigger_type {
            TriggerType::PriceBreakAbove(p) => assert!((p - 200.0).abs() < 1e-9),
            _ => panic!("expected PriceBreakAbove(200.0) for Down RunningCorrection"),
        }
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
