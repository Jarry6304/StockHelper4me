// missing_wave — Stage 9a:Missing Wave 偵測(PR-6b-3 完整實作)
//
// 對齊 m3Spec/neely_rules.md Ch12 line 2580-2597 Missing Waves lookup table。
//
// Ch12 Min data points lookup:
//   | Polywave 形態      | 最少資料點 | 缺點認定                                  |
//   | Impulse           | 8         | < 4 肯定;4-8 可能;≥ 16 不應             |
//   | Zigzag/Flat       | 5         | < 2.5 肯定;2.5-5 可能;≥ 10 不應         |
//   | Double 結尾 Triangle | 13      | < 6.5 肯定;≥ 26 不應                    |
//   | Triple 結尾 Triangle | 18      | < 9 肯定;≥ 36 不應                      |
//
// 邏輯:對每個 Scenario,基於 pattern_type 查表並比對 candidate monowave_indices 數量。

use crate::monowave::ClassifiedMonowave;
use crate::output::{NeelyPatternType, Scenario};

/// 從 Scenario.wave_tree.children 計算對應的 monowave 數量。
fn count_monowaves_in_scenario(scenario: &Scenario) -> usize {
    scenario.wave_tree.children.len()
}

/// 取得 pattern_type 對應的最少資料點(Ch12 lookup)。
fn min_data_points_for(pattern: &NeelyPatternType) -> usize {
    match pattern {
        NeelyPatternType::Impulse | NeelyPatternType::TerminalImpulse => 8,
        NeelyPatternType::Zigzag { .. } | NeelyPatternType::Flat { .. }
        | NeelyPatternType::RunningCorrection => 5,
        NeelyPatternType::Triangle { .. } => 13,
        NeelyPatternType::Combination { sub_kinds } => {
            // Combination 內 sub_kinds 數量決定:1 個 = Double Triangle,2+ = Triple
            if sub_kinds.len() >= 2 { 18 } else { 13 }
        }
    }
}

/// 偵測 Scenario 是否「missing wave」(資料點不足)。
///
/// 規則(Ch12):
///   - count < min × 0.5(50% 以下)→ 肯定缺漏(true)
///   - min × 0.5 ≤ count < min → 可能缺漏(true,保守標記)
///   - count ≥ min → 不缺漏(false)
pub fn detect_missing_wave(scenario: &Scenario) -> bool {
    let count = count_monowaves_in_scenario(scenario);
    let min = min_data_points_for(&scenario.pattern_type);
    count < min
}

/// 對 Forest 中所有 scenario 套 missing_wave 偵測;
/// 寫入 scenario.structural_facts(用 overlap_pattern.label 暫存 — 不破壞既有 schema)。
pub fn apply_to_forest(forest: &mut [Scenario]) -> Vec<bool> {
    let mut results = Vec::with_capacity(forest.len());
    for scenario in forest.iter_mut() {
        let missing = detect_missing_wave(scenario);
        results.push(missing);
        if missing {
            // 標到 overlap_pattern.label(Stage 9a 暫用,Stage 9b emulation 也用同 label space)
            let existing = std::mem::take(&mut scenario.structural_facts.overlap_pattern.label);
            scenario.structural_facts.overlap_pattern.label = if existing.is_empty() {
                "missing_wave".to_string()
            } else {
                format!("{};missing_wave", existing)
            };
        }
    }
    results
}

/// 偵測 5-wave 中是否有單一 wave magnitude ≈ 0(suggests missing sub-wave segment)。
/// 補強 detect_missing_wave 的細粒度判斷。
pub fn has_zero_magnitude_wave(
    scenario: &Scenario,
    classified: &[ClassifiedMonowave],
) -> bool {
    for child in &scenario.wave_tree.children {
        if let Some(c) = classified
            .iter()
            .find(|c| c.monowave.start_date == child.start && c.monowave.end_date == child.end)
        {
            if c.metrics.magnitude.abs() < 1e-9 {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::*;
    use chrono::NaiveDate;

    fn make_scenario(pattern: NeelyPatternType, num_children: usize) -> Scenario {
        let date = NaiveDate::parse_from_str("2026-01-01", "%Y-%m-%d").unwrap();
        let children = (0..num_children)
            .map(|_| WaveNode {
                label: "w".to_string(),
                start: date,
                end: date,
                children: Vec::new(),
            })
            .collect();
        Scenario {
            id: "test".to_string(),
            wave_tree: WaveNode {
                label: "test".to_string(),
                start: date,
                end: date,
                children,
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
            awaiting_l_label: false,
        }
    }

    #[test]
    fn impulse_with_5_children_missing_under_8() {
        // Impulse min=8,5 children → missing
        let s = make_scenario(NeelyPatternType::Impulse, 5);
        assert!(detect_missing_wave(&s));
    }

    #[test]
    fn impulse_with_10_children_not_missing() {
        // 10 ≥ 8 → not missing
        let s = make_scenario(NeelyPatternType::Impulse, 10);
        assert!(!detect_missing_wave(&s));
    }

    #[test]
    fn zigzag_with_3_children_missing_under_5() {
        let s = make_scenario(
            NeelyPatternType::Zigzag { sub_kind: ZigzagVariant::Normal },
            3,
        );
        assert!(detect_missing_wave(&s));
    }

    #[test]
    fn zigzag_with_6_children_not_missing() {
        let s = make_scenario(
            NeelyPatternType::Zigzag { sub_kind: ZigzagVariant::Normal },
            6,
        );
        assert!(!detect_missing_wave(&s));
    }

    #[test]
    fn triangle_with_5_children_missing_under_13() {
        let s = make_scenario(
            NeelyPatternType::Triangle { sub_kind: TriangleVariant::HorizontalLimiting },
            5,
        );
        assert!(detect_missing_wave(&s));
    }

    #[test]
    fn triangle_with_15_children_not_missing() {
        let s = make_scenario(
            NeelyPatternType::Triangle { sub_kind: TriangleVariant::HorizontalLimiting },
            15,
        );
        assert!(!detect_missing_wave(&s));
    }

    #[test]
    fn combination_double_min_13() {
        let s = make_scenario(
            NeelyPatternType::Combination {
                sub_kinds: vec![CombinationKind::DoubleThree],
            },
            10,
        );
        assert!(detect_missing_wave(&s)); // 10 < 13
    }

    #[test]
    fn combination_triple_min_18() {
        let s = make_scenario(
            NeelyPatternType::Combination {
                sub_kinds: vec![CombinationKind::TripleThree, CombinationKind::DoubleThree],
            },
            17,
        );
        assert!(detect_missing_wave(&s)); // 17 < 18
    }

    #[test]
    fn apply_to_forest_marks_missing_wave_label() {
        let mut forest = vec![
            make_scenario(NeelyPatternType::Impulse, 3),  // missing
            make_scenario(NeelyPatternType::Impulse, 10), // not missing
        ];
        let results = apply_to_forest(&mut forest);
        assert_eq!(results, vec![true, false]);
        assert!(forest[0].structural_facts.overlap_pattern.label.contains("missing_wave"));
        assert!(!forest[1].structural_facts.overlap_pattern.label.contains("missing_wave"));
    }

    #[test]
    fn apply_to_empty_forest() {
        let mut forest: Vec<Scenario> = vec![];
        assert_eq!(apply_to_forest(&mut forest).len(), 0);
    }
}
