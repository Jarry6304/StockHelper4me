// power_rating — Stage 10a:Power Rating 7 級查表(PR-6b-1 完整實作)
//
// 對齊 m3Spec/neely_core_architecture.md §9.2 / §11(截斷哲學)
// + neely_rules.md Ch10 p.10-1~5(7 級完整查表 + 回測限制 + Triangle/Terminal override)。
//
// 設計原則:
//   - PowerRating enum 取代 v1.1 i8(§9.2 防無效值)
//   - 方向中性語意:FavorContinuation/AgainstContinuation(r5)
//   - 截斷哲學:不外推到 spec 邊界外 case(§十一)
//
// **PR-6b-1 階段(2026-05-13)**:
//   - 7 級完整查表 對映 NeelyPatternType + sub_kind
//   - direction-aware:Up / Down 趨勢翻轉符號(line 2006「向下則反號」)
//   - Triangle / Terminal 內部 override(暫不能 traverse parent context;
//     單一 Scenario 自身是 Triangle/Terminal → Neutral)
//   - max_retracement 寫入 Scenario(0/±1/±2/±3 → 1.0/0.90/0.80/0.65)

use crate::output::{MonowaveDirection, Scenario};

pub mod table;

/// 從 wave_tree 推 initial_direction(對應 r5 §9.2 方向決定 sign)
fn infer_direction(scenario: &Scenario) -> MonowaveDirection {
    // 從 wave_tree.start vs wave_tree.children[0].end 推趨勢方向
    // wave_tree 沒帶 price,改從 structure_label 含 "Up"/"Down" 推
    if scenario.structure_label.contains("Up") {
        MonowaveDirection::Up
    } else if scenario.structure_label.contains("Down") {
        MonowaveDirection::Down
    } else {
        MonowaveDirection::Neutral
    }
}

/// 對 Scenario 套 Power Rating(查表)+ 寫 max_retracement。
pub fn rate_scenario(scenario: &Scenario) -> (crate::output::PowerRating, f64) {
    let direction = infer_direction(scenario);
    let rating = table::lookup_power_rating(&scenario.pattern_type, direction);
    let max_retrace = table::max_retracement_for(rating);
    (rating, max_retrace)
}

/// 對 Forest 套 Power Rating + max_retracement,直接更新各 Scenario 的對應欄位。
pub fn apply_to_forest(forest: &mut [Scenario]) {
    for scenario in forest.iter_mut() {
        let (rating, max_retrace) = rate_scenario(scenario);
        scenario.power_rating = rating;
        scenario.max_retracement = max_retrace;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::*;
    use chrono::NaiveDate;

    fn make_scenario(pattern: NeelyPatternType, label: &str) -> Scenario {
        let date = NaiveDate::parse_from_str("2026-01-01", "%Y-%m-%d").unwrap();
        Scenario {
            id: "test".to_string(),
            wave_tree: WaveNode {
                label: "test".to_string(),
                start: date,
                end: date,
                children: vec![WaveNode {
                    label: "W1".to_string(),
                    start: date,
                    end: date,
                    children: Vec::new(),
                }],
            },
            pattern_type: pattern,
            structure_label: label.to_string(),
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
    fn impulse_rates_neutral_per_ch10() {
        // Ch10:Impulse 本身 = 0(實際 power 由後續延伸決定)
        let s = make_scenario(NeelyPatternType::Impulse, "Impulse 5-wave Up");
        let (rating, max_r) = rate_scenario(&s);
        assert!(matches!(rating, PowerRating::Neutral));
        assert_eq!(max_r, 1.0);
    }

    #[test]
    fn running_correction_up_strongly_against() {
        let s = make_scenario(NeelyPatternType::RunningCorrection, "RunningCorrection Up");
        let (rating, max_r) = rate_scenario(&s);
        assert!(matches!(rating, PowerRating::StronglyAgainstContinuation));
        assert_eq!(max_r, 0.65);
    }

    #[test]
    fn running_correction_down_flips_to_strongly_favor() {
        let s = make_scenario(NeelyPatternType::RunningCorrection, "RunningCorrection Down");
        let (rating, _) = rate_scenario(&s);
        assert!(matches!(rating, PowerRating::StronglyFavorContinuation));
    }

    #[test]
    fn flat_irregular_failure_moderately_against() {
        let s = make_scenario(
            NeelyPatternType::Flat { sub_kind: FlatVariant::IrregularFailure },
            "Flat Up",
        );
        let (rating, max_r) = rate_scenario(&s);
        assert!(matches!(rating, PowerRating::ModeratelyAgainstContinuation));
        assert_eq!(max_r, 0.80);
    }

    #[test]
    fn triangle_internal_neutral() {
        let s = make_scenario(
            NeelyPatternType::Triangle { sub_kind: TriangleVariant::HorizontalLimiting },
            "Triangle Up",
        );
        let (rating, _) = rate_scenario(&s);
        assert!(matches!(rating, PowerRating::Neutral));
    }

    #[test]
    fn apply_to_forest_writes_max_retracement() {
        let mut forest = vec![
            make_scenario(NeelyPatternType::RunningCorrection, "RC Up"),
            make_scenario(
                NeelyPatternType::Flat { sub_kind: FlatVariant::CFailure },
                "Flat Up",
            ),
        ];
        apply_to_forest(&mut forest);
        assert_eq!(forest[0].max_retracement, 0.65);   // ±3 → 0.65
        assert_eq!(forest[1].max_retracement, 0.90);   // ±1 → 0.90
        assert!(matches!(forest[0].power_rating, PowerRating::StronglyAgainstContinuation));
        assert!(matches!(forest[1].power_rating, PowerRating::SlightlyAgainstContinuation));
    }
}
