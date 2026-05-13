// missing_wave — Stage 9a:Missing Wave 偵測
//
// 對齊 m2Spec/oldm2Spec/neely_core.md §三 / §七 Stage 9。
// Neely 原書 Item 9 — 對 Forest 中各 Scenario 偵測缺失波浪。
//
// **M3 PR-6 階段**(先實踐以後再改):
//   - skeleton:標 each scenario 是否「可能 missing wave」
//   - 預設都標 false(留 PR-6b 校準 Item 9 細節)

use crate::output::Scenario;

/// 對單一 Scenario 偵測 missing wave。
///
/// **M3 PR-6 階段**:預設 false。Item 9 規則細節留 PR-6b。
pub fn detect_missing_wave(_scenario: &Scenario) -> bool {
    false
}

/// 對 Forest 中所有 scenario 套 missing_wave 偵測。
/// 不刪除 scenario,只標記到 structural_facts(留 PR-6b 補)。
pub fn apply_to_forest(forest: &[Scenario]) -> Vec<bool> {
    forest.iter().map(detect_missing_wave).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::*;
    use chrono::NaiveDate;

    fn make_scenario() -> Scenario {
        let date = NaiveDate::parse_from_str("2026-01-01", "%Y-%m-%d").unwrap();
        Scenario {
            id: "test".to_string(),
            wave_tree: WaveNode {
                label: "test".to_string(),
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
        }
    }

    #[test]
    fn skeleton_returns_false() {
        assert!(!detect_missing_wave(&make_scenario()));
    }

    #[test]
    fn apply_to_empty_forest() {
        assert_eq!(apply_to_forest(&[]).len(), 0);
    }
}
