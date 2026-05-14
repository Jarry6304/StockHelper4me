// emulation — Stage 9b:Emulation 偵測(Ch12 Emulation)
//
// 對齊 m3Spec/neely_rules.md §Ch8 Non-Standard Polywaves(1902-1906 行 Running 變體辨識要點)
//       + §Ch12 Emulation
//       + m3Spec/neely_core_architecture.md §7.1 Stage 9b + §9.3 Ch12_Emulation
//
// **Phase 9 PR**(完整 Ch12 Emulation 實作):
//   對 forest 中每個 scenario 套用視覺/結構 emulation 偵測。Emulation 是「視覺上
//   相似於 X 但結構規則屬 Y」的場景。Phase 9 偵測四種主要 emulation kind:
//   1. RunningDoubleThreeAsImpulse(spec 1905-1906):
//      Running Double Three Combination 偽裝 1st Wave Extension Impulse
//      辨識:該 5-wave Impulse 的 W3 monowave structure_label_candidates 含 :3 系列
//   2. DiagonalAsImpulse:Diagonal 偽裝 Trending Impulse
//      Phase 9 簡化:已被 classifier 區分,scenario.pattern_type 是 Diagonal 即標
//   3. TriangleAsFailure:Triangle 偽裝 5-wave Failure
//      辨識:Triangle scenario + 末段相對短(可能被誤判 Truncated)
//   4. FirstExtAsTerminal:1st Ext Impulse 偽裝 Terminal Impulse
//      辨識:Impulse scenario + advisory_findings 中 Ch5_Overlap_* 接近邊界

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
        // Zigzag / Flat / Combination:Phase 9 暫不偵測 emulation
        _ => {}
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
            },
            atr_at_start: 1.0,
            metrics: ProportionMetrics {
                magnitude: (end_p - start_p).abs(),
                duration_bars: dur,
                atr_relative: 1.0,
                slope_vs_45deg: 1.0,
            },
            structure_label_candidates: cands,
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
}
