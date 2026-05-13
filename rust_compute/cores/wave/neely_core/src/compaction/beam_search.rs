// beam_search.rs — Forest 上限保護的 Fallback
//
// 對齊 m2Spec/oldm2Spec/neely_core.md §十二(Forest 上限保護機制)。
//
// 觸發條件:
//   - exhaustive Compaction 後 forest.len() > cfg.forest_max_size
//   - cfg.overflow_strategy == OverflowStrategy::BeamSearchFallback { k }
//
// 演算法:
//   - 用 power_rating 做 ranking key
//   - 保留兩端極值(StrongBullish / StrongBearish)優先,Neutral 次之
//   - 達到 k 個 scenario 即停
//
// **M3 PR-5 階段**:用 |power_rating| 排序保留 top-K(對齊 §12 文字描述)。
// 排序 tie-break:同 |power_rating| 時保留 Bullish 側(任意決定,不影響功能)。
//
// k 預設 100(NeelyEngineConfig.OverflowStrategy::BeamSearchFallback default)。
// P0 Gate 五檔實測後可能調整。

use super::power_rating_magnitude;
use crate::output::Scenario;
use std::cmp::Ordering;

/// 保留 top-K by |power_rating|(兩端極值優先),Neutral 後保留。
pub fn keep_top_k_by_power_rating(mut scenarios: Vec<Scenario>, k: usize) -> Vec<Scenario> {
    if scenarios.len() <= k {
        return scenarios;
    }

    // 排序:|power_rating| 降序;同 |rating| 時 Bullish 優先(magnitude > 0 排前)
    scenarios.sort_by(|a, b| {
        let ma = power_rating_magnitude(a.power_rating);
        let mb = power_rating_magnitude(b.power_rating);
        let abs_a = ma.abs();
        let abs_b = mb.abs();
        match abs_b.cmp(&abs_a) {
            Ordering::Equal => mb.cmp(&ma), // 同 |rating| 時 Bullish(正)排前
            other => other,
        }
    });

    scenarios.truncate(k);
    scenarios
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::*;
    use chrono::NaiveDate;

    fn make(id: &str, rating: PowerRating) -> Scenario {
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
            power_rating: rating,
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
    fn under_k_pass_through() {
        let scenarios = vec![make("a", PowerRating::Bullish)];
        let kept = keep_top_k_by_power_rating(scenarios, 5);
        assert_eq!(kept.len(), 1);
    }

    #[test]
    fn keeps_extreme_ratings_first() {
        let scenarios = vec![
            make("neut", PowerRating::Neutral),
            make("strong_bull", PowerRating::StrongBullish),
            make("slight_bear", PowerRating::SlightBearish),
            make("strong_bear", PowerRating::StrongBearish),
            make("slight_bull", PowerRating::SlightBullish),
        ];
        let kept = keep_top_k_by_power_rating(scenarios, 2);
        assert_eq!(kept.len(), 2);
        // 兩端極值優先:StrongBullish + StrongBearish 留下
        let ids: Vec<&str> = kept.iter().map(|s| s.id.as_str()).collect();
        assert!(ids.contains(&"strong_bull") && ids.contains(&"strong_bear"));
    }

    #[test]
    fn equal_magnitude_prefers_bullish() {
        let scenarios = vec![
            make("bear", PowerRating::Bearish),
            make("bull", PowerRating::Bullish),
        ];
        let kept = keep_top_k_by_power_rating(scenarios, 1);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].id, "bull", "同 |rating| 時 Bullish 優先");
    }
}
