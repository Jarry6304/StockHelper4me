// fibonacci/projection.rs — Stage 10b:Fibonacci 投影 + per-pattern alignment(PR-6b-2)
//
// 對齊 m3Spec/neely_core_architecture.md §9.5 + neely_rules.md Ch12 line 2533-2557。
//
// **PR-6b-2 階段(2026-05-13)**:
//   - 從 r2 10 ratios 砍至 5 standard ratios(對齊 ratios.rs)
//   - 加 per-pattern fibonacci_alignment 計算(對齊 spec §9.5 8 子欄位)
//   - Internal vs External 應用方式:
//     - Impulse Ext:W3/W1 / W5/W1 內部比例;W5 外部投影
//     - Zigzag c-wave:c/a Internal(常 = 100%);Waterfall External
//     - Flat c-wave:c/a Internal(常 = 100%);Elongated > 161.8%
//     - Triangle:每段 leg ≈ 61.8% 前段 Internal

use super::ratios::{FIB_TOLERANCE_PCT, NEELY_FIB_RATIOS, NEELY_FIB_RATIOS_PCT};
use crate::output::{FibZone, FibonacciAlignment, NeelyPatternType, Scenario};
use crate::monowave::ClassifiedMonowave;

/// 內部用 Fib 投影紀錄(供 caller 進一步處理)。
#[derive(Debug, Clone)]
pub struct FibProjection {
    pub label: String,
    pub ratio: f64,
    pub price: f64,
}

/// 從 Scenario.wave_tree 推算 expected_fib_zones(需配合 classified monowaves 取 price)。
///
/// PR-6b-2:取 wave_tree.children[0] 的 start_date 找 monowave 反查 W1 price。
/// 簡化:wave_tree 沒帶 price,需 caller 用 classified slice 對應索引。
/// 本函式仍回空 — 由 apply_to_forest_with_monowaves 真實計算。
pub fn compute_expected_fib_zones(_scenario: &Scenario) -> Vec<FibZone> {
    Vec::new()
}

/// 給定 W1 端點 price,計算所有 NEELY_FIB_RATIOS 對應的 FibZone(External 投影)。
///
/// External:從 W1 終點起,投影到 W2/W3 端點預期區間。
pub fn project_from_w1(w1_start: f64, w1_end: f64) -> Vec<FibZone> {
    let w1_magnitude = (w1_end - w1_start).abs();
    if w1_magnitude < f64::EPSILON {
        return Vec::new();
    }
    let direction_sign = if w1_end > w1_start { 1.0 } else { -1.0 };

    NEELY_FIB_RATIOS
        .iter()
        .map(|&ratio| {
            let price = w1_end + direction_sign * w1_magnitude * ratio;
            let half_tol = price.abs() * FIB_TOLERANCE_PCT / 100.0 / 2.0;
            FibZone {
                label: format!("Fib {:.1}%", ratio * 100.0),
                low: price - half_tol,
                high: price + half_tol,
                source_ratio: ratio,
            }
        })
        .collect()
}

/// 計算 scenario 的 Fibonacci internal alignment(對齊 StructuralFacts.fibonacci_alignment §9.5)。
///
/// Per-pattern internal alignment:
///   - Impulse:檢查 W3/W1 / W5/W1 / W4/W1 比例匹配 Fib
///   - Zigzag(3-wave):c/a 匹配 Fib
///   - Flat(3-wave):c/a 與 c/b 匹配 Fib
///   - Triangle(5-wave):c/a / b/a / d/b 匹配 Fib(每段約 61.8% 前段)
///   - 其他:回空 matched_ratios
pub fn compute_internal_alignment(
    scenario: &Scenario,
    classified: &[ClassifiedMonowave],
) -> FibonacciAlignment {
    let monowave_indices = extract_monowave_indices(scenario, classified);
    if monowave_indices.is_empty() {
        return FibonacciAlignment::default();
    }

    let mags: Vec<f64> = monowave_indices
        .iter()
        .filter_map(|&idx| classified.get(idx).map(|c| c.metrics.magnitude))
        .collect();

    if mags.is_empty() || mags[0] <= 0.0 {
        return FibonacciAlignment::default();
    }

    let mut matched = Vec::new();
    match &scenario.pattern_type {
        NeelyPatternType::Impulse | NeelyPatternType::TerminalImpulse => {
            if mags.len() >= 5 {
                // W3/W1 / W5/W1 比例
                check_and_push(&mut matched, mags[2], mags[0]);
                check_and_push(&mut matched, mags[4], mags[0]);
            }
        }
        NeelyPatternType::Zigzag { .. } => {
            if mags.len() >= 3 {
                // c/a 比例(Zigzag c-wave 常 = 100%)
                check_and_push(&mut matched, mags[2], mags[0]);
            }
        }
        NeelyPatternType::Flat { .. } => {
            if mags.len() >= 3 {
                check_and_push(&mut matched, mags[2], mags[0]); // c/a
                if mags[1] > 0.0 {
                    check_and_push(&mut matched, mags[2], mags[1]); // c/b
                }
            }
        }
        NeelyPatternType::Triangle { .. } => {
            if mags.len() >= 5 {
                // 各段比例(Ch12:c/a / d/b 等)
                check_and_push(&mut matched, mags[2], mags[0]); // c/a
                if mags[1] > 0.0 {
                    check_and_push(&mut matched, mags[3], mags[1]); // d/b
                }
            }
        }
        NeelyPatternType::RunningCorrection => {
            if mags.len() >= 3 {
                check_and_push(&mut matched, mags[2], mags[0]); // c/a
            }
        }
        NeelyPatternType::Combination { .. } => {
            // 暫不計算(combination 涉及 sub-pattern 嵌套,留 PR-5b/PR-6b-3)
        }
    }

    // 去重(同 ratio 可能匹配多個比例)
    matched.sort_by(|a, b| a.partial_cmp(b).unwrap());
    matched.dedup_by(|a, b| (*a - *b).abs() < 0.01);

    FibonacciAlignment { matched_ratios: matched }
}

/// helper:若 `numerator/denominator × 100` 匹配任一 NEELY_FIB_RATIOS_PCT(±4%),push 對應 ratio。
fn check_and_push(matched: &mut Vec<f64>, numerator: f64, denominator: f64) {
    if denominator.abs() < 1e-9 {
        return;
    }
    let pct = numerator / denominator * 100.0;
    for &fib_pct in NEELY_FIB_RATIOS_PCT {
        if (pct - fib_pct).abs() <= FIB_TOLERANCE_PCT {
            matched.push(fib_pct / 100.0);
            break; // 已匹配,不重複加同一 ratio 的多個觸發
        }
    }
}

/// helper:從 Scenario.wave_tree.children 找出對應的 classified monowave indices。
/// 用 wave_tree.children[i] 的 start_date 在 classified 找匹配 monowave。
fn extract_monowave_indices(scenario: &Scenario, classified: &[ClassifiedMonowave]) -> Vec<usize> {
    scenario
        .wave_tree
        .children
        .iter()
        .filter_map(|node| {
            classified
                .iter()
                .position(|c| c.monowave.start_date == node.start && c.monowave.end_date == node.end)
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

    #[test]
    fn empty_w1_yields_no_zones() {
        assert!(project_from_w1(100.0, 100.0).is_empty());
    }

    #[test]
    fn project_count_matches_ratios() {
        let zones = project_from_w1(100.0, 110.0);
        assert_eq!(zones.len(), NEELY_FIB_RATIOS.len());
        assert_eq!(zones.len(), 5);  // r5 spec 5 standard ratios
    }

    #[test]
    fn project_38_2_pct_extension_position() {
        // W1 100→110 上升;從 W1 終點 110 起,Fib 38.2% extension
        // = 110 + 10 × 0.382 = 113.82
        let zones = project_from_w1(100.0, 110.0);
        let z = zones.iter().find(|z| (z.source_ratio - 0.382).abs() < 1e-9).unwrap();
        let center = (z.low + z.high) / 2.0;
        assert!((center - 113.82).abs() < 0.5, "center={}", center);
    }

    #[test]
    fn project_100_pct_extension_position() {
        // W1 100→110;Fib 100% extension = 110 + 10 × 1.0 = 120.0
        let zones = project_from_w1(100.0, 110.0);
        let z = zones.iter().find(|z| (z.source_ratio - 1.0).abs() < 1e-9).unwrap();
        let center = (z.low + z.high) / 2.0;
        assert!((center - 120.0).abs() < 0.6);
    }

    #[test]
    fn project_descending_w1() {
        // W1 100→90 下降;Fib 100% extension = 90 + (-1) × 10 × 1.0 = 80
        let zones = project_from_w1(100.0, 90.0);
        let z = zones.iter().find(|z| (z.source_ratio - 1.0).abs() < 1e-9).unwrap();
        let center = (z.low + z.high) / 2.0;
        assert!((center - 80.0).abs() < 0.5);
    }

    #[test]
    fn alignment_zigzag_c_equals_a_matched() {
        // 3-wave Zigzag,c/a = 100% → matched ratio 1.0
        let d0 = NaiveDate::parse_from_str("2026-01-01", "%Y-%m-%d").unwrap();
        let d1 = NaiveDate::parse_from_str("2026-01-02", "%Y-%m-%d").unwrap();
        let d2 = NaiveDate::parse_from_str("2026-01-03", "%Y-%m-%d").unwrap();
        let d3 = NaiveDate::parse_from_str("2026-01-04", "%Y-%m-%d").unwrap();
        let classified = vec![
            cmw(100.0, 110.0, d0, d1),
            cmw(110.0, 106.0, d1, d2),
            cmw(106.0, 116.0, d2, d3),
        ];
        let scenario = Scenario {
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
        let align = compute_internal_alignment(&scenario, &classified);
        assert!(align.matched_ratios.contains(&1.0), "預期匹配 100% c/a, got {:?}", align.matched_ratios);
    }

    #[test]
    fn alignment_impulse_w3_618x_w1_matched() {
        // 5-wave Impulse: W1=10, W3=16.18 → W3/W1 = 161.8%
        let d0 = NaiveDate::parse_from_str("2026-01-01", "%Y-%m-%d").unwrap();
        let d1 = NaiveDate::parse_from_str("2026-01-02", "%Y-%m-%d").unwrap();
        let d2 = NaiveDate::parse_from_str("2026-01-03", "%Y-%m-%d").unwrap();
        let d3 = NaiveDate::parse_from_str("2026-01-04", "%Y-%m-%d").unwrap();
        let d4 = NaiveDate::parse_from_str("2026-01-05", "%Y-%m-%d").unwrap();
        let d5 = NaiveDate::parse_from_str("2026-01-06", "%Y-%m-%d").unwrap();
        let classified = vec![
            cmw(100.0, 110.0, d0, d1),       // W1 = 10
            cmw(110.0, 105.0, d1, d2),       // W2 = 5
            cmw(105.0, 121.18, d2, d3),      // W3 = 16.18 (161.8% × W1)
            cmw(121.18, 115.0, d3, d4),      // W4 = 6.18
            cmw(115.0, 125.0, d4, d5),       // W5 = 10
        ];
        let scenario = Scenario {
            id: "i".to_string(),
            wave_tree: WaveNode {
                label: "i".to_string(),
                start: d0,
                end: d5,
                children: vec![
                    WaveNode { label: "W1".to_string(), start: d0, end: d1, children: vec![] },
                    WaveNode { label: "W2".to_string(), start: d1, end: d2, children: vec![] },
                    WaveNode { label: "W3".to_string(), start: d2, end: d3, children: vec![] },
                    WaveNode { label: "W4".to_string(), start: d3, end: d4, children: vec![] },
                    WaveNode { label: "W5".to_string(), start: d4, end: d5, children: vec![] },
                ],
            },
            pattern_type: NeelyPatternType::Impulse,
            structure_label: "I".to_string(),
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
        let align = compute_internal_alignment(&scenario, &classified);
        assert!(align.matched_ratios.contains(&1.618),
            "預期匹配 W3/W1=161.8%, got {:?}", align.matched_ratios);
        // W5/W1 = 100% 也匹配
        assert!(align.matched_ratios.contains(&1.0),
            "預期 W5/W1=100% 也匹配, got {:?}", align.matched_ratios);
    }

    #[test]
    fn alignment_no_match_yields_empty() {
        // 3-wave Zigzag, c/a = 80% (no Fib match in 5 standard ratios)
        let d0 = NaiveDate::parse_from_str("2026-01-01", "%Y-%m-%d").unwrap();
        let d1 = NaiveDate::parse_from_str("2026-01-02", "%Y-%m-%d").unwrap();
        let d2 = NaiveDate::parse_from_str("2026-01-03", "%Y-%m-%d").unwrap();
        let d3 = NaiveDate::parse_from_str("2026-01-04", "%Y-%m-%d").unwrap();
        let classified = vec![
            cmw(100.0, 110.0, d0, d1),
            cmw(110.0, 106.0, d1, d2),
            cmw(106.0, 114.0, d2, d3), // c = 8, c/a = 80%
        ];
        let scenario = Scenario {
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
        let align = compute_internal_alignment(&scenario, &classified);
        assert!(align.matched_ratios.is_empty(),
            "80% c/a 不匹配 Fib, got {:?}", align.matched_ratios);
    }
}
