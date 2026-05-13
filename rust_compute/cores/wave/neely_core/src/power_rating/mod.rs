// power_rating — Stage 10a:Power Rating 查表
//
// 對齊 m3Spec/neely_core_architecture.md §9.2 / §11(power_rating 截斷哲學)
// + neely_rules.md Ch10 p.10-1~5(7 級完整查表)。
//
// 設計原則:
//   - PowerRating enum 取代 v1.1 i8(§9.2 防無效值)
//   - 方向中性語意:FavorContinuation/AgainstContinuation 取代 Bullish/Bearish(r5)
//   - 截斷哲學:Neely 規則邊界外的 case 截斷不外推(§十一 power_rating 截斷哲學論證)
//
// **PR-3c-pre 階段(2026-05-13)**:
//   - r2 Bullish/Bearish 命名改 FavorContinuation/AgainstContinuation
//   - r2 Diagonal{sub_kind} 改 TerminalImpulse
//   - rate_scenario 仍為 best-guess,完整 Ch10 查表留 PR-6b-1

use crate::output::{NeelyPatternType, PowerRating, Scenario};

pub mod table;

/// 對 Scenario 套 Power Rating(查表)。
///
/// **PR-3c-pre 階段** best-guess 邏輯(r5 §9.2 方向中性語意):
///   - Impulse 上漲(initial Up,符合既有趨勢)→ ModeratelyFavorContinuation
///   - Impulse 下跌(initial Down,逆既有趨勢)→ ModeratelyAgainstContinuation
///   - TerminalImpulse → SlightlyFavorContinuation / SlightlyAgainstContinuation
///     (Terminal pattern 終結 trend 但 power 較弱)
///   - Zigzag / Flat / Triangle / Combination / RunningCorrection → Neutral
///     (correction 不指向 trend continuation)
///
/// 注意:本實作為 best-guess,留 PR-6b-1 對齊 Neely Ch10 完整查表(7 級)。
pub fn rate_scenario(scenario: &Scenario) -> PowerRating {
    use crate::output::MonowaveDirection;
    let initial_dir = scenario
        .wave_tree
        .children
        .first()
        .map(|_| {
            if scenario.structure_label.contains("Up") {
                MonowaveDirection::Up
            } else if scenario.structure_label.contains("Down") {
                MonowaveDirection::Down
            } else {
                MonowaveDirection::Neutral
            }
        })
        .unwrap_or(MonowaveDirection::Neutral);

    match (&scenario.pattern_type, initial_dir) {
        (NeelyPatternType::Impulse, MonowaveDirection::Up) => PowerRating::ModeratelyFavorContinuation,
        (NeelyPatternType::Impulse, MonowaveDirection::Down) => PowerRating::ModeratelyAgainstContinuation,
        (NeelyPatternType::TerminalImpulse, MonowaveDirection::Up) => PowerRating::SlightlyFavorContinuation,
        (NeelyPatternType::TerminalImpulse, MonowaveDirection::Down) => PowerRating::SlightlyAgainstContinuation,
        _ => PowerRating::Neutral,
    }
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
        }
    }

    #[test]
    fn impulse_up_rates_favor_continuation() {
        let s = make_scenario(NeelyPatternType::Impulse, "Impulse 5-wave Up");
        assert!(matches!(rate_scenario(&s), PowerRating::ModeratelyFavorContinuation));
    }

    #[test]
    fn impulse_down_rates_against_continuation() {
        let s = make_scenario(NeelyPatternType::Impulse, "Impulse 5-wave Down");
        assert!(matches!(rate_scenario(&s), PowerRating::ModeratelyAgainstContinuation));
    }

    #[test]
    fn zigzag_rates_neutral() {
        let s = make_scenario(
            NeelyPatternType::Zigzag { sub_kind: ZigzagVariant::Normal },
            "Zigzag Up",
        );
        assert!(matches!(rate_scenario(&s), PowerRating::Neutral));
    }

    #[test]
    fn terminal_impulse_up_rates_slightly_favor() {
        let s = make_scenario(NeelyPatternType::TerminalImpulse, "TerminalImpulse 5-wave Up");
        assert!(matches!(rate_scenario(&s), PowerRating::SlightlyFavorContinuation));
    }

    #[test]
    fn apply_to_forest_mutates() {
        let mut forest = vec![
            make_scenario(NeelyPatternType::Impulse, "Up"),
            make_scenario(NeelyPatternType::Impulse, "Down"),
        ];
        apply_to_forest(&mut forest);
        assert!(matches!(forest[0].power_rating, PowerRating::ModeratelyFavorContinuation));
        assert!(matches!(forest[1].power_rating, PowerRating::ModeratelyAgainstContinuation));
    }
}
