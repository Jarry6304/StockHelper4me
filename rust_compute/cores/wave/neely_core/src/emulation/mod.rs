// emulation — Stage 9b:Emulation 偵測(Ch12 Emulation)
//
// 對齊 m3Spec/neely_rules.md §Ch8 Non-Standard Polywaves(1902-1906 行 Running 變體辨識要點)
//       + §Ch12 Emulation + Ch11 Zigzag wave-c 規則(2337-2342 行)
//       + m3Spec/neely_core_architecture.md §7.1 Stage 9b + §9.3 Ch12_Emulation
//
// **Phase 9 PR**(原始 4 種 Ch12 Emulation 實作):
//   對 forest 中每個 scenario 套用視覺/結構 emulation 偵測。Emulation 是「視覺上
//   相似於 X 但結構規則屬 Y」的場景。Phase 9 偵測四種主要 emulation kind:
//   1. RunningDoubleThreeAsImpulse(spec 1905-1906)
//   2. DiagonalAsImpulse
//   3. TriangleAsFailure
//   4. FirstExtAsTerminal
//
// **v4.5.1(2026-05-19)Group 2.1 補完 Zigzag emulation**:
//   5. ZigzagAsFlatFailure(spec 2337-2342):Zigzag wave-c < 100% × wave-a → 似 Flat C-Failure
//
// **v4.5.2(2026-05-19)Group 2.2 補完 Flat emulation**:
//   6. FlatAsZigzag(spec 2191-2321 Elongated):Flat wave-c > 138.2% × wave-a → 似 Zigzag
//
// **v4.5.4(2026-05-19)Group 2.4 補完 Combination emulation**:
//   7. CombinationAsImpulse(spec 1905-1906 一般化):DoubleThree*/TripleThree* + 5/7 children → 似 Impulse
//   Match arm 變 exhaustive(移除 `_ => {}` catch-all)

use crate::monowave::ClassifiedMonowave;
use crate::output::{
    Certainty, EmulationKind, EmulationSuspect, NeelyPatternType, Scenario, StructureLabel,
};

/// Stage 9b 主入口:對每個 scenario 偵測 emulation 場景。
///
/// 每個 scenario 可能產出 0-N 個 EmulationSuspect(諮詢性,不過濾 scenarios)。
pub fn detect_all(
    forest: &[Scenario],
    classified: &[ClassifiedMonowave],
) -> Vec<EmulationSuspect> {
    let mut suspects = Vec::new();
    for scenario in forest {
        suspects.extend(detect_for_scenario(scenario, classified));
    }
    suspects
}

/// 對單一 scenario 偵測各 EmulationKind。
fn detect_for_scenario(
    scenario: &Scenario,
    classified: &[ClassifiedMonowave],
) -> Vec<EmulationSuspect> {
    let mut suspects = Vec::new();

    match &scenario.pattern_type {
        NeelyPatternType::Impulse => {
            // 偵測 1:Running Double Three Combination 偽裝
            if let Some(s) = check_running_double_three_as_impulse(scenario, classified) {
                suspects.push(s);
            }
            // 偵測 4:1st Ext 偽裝 Terminal(advisory 含 Overlap 接近邊界訊號)
            if let Some(s) = check_first_ext_as_terminal(scenario) {
                suspects.push(s);
            }
        }
        NeelyPatternType::Diagonal { .. } => {
            // 偵測 2:Diagonal 偽裝 Impulse(已分類為 Diagonal 即標)
            suspects.push(EmulationSuspect {
                scenario_id: Some(scenario.id.clone()),
                kind: EmulationKind::DiagonalAsImpulse,
                message: format!(
                    "Scenario[{}] 已分類為 Diagonal (Terminal Impulse) — 視覺上常被誤認為 Trending Impulse",
                    scenario.id
                ),
            });
        }
        NeelyPatternType::Triangle { .. } => {
            // 偵測 3:Triangle 偽裝 5-wave Failure
            if let Some(s) = check_triangle_as_failure(scenario, classified) {
                suspects.push(s);
            }
        }
        // v4.5.1 — Zigzag 偽裝 Flat C-Failure
        NeelyPatternType::Zigzag { .. } => {
            if let Some(s) = check_zigzag_as_flat_failure(scenario, classified) {
                suspects.push(s);
            }
        }
        // v4.5.2 — Flat 偽裝 Zigzag(Elongated Flat 之 wave-c 過長)
        NeelyPatternType::Flat { .. } => {
            if let Some(s) = check_flat_as_zigzag(scenario, classified) {
                suspects.push(s);
            }
        }
        // v4.5.4 — Combination 偽裝 Trending Impulse
        NeelyPatternType::Combination { .. } => {
            if let Some(s) = check_combination_as_impulse(scenario, classified) {
                suspects.push(s);
            }
        }
        // RunningCorrection:暫不偵測 emulation(spec 對 RunningCorrection 視覺辨識度高)
        NeelyPatternType::RunningCorrection => {}
    }

    suspects
}

/// 偵測 1:Running Double Three Combination 偽裝 1st Ext Impulse(spec 1905-1906)。
///
/// 辨識關鍵:
///   - scenario.pattern_type == Impulse
///   - W3 monowave 的 structure_label_candidates 含 :3 系列(corrective)
///   - 真 Impulse 之 W3 必為 :5(impulsive)
fn check_running_double_three_as_impulse(
    scenario: &Scenario,
    classified: &[ClassifiedMonowave],
) -> Option<EmulationSuspect> {
    // 從 wave_tree 找 W3 對應的 monowave
    if scenario.wave_tree.children.len() < 5 {
        return None;
    }
    let w3_node = &scenario.wave_tree.children[2];
    let w3_mw = classified
        .iter()
        .find(|c| c.monowave.start_date == w3_node.start && c.monowave.end_date == w3_node.end)?;

    // 檢查 W3 的 structure_label_candidates 是否含 :3 系列 (Primary cert)
    let has_corrective_primary = w3_mw.structure_label_candidates.iter().any(|c| {
        matches!(c.certainty, Certainty::Primary)
            && matches!(
                c.label,
                StructureLabel::Three
                    | StructureLabel::F3
                    | StructureLabel::C3
                    | StructureLabel::L3
                    | StructureLabel::SL3
            )
    });

    if has_corrective_primary {
        Some(EmulationSuspect {
            scenario_id: Some(scenario.id.clone()),
            kind: EmulationKind::RunningDoubleThreeAsImpulse,
            message: format!(
                "Scenario[{}] Impulse 但 W3 monowave 含 :3 corrective Primary 標記 — \
                 可能為 Running Double Three Combination 偽裝(spec 1905-1906)",
                scenario.id
            ),
        })
    } else {
        None
    }
}

/// 偵測 3:Triangle 偽裝 5-wave Failure。
///
/// 辨識(Phase 9 簡化):Triangle scenario 的 wave-e magnitude < 0.382 × wave-a。
/// 真 5-wave Failure 的 W5 應 ≥ 38.2% × W4,而非 W4 之 1/3。
fn check_triangle_as_failure(
    scenario: &Scenario,
    classified: &[ClassifiedMonowave],
) -> Option<EmulationSuspect> {
    if scenario.wave_tree.children.len() < 5 {
        return None;
    }
    let wave_a_node = &scenario.wave_tree.children[0];
    let wave_e_node = &scenario.wave_tree.children[4];

    let wave_a_mw = classified
        .iter()
        .find(|c| c.monowave.start_date == wave_a_node.start)?;
    let wave_e_mw = classified
        .iter()
        .find(|c| c.monowave.end_date == wave_e_node.end)?;

    let mag_a = wave_a_mw.metrics.magnitude;
    let mag_e = wave_e_mw.metrics.magnitude;

    if mag_a > 1e-9 && mag_e / mag_a < 0.382 {
        Some(EmulationSuspect {
            scenario_id: Some(scenario.id.clone()),
            kind: EmulationKind::TriangleAsFailure,
            message: format!(
                "Scenario[{}] Triangle 但 wave-e/wave-a 比 = {:.3} < 0.382 — \
                 視覺上可能被誤判為 5-wave Failure(wave-e 過短)",
                scenario.id,
                mag_e / mag_a
            ),
        })
    } else {
        None
    }
}

/// 偵測 4:1st Wave Extension Impulse 偽裝 Terminal Impulse。
///
/// 辨識(Phase 9 簡化):scenario 為 Impulse,且 advisory_findings 含
/// Ch5_Overlap_Terminal 接近邊界訊號(Phase 7 channeling 模組生成)。
fn check_first_ext_as_terminal(scenario: &Scenario) -> Option<EmulationSuspect> {
    use crate::output::RuleId;
    let has_terminal_advisory = scenario.advisory_findings.iter().any(|f| {
        matches!(f.rule_id, RuleId::Ch5_Channeling_24)
            && f.message.contains("Terminal")
    });
    if has_terminal_advisory {
        Some(EmulationSuspect {
            scenario_id: Some(scenario.id.clone()),
            kind: EmulationKind::FirstExtAsTerminal,
            message: format!(
                "Scenario[{}] Impulse 但 2-4 線早突破(Ch5_Channeling_24 advisory)— \
                 可能被誤判為 Terminal Impulse",
                scenario.id
            ),
        })
    } else {
        None
    }
}

/// v4.5.1 — 偵測 5:Zigzag 偽裝 Flat C-Failure(spec 2337-2342)。
///
/// 辨識(典型 Zigzag wave-c ∈ [61.8%, 161.8%] × wave-a):
///   - scenario.pattern_type == Zigzag
///   - wave-a 與 wave-c 都能找到對應 monowave
///   - wave-c.magnitude / wave-a.magnitude < 1.0(truncated wave-c)
///   - 視覺上似 Flat C-Failure(c 未過 wave-a 端點)
fn check_zigzag_as_flat_failure(
    scenario: &Scenario,
    classified: &[ClassifiedMonowave],
) -> Option<EmulationSuspect> {
    if scenario.wave_tree.children.len() < 3 {
        return None;
    }
    let wave_a_node = &scenario.wave_tree.children[0];
    let wave_c_node = &scenario.wave_tree.children[2];

    let wave_a_mw = classified
        .iter()
        .find(|c| c.monowave.start_date == wave_a_node.start)?;
    let wave_c_mw = classified
        .iter()
        .find(|c| c.monowave.end_date == wave_c_node.end)?;

    let mag_a = wave_a_mw.metrics.magnitude;
    let mag_c = wave_c_mw.metrics.magnitude;

    if mag_a > 1e-9 && mag_c / mag_a < 1.0 {
        Some(EmulationSuspect {
            scenario_id: Some(scenario.id.clone()),
            kind: EmulationKind::ZigzagAsFlatFailure,
            message: format!(
                "Scenario[{}] Zigzag 但 wave-c/wave-a 比 = {:.3} < 1.0 — \
                 視覺上可能被誤判為 Flat C-Failure(wave-c 未過 wave-a 端點;spec 2337-2342)",
                scenario.id,
                mag_c / mag_a
            ),
        })
    } else {
        None
    }
}

/// v4.5.2 — 偵測 6:Flat 偽裝 Zigzag(spec 2191-2321 Elongated Flat)。
///
/// 辨識:
///   - scenario.pattern_type == Flat
///   - wave-c.magnitude / wave-a.magnitude ≥ 1.382(Elongated Flat 區段)
///   - 視覺上 wave-c 顯著延伸超過 wave-a 端點,似 Zigzag
fn check_flat_as_zigzag(
    scenario: &Scenario,
    classified: &[ClassifiedMonowave],
) -> Option<EmulationSuspect> {
    if scenario.wave_tree.children.len() < 3 {
        return None;
    }
    let wave_a_node = &scenario.wave_tree.children[0];
    let wave_c_node = &scenario.wave_tree.children[2];

    let wave_a_mw = classified
        .iter()
        .find(|c| c.monowave.start_date == wave_a_node.start)?;
    let wave_c_mw = classified
        .iter()
        .find(|c| c.monowave.end_date == wave_c_node.end)?;

    let mag_a = wave_a_mw.metrics.magnitude;
    let mag_c = wave_c_mw.metrics.magnitude;

    if mag_a > 1e-9 && mag_c / mag_a >= 1.382 {
        Some(EmulationSuspect {
            scenario_id: Some(scenario.id.clone()),
            kind: EmulationKind::FlatAsZigzag,
            message: format!(
                "Scenario[{}] Flat 但 wave-c/wave-a 比 = {:.3} ≥ 1.382(Elongated)— \
                 視覺上可能被誤判為 Zigzag(spec 2191-2321 Elongated Flat 區段)",
                scenario.id,
                mag_c / mag_a
            ),
        })
    } else {
        None
    }
}

/// v4.5.4 — 偵測 7:Combination 偽裝 Trending Impulse(spec 1905-1906 一般化)。
///
/// 辨識:
///   - scenario.pattern_type == Combination(任 DoubleThree* / TripleThree* sub_kind)
///   - wave_tree.children.len() 等於 5 或 7(對齊 5-wave 表象;wave-x 串接 5 或 7 段)
///   - 與 RunningDoubleThreeAsImpulse 區別:本偵測不限 W3 含 :3 標記,
///     而是純結構(Combination + 表象 5 段)的偽裝警示
fn check_combination_as_impulse(
    scenario: &Scenario,
    _classified: &[ClassifiedMonowave],
) -> Option<EmulationSuspect> {
    let child_count = scenario.wave_tree.children.len();
    let is_running_combo = match &scenario.pattern_type {
        NeelyPatternType::Combination { sub_kinds } => sub_kinds.iter().any(|k| {
            matches!(
                k,
                crate::output::CombinationKind::DoubleThree
                    | crate::output::CombinationKind::DoubleThreeCombination
                    | crate::output::CombinationKind::DoubleThreeRunning
                    | crate::output::CombinationKind::TripleThree
                    | crate::output::CombinationKind::TripleThreeCombination
                    | crate::output::CombinationKind::TripleThreeRunning
            )
        }),
        _ => false,
    };
    if is_running_combo && (child_count == 5 || child_count == 7) {
        Some(EmulationSuspect {
            scenario_id: Some(scenario.id.clone()),
            kind: EmulationKind::CombinationAsImpulse,
            message: format!(
                "Scenario[{}] DoubleThree*/TripleThree* Combination 含 {} 段 — \
                 視覺上可能被誤判為 5/7-wave Trending Impulse(spec 1905-1906 一般化)",
                scenario.id, child_count
            ),
        })
    } else {
        None
    }
}

/// Legacy API(Phase 1 skeleton)— Phase 9 改用 detect_all(forest, classified)。
#[deprecated(note = "Phase 9 改用 detect_all(forest, classified)")]
pub fn detect_emulation(_scenario: &Scenario) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::monowave::ProportionMetrics;
    use crate::output::*;
    use chrono::NaiveDate;

    fn cmw_with(
        start_p: f64,
        end_p: f64,
        dur: usize,
        day_offset: i64,
        labels: Vec<(StructureLabel, Certainty)>,
    ) -> ClassifiedMonowave {
        let base = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        let cands = labels
            .into_iter()
            .map(|(l, c)| StructureLabelCandidate {
                label: l,
                certainty: c,
            })
            .collect();
        ClassifiedMonowave {
            monowave: Monowave {
                start_date: base + chrono::Duration::days(day_offset),
                end_date: base + chrono::Duration::days(day_offset + dur as i64 - 1),
                start_price: start_p,
                end_price: end_p,
                direction: if end_p > start_p {
                    MonowaveDirection::Up
                } else {
                    MonowaveDirection::Down
                },
                bar_indices: (0, 0),
            },
            atr_at_start: 1.0,
            metrics: ProportionMetrics {
                magnitude: (end_p - start_p).abs(),
                duration_bars: dur,
                atr_relative: 1.0,
                slope_vs_45deg: 1.0,
            },
            structure_label_candidates: cands,
            polywave_size: 0,
        }
    }

    fn make_scenario(pattern: NeelyPatternType, children: Vec<WaveNode>) -> Scenario {
        let date = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        Scenario {
            id: "test".to_string(),
            wave_tree: WaveNode {
                label: "test".to_string(),
                start: date,
                end: date,
                children,
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
            in_triangle_context: false,
            awaiting_l_label: false,
            monowave_structure_labels: Vec::new(),
            round_state: RoundState::Round1,
            pattern_isolation_anchors: Vec::new(),
            triplexity_detected: false,
        }
    }

    fn wave_node(start_offset: i64, dur: i64) -> WaveNode {
        let base = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        WaveNode {
            label: "W".to_string(),
            start: base + chrono::Duration::days(start_offset),
            end: base + chrono::Duration::days(start_offset + dur - 1),
            children: Vec::new(),
        }
    }

    #[test]
    fn impulse_with_corrective_w3_yields_running_double_three_suspect() {
        // 5 monowaves;W3 (index 2) 含 :c3 Primary 標記
        let classified = vec![
            cmw_with(100.0, 110.0, 5, 0, vec![]),
            cmw_with(110.0, 105.0, 3, 5, vec![]),
            cmw_with(
                105.0,
                125.0,
                5,
                8,
                vec![(StructureLabel::C3, Certainty::Primary)],
            ),
            cmw_with(125.0, 118.0, 3, 13, vec![]),
            cmw_with(118.0, 132.0, 5, 16, vec![]),
        ];
        let scenario = make_scenario(
            NeelyPatternType::Impulse,
            vec![
                wave_node(0, 5),
                wave_node(5, 3),
                wave_node(8, 5),
                wave_node(13, 3),
                wave_node(16, 5),
            ],
        );
        let suspects = detect_for_scenario(&scenario, &classified);
        assert!(suspects.iter().any(|s| matches!(
            s.kind,
            EmulationKind::RunningDoubleThreeAsImpulse
        )));
    }

    #[test]
    fn diagonal_scenario_yields_diagonal_as_impulse_suspect() {
        let scenario = make_scenario(
            NeelyPatternType::Diagonal {
                sub_kind: DiagonalKind::Leading,
            },
            Vec::new(),
        );
        let suspects = detect_for_scenario(&scenario, &[]);
        assert!(suspects
            .iter()
            .any(|s| matches!(s.kind, EmulationKind::DiagonalAsImpulse)));
    }

    #[test]
    fn triangle_with_short_wave_e_yields_failure_suspect() {
        // Triangle:wave-a mag 10,wave-e mag 2(ratio 0.2 < 0.382)
        let classified = vec![
            cmw_with(100.0, 110.0, 5, 0, vec![]),  // wave-a
            cmw_with(110.0, 105.0, 5, 5, vec![]),  // wave-b
            cmw_with(105.0, 108.0, 5, 10, vec![]), // wave-c
            cmw_with(108.0, 106.0, 5, 15, vec![]), // wave-d
            cmw_with(106.0, 108.0, 5, 20, vec![]), // wave-e (mag 2)
        ];
        let scenario = make_scenario(
            NeelyPatternType::Triangle {
                sub_kind: TriangleKind::Contracting,
            },
            vec![
                wave_node(0, 5),
                wave_node(5, 5),
                wave_node(10, 5),
                wave_node(15, 5),
                wave_node(20, 5),
            ],
        );
        let suspects = detect_for_scenario(&scenario, &classified);
        assert!(suspects
            .iter()
            .any(|s| matches!(s.kind, EmulationKind::TriangleAsFailure)));
    }

    #[test]
    fn impulse_with_terminal_channeling_advisory_yields_first_ext_suspect() {
        let mut scenario = make_scenario(NeelyPatternType::Impulse, Vec::new());
        scenario.advisory_findings.push(AdvisoryFinding {
            rule_id: RuleId::Ch5_Channeling_24,
            severity: AdvisorySeverity::Warning,
            message: "2-4 line breach hints Terminal".to_string(),
        });
        let suspects = detect_for_scenario(&scenario, &[]);
        assert!(suspects
            .iter()
            .any(|s| matches!(s.kind, EmulationKind::FirstExtAsTerminal)));
    }

    // v4.5.1 ZigzagAsFlatFailure tests ------------------------------------

    #[test]
    fn zigzag_with_short_wave_c_yields_flat_failure_suspect() {
        // Zigzag wave-a mag 10, wave-b mag 5, wave-c mag 6 → c/a = 0.6 < 1.0
        let classified = vec![
            cmw_with(100.0, 110.0, 5, 0, vec![]),  // wave-a: mag 10
            cmw_with(110.0, 105.0, 5, 5, vec![]),  // wave-b
            cmw_with(105.0, 111.0, 5, 10, vec![]), // wave-c: mag 6
        ];
        let scenario = make_scenario(
            NeelyPatternType::Zigzag {
                sub_kind: ZigzagKind::Single,
            },
            vec![wave_node(0, 5), wave_node(5, 5), wave_node(10, 5)],
        );
        let suspects = detect_for_scenario(&scenario, &classified);
        assert!(suspects
            .iter()
            .any(|s| matches!(s.kind, EmulationKind::ZigzagAsFlatFailure)));
    }

    #[test]
    fn zigzag_with_normal_wave_c_does_not_yield_emulation() {
        // wave-c mag 12 > wave-a mag 10 → not truncated
        let classified = vec![
            cmw_with(100.0, 110.0, 5, 0, vec![]),
            cmw_with(110.0, 105.0, 5, 5, vec![]),
            cmw_with(105.0, 117.0, 5, 10, vec![]),
        ];
        let scenario = make_scenario(
            NeelyPatternType::Zigzag {
                sub_kind: ZigzagKind::Single,
            },
            vec![wave_node(0, 5), wave_node(5, 5), wave_node(10, 5)],
        );
        let suspects = detect_for_scenario(&scenario, &classified);
        assert!(!suspects
            .iter()
            .any(|s| matches!(s.kind, EmulationKind::ZigzagAsFlatFailure)));
    }

    // v4.5.2 FlatAsZigzag tests -------------------------------------------

    #[test]
    fn flat_with_elongated_wave_c_yields_zigzag_suspect() {
        // wave-a mag 10, wave-c mag 15 → c/a = 1.5 ≥ 1.382
        let classified = vec![
            cmw_with(100.0, 110.0, 5, 0, vec![]),
            cmw_with(110.0, 107.0, 5, 5, vec![]),
            cmw_with(107.0, 122.0, 5, 10, vec![]),
        ];
        let scenario = make_scenario(
            NeelyPatternType::Flat {
                sub_kind: FlatKind::Elongated,
            },
            vec![wave_node(0, 5), wave_node(5, 5), wave_node(10, 5)],
        );
        let suspects = detect_for_scenario(&scenario, &classified);
        assert!(suspects
            .iter()
            .any(|s| matches!(s.kind, EmulationKind::FlatAsZigzag)));
    }

    #[test]
    fn flat_common_with_short_wave_c_does_not_yield_emulation() {
        // wave-c mag 11 < wave-a × 1.382 = 13.82 → not Elongated
        let classified = vec![
            cmw_with(100.0, 110.0, 5, 0, vec![]),
            cmw_with(110.0, 102.0, 5, 5, vec![]),
            cmw_with(102.0, 113.0, 5, 10, vec![]),
        ];
        let scenario = make_scenario(
            NeelyPatternType::Flat {
                sub_kind: FlatKind::Common,
            },
            vec![wave_node(0, 5), wave_node(5, 5), wave_node(10, 5)],
        );
        let suspects = detect_for_scenario(&scenario, &classified);
        assert!(!suspects
            .iter()
            .any(|s| matches!(s.kind, EmulationKind::FlatAsZigzag)));
    }

    // v4.5.4 CombinationAsImpulse tests -----------------------------------

    #[test]
    fn double_three_with_five_children_yields_combination_as_impulse_suspect() {
        let scenario = make_scenario(
            NeelyPatternType::Combination {
                sub_kinds: vec![CombinationKind::DoubleThree],
            },
            vec![
                wave_node(0, 5),
                wave_node(5, 5),
                wave_node(10, 5),
                wave_node(15, 5),
                wave_node(20, 5),
            ],
        );
        let suspects = detect_for_scenario(&scenario, &[]);
        assert!(suspects
            .iter()
            .any(|s| matches!(s.kind, EmulationKind::CombinationAsImpulse)));
    }

    #[test]
    fn double_zigzag_combination_does_not_yield_emulation() {
        // DoubleZigzag is a Table A small-x combination, not a DoubleThree* variant
        let scenario = make_scenario(
            NeelyPatternType::Combination {
                sub_kinds: vec![CombinationKind::DoubleZigzag],
            },
            vec![
                wave_node(0, 5),
                wave_node(5, 5),
                wave_node(10, 5),
                wave_node(15, 5),
                wave_node(20, 5),
            ],
        );
        let suspects = detect_for_scenario(&scenario, &[]);
        assert!(!suspects
            .iter()
            .any(|s| matches!(s.kind, EmulationKind::CombinationAsImpulse)));
    }
}
