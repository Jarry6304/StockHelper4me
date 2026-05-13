// post_validator — Stage 6:Post-Constructive Validator
//
// 對齊 m2Spec/oldm2Spec/neely_core.md §三 / §七 Stage 6。
// Neely 「型態完成必要條件」(Item 7B)。
//
// **M3 PR-4 階段**(先實踐以後再改):
//   - Skeleton + 基本 wave_count 完整度檢查
//   - 規則細節 spec §10.1 寫「P0 開發時逐條建檔」沒列,留 PR-4b 校準
//
// 設計原則:
//   - Stage 4 Validator 是「規則檢查」,Post-Validator 是「型態完成必要條件」
//   - 例:Triangle 必須 5 個 sub-wave 完成才算成立
//   - 不通過 → Scenario 標 deferred(等更多 K 棒驗證)

use crate::output::Scenario;

#[derive(Debug, Clone)]
pub struct PostValidationReport {
    pub scenario_id: String,
    /// 型態完成度判定(本 PR-4 階段預設 true,Item 7B 細節留 PR-4b)
    pub pattern_complete: bool,
    /// 待驗證的後續條件(留 §10.3 deferred 機制)
    pub pending_conditions: Vec<String>,
}

/// 對單一 Scenario 跑 Post-Constructive Validator。
///
/// **M3 PR-4 階段**:預設所有 Scenario 視為 pattern_complete = true,
/// pending_conditions 空 list。Item 7B 具體規則留 PR-4b。
pub fn post_validate(scenario: &Scenario) -> PostValidationReport {
    PostValidationReport {
        scenario_id: scenario.id.clone(),
        pattern_complete: true,
        pending_conditions: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::*;
    use chrono::NaiveDate;

    fn make_minimal_scenario() -> Scenario {
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
            post_pattern_behavior: PostBehavior::Unconstrained,
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
    fn skeleton_post_validate_passes_by_default() {
        let scenario = make_minimal_scenario();
        let report = post_validate(&scenario);
        assert!(report.pattern_complete);
        assert!(report.pending_conditions.is_empty());
    }
}
