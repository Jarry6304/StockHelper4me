// power_rating — Stage 10a:Power Rating 查表
//
// 對齊 m3Spec/neely_rules.md §Pattern Implications & Power Ratings(2004-2022 行)
//       + m3Spec/neely_core_architecture.md §9.4 / §11
//
// **Phase 5 PR**(r5 alignment):
//   - rate_scenario() 改用 Scenario.initial_direction(Phase 5 PR 新增 field),
//     不再用 structure_label 字串 parsing(原 best-guess 方案)
//   - 完整 r5 Power Rating 表已在 table.rs 落地(spec 2006-2014 行 7 級)
//   - in_triangle 例外:Phase 5 預設 false,留 P6/P8 Compaction 補 nested context

use crate::output::{PowerRating, Scenario};

pub mod table;

/// 對 Scenario 套 Power Rating(查表)。
///
/// Phase 5 邏輯:
///   - 由 scenario.pattern_type + scenario.initial_direction 查 table::lookup_power_rating
///   - in_triangle 預設 false(無 parent context — 留 P6/P8 Compaction)
pub fn rate_scenario(scenario: &Scenario) -> PowerRating {
    // in_triangle 由 Stage 8 Compaction 提供 parent scenario context 後填;
    // Phase 5 暫無 parent scenario chain → 一律 false
    let in_triangle = false;
    table::lookup_power_rating(&scenario.pattern_type, scenario.initial_direction, in_triangle)
}

/// 對 Forest 套 Power Rating,直接更新每 Scenario 的 power_rating 欄位。
pub fn apply_to_forest(forest: &mut [Scenario]) {
    for scenario in forest.iter_mut() {
        scenario.power_rating = rate_scenario(scenario);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::*;
    use chrono::NaiveDate;

    fn make_scenario(
        pattern: NeelyPatternType,
        direction: MonowaveDirection,
    ) -> Scenario {
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
            initial_direction: direction,
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
        }
    }

    #[test]
    fn impulse_up_rates_strong_bullish() {
        let s = make_scenario(NeelyPatternType::Impulse, MonowaveDirection::Up);
        assert!(matches!(rate_scenario(&s), PowerRating::StrongBullish));
    }

    #[test]
    fn impulse_down_rates_strong_bearish() {
        let s = make_scenario(NeelyPatternType::Impulse, MonowaveDirection::Down);
        assert!(matches!(rate_scenario(&s), PowerRating::StrongBearish));
    }

    #[test]
    fn zigzag_single_up_rates_neutral() {
        let s = make_scenario(
            NeelyPatternType::Zigzag {
                sub_kind: ZigzagKind::Single,
            },
            MonowaveDirection::Up,
        );
        assert!(matches!(rate_scenario(&s), PowerRating::Neutral));
    }

    #[test]
    fn zigzag_double_up_rates_bullish() {
        let s = make_scenario(
            NeelyPatternType::Zigzag {
                sub_kind: ZigzagKind::Double,
            },
            MonowaveDirection::Up,
        );
        assert!(matches!(rate_scenario(&s), PowerRating::Bullish));
    }

    #[test]
    fn diagonal_leading_up_slight_bullish() {
        let s = make_scenario(
            NeelyPatternType::Diagonal {
                sub_kind: DiagonalKind::Leading,
            },
            MonowaveDirection::Up,
        );
        assert!(matches!(rate_scenario(&s), PowerRating::SlightBullish));
    }

    #[test]
    fn diagonal_ending_up_slight_bearish() {
        let s = make_scenario(
            NeelyPatternType::Diagonal {
                sub_kind: DiagonalKind::Ending,
            },
            MonowaveDirection::Up,
        );
        assert!(matches!(rate_scenario(&s), PowerRating::SlightBearish));
    }

    #[test]
    fn apply_to_forest_mutates_all_scenarios() {
        let mut forest = vec![
            make_scenario(NeelyPatternType::Impulse, MonowaveDirection::Up),
            make_scenario(NeelyPatternType::Impulse, MonowaveDirection::Down),
        ];
        apply_to_forest(&mut forest);
        assert!(matches!(forest[0].power_rating, PowerRating::StrongBullish));
        assert!(matches!(forest[1].power_rating, PowerRating::StrongBearish));
    }
}
