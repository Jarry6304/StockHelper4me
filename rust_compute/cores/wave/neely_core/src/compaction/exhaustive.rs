// exhaustive.rs — Compaction 窮舉模式(M3 PR-5 簡化版)
//
// 對齊 m2Spec/oldm2Spec/neely_core.md §十一(Compaction 重新定位)。
//
// 設計目標:
//   - 純結構壓縮(§11.2)— 不選最優,窮舉所有合法 compression paths
//   - 對齊 v2.0「展示式」哲學(§2.1)
//
// **M3 PR-5 階段**(先實踐以後再改):
//   - 簡化版:每個 Stage 5-7 通過的 Scenario 直接 pass-through 進 Forest
//   - 「合法 compression paths 窮舉」需要 sub-wave 嵌套結構(degree 階層 W1-W2 內部
//     再有 sub-W1-W2)— 那是 Stage 3 Bottom-up Generator 進階 + 5-wave-of-3 嵌套
//     後才能做。先用 pass-through 把 pipeline 走通,留 PR-5b 補完整窮舉。

use crate::output::Scenario;

/// 窮舉所有合法 compression paths,產出 Forest。
///
/// **M3 PR-5 階段**:pass-through(每 Scenario 直接成 Forest 一棵樹)。
pub fn compact(scenarios: Vec<Scenario>) -> Vec<Scenario> {
    // 留 PR-5b:窮舉合法壓縮路徑(需 sub-wave 嵌套結構)
    scenarios
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::*;
    use chrono::NaiveDate;

    fn make_scenario(id: &str) -> Scenario {
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
            complexity_level: ComplexityLevel::Simple,
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
            in_triangle_context: false,
            awaiting_l_label: false,
        }
    }

    #[test]
    fn pass_through_preserves_count() {
        let scenarios = vec![make_scenario("a"), make_scenario("b"), make_scenario("c")];
        let forest = compact(scenarios);
        assert_eq!(forest.len(), 3);
    }

    #[test]
    fn empty_input_yields_empty() {
        assert!(compact(vec![]).is_empty());
    }
}
