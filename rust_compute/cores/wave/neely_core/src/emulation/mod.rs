// emulation — Stage 9b:Emulation 辨識
//
// 對齊 m2Spec/oldm2Spec/neely_core.md §三 / §七 Stage 9。
// Neely 原書 Item 10 — 對 Forest 中各 Scenario 辨識 emulation 行為。
//
// **M3 PR-6 階段**(先實踐以後再改):
//   - skeleton:標 each scenario 是否「emulation」(模仿其他 pattern)
//   - 預設都標 false(留 PR-6b 校準 Item 10 細節)

use crate::output::Scenario;

/// 對單一 Scenario 偵測 emulation 行為。
///
/// **M3 PR-6 階段**:預設 false。Item 10 規則細節留 PR-6b。
pub fn detect_emulation(_scenario: &Scenario) -> bool {
    false
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
        }
    }

    #[test]
    fn skeleton_returns_false() {
        assert!(!detect_emulation(&make_scenario()));
    }
}
