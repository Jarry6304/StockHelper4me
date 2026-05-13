// emulation — Stage 9b:Emulation 辨識(PR-6b-3 完整實作)
//
// 對齊 m3Spec/neely_rules.md Ch12 line 2559-2578 Emulation kinds。
//
// 5 種 EmulationKind(spec §9.3 line 1026):
//   - DoubleFailureAsTriangle:Double Failure 模仿 Triangle(假 d-wave 突破 a/b 端點即破除)
//   - DoubleFlatAsImpulse:Double Flat 模仿 1-2-3 with 3rd Ext(缺 x-wave,2/4 缺 Alternation)
//   - MultiZigzagAsImpulse:Double/Triple Zigzag 模仿 Impulse(通道完美;wave-5 後 2-4 線未及時破)
//   - FirstExtAsZigzag:1st Ext(缺 wave-4)看起來像 c ≤ a 的 Zigzag
//   - FifthExtAsZigzag:5th Ext(缺 wave-2)看起來像 c > a 的 Zigzag
//
// 設計:emulation 識別基於 pattern_type 與 sub_kind 的「易混淆」啟發式判斷。
// 對 forest 中的每個 scenario 偵測潛在 emulation 風險。

use crate::monowave::ClassifiedMonowave;
use crate::output::{
    EmulationKind, FlatVariant, NeelyPatternType, Scenario,
};

/// 偵測 Scenario 的 emulation kind(若無 emulation 風險 → None)。
///
/// **PR-6b-3 階段** 簡化版啟發式:
///   - Flat DoubleFailure → 可能模仿 Triangle(若 c < b < a 收斂)
///   - Flat / Combination with sub_kinds DoubleThree → 可能模仿 Impulse
///   - Zigzag with c < a × 0.5 → 可能 1st Ext 缺 wave-4
///   - Zigzag with c > a × 2.0 → 可能 5th Ext 缺 wave-2
pub fn detect_emulation(
    scenario: &Scenario,
    classified: &[ClassifiedMonowave],
) -> Option<EmulationKind> {
    let mags = collect_wave_magnitudes(scenario, classified);

    match &scenario.pattern_type {
        NeelyPatternType::Flat { sub_kind: FlatVariant::DoubleFailure } => {
            // DoubleFailure 形態自然模仿 Triangle(b 弱 + c 弱 = 收斂)
            Some(EmulationKind::DoubleFailureAsTriangle)
        }
        NeelyPatternType::Combination { sub_kinds } => {
            // 任何 Combination 都可能模仿 Impulse(尤其 Double)
            if sub_kinds.is_empty() {
                None
            } else {
                // Double Flat 是 Combination + 內含 Flat sub-kinds(此實作 sub_kinds 用 CombinationKind enum
                // 不分 Flat/Zigzag,所以 DoubleThree → DoubleFlatAsImpulse 或 MultiZigzagAsImpulse 都可能;
                // 為簡化:DoubleThree → DoubleFlatAsImpulse,TripleThree → MultiZigzagAsImpulse)
                Some(EmulationKind::DoubleFlatAsImpulse)
            }
        }
        NeelyPatternType::Zigzag { .. } => {
            // Zigzag 對映 Impulse 殘缺 wave-2 或 wave-4 的偽 Zigzag 樣貌
            if mags.len() >= 3 {
                let a = mags[0];
                let c = mags[2];
                if a > 0.0 {
                    let ratio = c / a;
                    if ratio < 0.5 {
                        return Some(EmulationKind::FirstExtAsZigzag);
                    }
                    if ratio > 2.0 {
                        return Some(EmulationKind::FifthExtAsZigzag);
                    }
                }
            }
            None
        }
        _ => None,
    }
}

/// 對 Forest 中所有 scenario 套 emulation 偵測;
/// 寫到 scenario.structural_facts.overlap_pattern.label(同 missing_wave space)。
pub fn apply_to_forest(forest: &mut [Scenario], classified: &[ClassifiedMonowave]) -> Vec<Option<EmulationKind>> {
    let mut results = Vec::with_capacity(forest.len());
    for scenario in forest.iter_mut() {
        let emul = detect_emulation(scenario, classified);
        if let Some(kind) = emul {
            let tag = format!("emul:{:?}", kind);
            let existing = std::mem::take(&mut scenario.structural_facts.overlap_pattern.label);
            scenario.structural_facts.overlap_pattern.label = if existing.is_empty() {
                tag
            } else {
                format!("{};{}", existing, tag)
            };
        }
        results.push(emul);
    }
    results
}

fn collect_wave_magnitudes(scenario: &Scenario, classified: &[ClassifiedMonowave]) -> Vec<f64> {
    scenario
        .wave_tree
        .children
        .iter()
        .filter_map(|node| {
            classified
                .iter()
                .find(|c| c.monowave.start_date == node.start && c.monowave.end_date == node.end)
                .map(|c| c.metrics.magnitude)
        })
        .collect()
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

    fn make_zigzag_scenario(a_end: f64, b_end: f64, c_end: f64) -> (Scenario, Vec<ClassifiedMonowave>) {
        let d0 = NaiveDate::parse_from_str("2026-01-01", "%Y-%m-%d").unwrap();
        let d1 = NaiveDate::parse_from_str("2026-01-02", "%Y-%m-%d").unwrap();
        let d2 = NaiveDate::parse_from_str("2026-01-03", "%Y-%m-%d").unwrap();
        let d3 = NaiveDate::parse_from_str("2026-01-04", "%Y-%m-%d").unwrap();
        let classified = vec![
            cmw(100.0, a_end, d0, d1),
            cmw(a_end, b_end, d1, d2),
            cmw(b_end, c_end, d2, d3),
        ];
        let s = Scenario {
            id: "z".to_string(),
            wave_tree: WaveNode {
                label: "z".to_string(),
                start: d0,
                end: d3,
                children: vec![
                    WaveNode { label: "a".to_string(), start: d0, end: d1, children: vec![] },
                    WaveNode { label: "b".to_string(), start: d1, end: d2, children: vec![] },
                    WaveNode { label: "c".to_string(), start: d2, end: d3, children: vec![] },
                ],
            },
            pattern_type: NeelyPatternType::Zigzag { sub_kind: ZigzagVariant::Normal },
            structure_label: "Z".to_string(),
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
        (s, classified)
    }

    #[test]
    fn double_failure_flat_emulates_triangle() {
        let date = NaiveDate::parse_from_str("2026-01-01", "%Y-%m-%d").unwrap();
        let s = Scenario {
            id: "df".to_string(),
            wave_tree: WaveNode {
                label: "df".to_string(),
                start: date,
                end: date,
                children: vec![],
            },
            pattern_type: NeelyPatternType::Flat { sub_kind: FlatVariant::DoubleFailure },
            structure_label: "Flat DF".to_string(),
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
        let result = detect_emulation(&s, &[]);
        assert!(matches!(result, Some(EmulationKind::DoubleFailureAsTriangle)));
    }

    #[test]
    fn combination_emulates_impulse() {
        let date = NaiveDate::parse_from_str("2026-01-01", "%Y-%m-%d").unwrap();
        let s = Scenario {
            id: "comb".to_string(),
            wave_tree: WaveNode {
                label: "comb".to_string(),
                start: date,
                end: date,
                children: vec![],
            },
            pattern_type: NeelyPatternType::Combination {
                sub_kinds: vec![CombinationKind::DoubleThree],
            },
            structure_label: "Combination".to_string(),
            complexity_level: ComplexityLevel::Complex,
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
        let result = detect_emulation(&s, &[]);
        assert!(matches!(result, Some(EmulationKind::DoubleFlatAsImpulse)));
    }

    #[test]
    fn zigzag_short_c_emulates_first_ext() {
        // a=10, c=3, c/a = 0.3 < 0.5 → FirstExtAsZigzag
        let (s, c) = make_zigzag_scenario(110.0, 105.0, 102.0);
        // a 從 100→110 = 10, b 110→105 = 5, c 105→102 = 3 → c/a = 0.3
        let result = detect_emulation(&s, &c);
        assert!(matches!(result, Some(EmulationKind::FirstExtAsZigzag)));
    }

    #[test]
    fn zigzag_long_c_emulates_fifth_ext() {
        // a=10, c=25, c/a = 2.5 > 2.0 → FifthExtAsZigzag
        let (s, c) = make_zigzag_scenario(110.0, 105.0, 130.0);
        // a = 10, c = 130-105 = 25, c/a = 2.5
        let result = detect_emulation(&s, &c);
        assert!(matches!(result, Some(EmulationKind::FifthExtAsZigzag)));
    }

    #[test]
    fn zigzag_normal_no_emulation() {
        // a=10, c=10, c/a = 1.0(normal Zigzag)→ no emulation
        let (s, c) = make_zigzag_scenario(110.0, 105.0, 115.0);
        let result = detect_emulation(&s, &c);
        assert!(result.is_none());
    }

    #[test]
    fn impulse_no_emulation() {
        let date = NaiveDate::parse_from_str("2026-01-01", "%Y-%m-%d").unwrap();
        let s = Scenario {
            id: "imp".to_string(),
            wave_tree: WaveNode {
                label: "imp".to_string(),
                start: date,
                end: date,
                children: vec![],
            },
            pattern_type: NeelyPatternType::Impulse,
            structure_label: "Impulse".to_string(),
            complexity_level: ComplexityLevel::Intermediate,
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
        assert!(detect_emulation(&s, &[]).is_none());
    }

    #[test]
    fn apply_to_forest_writes_emulation_label() {
        let date = NaiveDate::parse_from_str("2026-01-01", "%Y-%m-%d").unwrap();
        let mut forest = vec![Scenario {
            id: "df".to_string(),
            wave_tree: WaveNode {
                label: "df".to_string(),
                start: date,
                end: date,
                children: vec![],
            },
            pattern_type: NeelyPatternType::Flat { sub_kind: FlatVariant::DoubleFailure },
            structure_label: "DF".to_string(),
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
        }];
        let results = apply_to_forest(&mut forest, &[]);
        assert!(matches!(results[0], Some(EmulationKind::DoubleFailureAsTriangle)));
        assert!(forest[0].structural_facts.overlap_pattern.label.contains("emul:"));
    }
}
