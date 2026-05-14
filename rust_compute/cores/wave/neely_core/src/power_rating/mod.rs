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

pub mod max_retracement;
pub mod post_behavior;
pub mod table;

/// 對 Scenario 套 Power Rating(查表)。
///
/// Phase 8 邏輯(更新):
///   - 由 scenario.pattern_type + scenario.initial_direction 查 table::lookup_power_rating
///   - **in_triangle 來自 scenario.in_triangle_context**(Stage 8 Three Rounds nested context 後填)
///   - 對齊 spec §Ch10 2021 行:三角內任一規則例外 → power = 0
pub fn rate_scenario(scenario: &Scenario) -> PowerRating {
    table::lookup_power_rating(
        &scenario.pattern_type,
        scenario.initial_direction,
        scenario.in_triangle_context,
    )
}

/// 對 Forest 套 Power Rating + Max Retracement + PostBehavior,
/// 直接更新每 Scenario 的 `power_rating` + `max_retracement` + `post_pattern_behavior` 欄位。
///
/// 對齊 m3Spec/neely_rules.md §第 10 章 2016-2022 行(Power Rating × 回測限制聯動表)
/// + 2024-2037 行(各修正暗示重點 = PostBehavior dispatch)
/// + m3Spec/neely_core_architecture.md §11.4(Triangle/Terminal 內部覆蓋規則)。
///
/// 三項 lookup 共用 `in_triangle_context` 覆蓋規則(spec line 2021):
/// in_triangle → max_retracement = None / post_pattern_behavior = Unconstrained
/// (power_rating 自身的 in_triangle 覆蓋已在 table::lookup_power_rating 處理)。
pub fn apply_to_forest(forest: &mut [Scenario]) {
    for scenario in forest.iter_mut() {
        scenario.power_rating = rate_scenario(scenario);
        scenario.max_retracement =
            max_retracement::lookup(scenario.power_rating, scenario.in_triangle_context);
        scenario.post_pattern_behavior =
            post_behavior::lookup(&scenario.pattern_type, scenario.in_triangle_context);
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
            max_retracement: None,
            post_pattern_behavior: PostBehavior::Unconstrained,
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
            monowave_structure_labels: Vec::new(),
            round_state: RoundState::Round1,
            pattern_isolation_anchors: Vec::new(),
            triplexity_detected: false,
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

    #[test]
    fn apply_to_forest_fills_max_retracement_from_rating() {
        // Strong (±3) → 0.65 / Slight (±1) → 0.90 / Neutral → None
        let mut forest = vec![
            make_scenario(NeelyPatternType::Impulse, MonowaveDirection::Up), // → Strong
            make_scenario(
                NeelyPatternType::Diagonal { sub_kind: DiagonalKind::Leading },
                MonowaveDirection::Up,
            ), // → Slight
            make_scenario(
                NeelyPatternType::Zigzag { sub_kind: ZigzagKind::Single },
                MonowaveDirection::Up,
            ), // → Neutral
        ];
        apply_to_forest(&mut forest);
        assert_eq!(forest[0].max_retracement, Some(0.65));
        assert_eq!(forest[1].max_retracement, Some(0.90));
        assert_eq!(forest[2].max_retracement, None);
    }

    #[test]
    fn apply_to_forest_in_triangle_context_overrides_to_none() {
        let mut forest = vec![make_scenario(NeelyPatternType::Impulse, MonowaveDirection::Up)];
        forest[0].in_triangle_context = true;
        apply_to_forest(&mut forest);
        // Triangle override: max_retracement → None regardless of underlying rating
        assert_eq!(forest[0].max_retracement, None);
    }

    #[test]
    fn apply_to_forest_fills_post_behavior() {
        // Phase 14: PostBehavior 8-variant lookup 同步寫入
        let mut forest = vec![
            make_scenario(NeelyPatternType::Impulse, MonowaveDirection::Up), // → NotFullyRetracedUnless
            make_scenario(
                NeelyPatternType::Zigzag { sub_kind: ZigzagKind::Single },
                MonowaveDirection::Up,
            ), // → Unconstrained
            make_scenario(
                NeelyPatternType::Diagonal { sub_kind: DiagonalKind::Ending },
                MonowaveDirection::Up,
            ), // → FullRetracementRequired
        ];
        apply_to_forest(&mut forest);
        assert!(matches!(
            forest[0].post_pattern_behavior,
            PostBehavior::NotFullyRetracedUnless { .. }
        ));
        assert!(matches!(forest[1].post_pattern_behavior, PostBehavior::Unconstrained));
        assert!(matches!(
            forest[2].post_pattern_behavior,
            PostBehavior::FullRetracementRequired
        ));
    }

    #[test]
    fn apply_to_forest_in_triangle_overrides_post_behavior_to_unconstrained() {
        let mut forest = vec![make_scenario(NeelyPatternType::Impulse, MonowaveDirection::Up)];
        forest[0].in_triangle_context = true;
        apply_to_forest(&mut forest);
        assert!(matches!(forest[0].post_pattern_behavior, PostBehavior::Unconstrained));
    }
}
