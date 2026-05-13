// complexity — Stage 7:Complexity Rule
//
// 對齊 m2Spec/oldm2Spec/neely_core.md §三 / §七 Stage 7。
// Neely Complexity Level(Item 8.1)— 篩選 Scenario 的複雜度層級。
//
// **M3 PR-4 階段**(先實踐以後再改):
//   - Stage 5 Classifier 已寫入每 Scenario 的 complexity_level 欄位
//   - Stage 7 Complexity Rule 是 *篩選器*:傳入 Forest,過濾掉複雜度差距 > 1 級的 scenario
//   - 對齊 oldm2Spec/neely_core.md §11.3「Complexity Rule(差距 ≤ 1 級)」
//   - PR-4 階段:篩選邏輯落地,但 Complexity 判別細節(用什麼層級)留 PR-4b

use crate::output::{ComplexityLevel, Scenario};

/// 對 Scenario list 套 Complexity Rule:差距 > 1 級的 scenario 篩除。
///
/// 演算法:
///   1. 取所有 scenario 的 complexity_level
///   2. 找最常見 / 最低 complexity 為 anchor
///   3. anchor ± 1 級內保留,其餘篩除
///
/// **M3 PR-4 階段**:用「最低 complexity 為 anchor」(保守:傾向保留簡單 case)
/// PR-4b 校準後可能改用「眾數」或 frequency-weighted。
pub fn apply_complexity_rule(scenarios: Vec<Scenario>) -> Vec<Scenario> {
    if scenarios.len() <= 1 {
        return scenarios;
    }

    // 取最低 complexity level 為 anchor
    let anchor = scenarios
        .iter()
        .map(|s| level_to_int(s.complexity_level))
        .min()
        .unwrap_or(0);

    scenarios
        .into_iter()
        .filter(|s| {
            let diff = (level_to_int(s.complexity_level) - anchor).abs();
            diff <= 1
        })
        .collect()
}

fn level_to_int(level: ComplexityLevel) -> i32 {
    match level {
        ComplexityLevel::Simple => 0,
        ComplexityLevel::Intermediate => 1,
        ComplexityLevel::Complex => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::*;
    use chrono::NaiveDate;

    fn make_scenario(id: &str, complexity: ComplexityLevel) -> Scenario {
        let date = NaiveDate::parse_from_str("2026-01-01", "%Y-%m-%d").unwrap();
        Scenario {
            id: id.to_string(),
            wave_tree: WaveNode {
                label: id.to_string(),
                start: date,
                end: date,
                children: Vec::new(),
            },
            pattern_type: NeelyPatternType::Impulse,
            initial_direction: MonowaveDirection::Up,
            compacted_base_label: StructureLabel::Five,
            structure_label: "test".to_string(),
            complexity_level: complexity,
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
            advisory_findings: Vec::new(),
        }
    }

    #[test]
    fn empty_or_single_scenario_pass_through() {
        assert_eq!(apply_complexity_rule(vec![]).len(), 0);
        let one = vec![make_scenario("a", ComplexityLevel::Complex)];
        assert_eq!(apply_complexity_rule(one).len(), 1);
    }

    #[test]
    fn complexity_diff_within_one_kept() {
        // anchor = Simple(0);Simple/Intermediate 都保留,Complex(2)被剔除
        let scenarios = vec![
            make_scenario("a", ComplexityLevel::Simple),
            make_scenario("b", ComplexityLevel::Intermediate),
            make_scenario("c", ComplexityLevel::Complex),
        ];
        let filtered = apply_complexity_rule(scenarios);
        assert_eq!(filtered.len(), 2);
        let ids: Vec<&str> = filtered.iter().map(|s| s.id.as_str()).collect();
        assert!(ids.contains(&"a") && ids.contains(&"b"));
        assert!(!ids.contains(&"c"));
    }

    #[test]
    fn anchor_is_lowest_complexity() {
        // 全 Complex 時 anchor = Complex,差距 0 全保留
        let scenarios = vec![
            make_scenario("a", ComplexityLevel::Complex),
            make_scenario("b", ComplexityLevel::Complex),
        ];
        assert_eq!(apply_complexity_rule(scenarios).len(), 2);
    }
}
