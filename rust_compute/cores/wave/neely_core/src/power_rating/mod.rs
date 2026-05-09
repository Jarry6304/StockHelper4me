// power_rating — Stage 10a:Power Rating 查表
//
// 對齊 m2Spec/oldm2Spec/neely_core.md §三 / §七 Stage 10 / §十三。
// 子模組:
//   - table.rs — Neely 書裡的 power rating 表(寫死,不可外部化 §6.6)
//
// 設計原則:
//   - PowerRating enum 取代 v1.1 i8(§9.4 防無效值)
//   - 截斷哲學:Neely 規則邊界外的 case 截斷不外推(§十三 power_rating 截斷哲學論證)
//
// **M3 PR-6 階段**(先實踐以後再改):
//   - rate_scenario():對 Scenario 套 best-guess Power Rating(基於 pattern_type +
//     initial direction)
//   - Neely 書頁完整查表留 PR-6b

use crate::output::{NeelyPatternType, PowerRating, Scenario};

pub mod table;

/// 對 Scenario 套 Power Rating(查表)。
///
/// **M3 PR-6 階段** best-guess 邏輯:
///   - Impulse 上漲(initial Up)→ Bullish
///   - Impulse 下跌(initial Down)→ Bearish
///   - Diagonal → SlightBullish / SlightBearish(終結 / 開啟 trend 但 power 較弱)
///   - Zigzag / Flat / Triangle / Combination → Neutral(correction 不指向 trend)
///
/// 注意:本實作為 best-guess,留 PR-6b 對齊 Neely 書頁完整查表。
pub fn rate_scenario(scenario: &Scenario) -> PowerRating {
    use crate::output::MonowaveDirection;
    let initial_dir = scenario
        .wave_tree
        .children
        .first()
        .map(|_| {
            // wave_tree 的 children[0] 是 W1,但 WaveNode 沒帶 direction;
            // 改從 wave_tree.start vs wave_tree.children[0].end 推
            // 簡化版:從 structure_label 含 "Up" 或從 wave_tree label 推
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
        (NeelyPatternType::Impulse, MonowaveDirection::Up) => PowerRating::Bullish,
        (NeelyPatternType::Impulse, MonowaveDirection::Down) => PowerRating::Bearish,
        (NeelyPatternType::Diagonal { .. }, MonowaveDirection::Up) => PowerRating::SlightBullish,
        (NeelyPatternType::Diagonal { .. }, MonowaveDirection::Down) => PowerRating::SlightBearish,
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
    fn impulse_up_rates_bullish() {
        let s = make_scenario(NeelyPatternType::Impulse, "Impulse 5-wave Up");
        assert!(matches!(rate_scenario(&s), PowerRating::Bullish));
    }

    #[test]
    fn impulse_down_rates_bearish() {
        let s = make_scenario(NeelyPatternType::Impulse, "Impulse 5-wave Down");
        assert!(matches!(rate_scenario(&s), PowerRating::Bearish));
    }

    #[test]
    fn zigzag_rates_neutral() {
        let s = make_scenario(
            NeelyPatternType::Zigzag {
                sub_kind: ZigzagKind::Single,
            },
            "Zigzag Up",
        );
        assert!(matches!(rate_scenario(&s), PowerRating::Neutral));
    }

    #[test]
    fn diagonal_up_rates_slight_bullish() {
        let s = make_scenario(
            NeelyPatternType::Diagonal {
                sub_kind: DiagonalKind::Leading,
            },
            "Diagonal 5-wave Up",
        );
        assert!(matches!(rate_scenario(&s), PowerRating::SlightBullish));
    }

    #[test]
    fn apply_to_forest_mutates() {
        let mut forest = vec![
            make_scenario(NeelyPatternType::Impulse, "Up"),
            make_scenario(NeelyPatternType::Impulse, "Down"),
        ];
        apply_to_forest(&mut forest);
        assert!(matches!(forest[0].power_rating, PowerRating::Bullish));
        assert!(matches!(forest[1].power_rating, PowerRating::Bearish));
    }
}
