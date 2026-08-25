// beam_search.rs — Forest 上限保護的 Fallback
//
// 對齊 m3Spec/neely_core_architecture.md §十二(Forest 上限保護機制)。
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
// 雙重排序(architecture §10.3):
//   鍵 1:PowerRating 級別 |rating|(±3 > ±2 > ±1 > 0)— 強訊號不被弱訊號擠掉
//   鍵 2(組內):rules_passed_count — 同級別內通過規則多者優先
//   tie-break:同組同 count 時保留 Bullish 側(任意決定,不影響功能)
//
// **G2.0(compaction v2 §8.3 / §8.4 T-5)**:鍵 2 補實 — lib.rs 自 Phase 8 起宣稱
// 雙重排序,實作長期只有鍵 1;P3 讓 Level-N 帶 Σ(children) 計數後鍵 2 才有意義,
// 一併補上,Level-N 不再因 count=0 在組內墊底。
//
// k 預設 100(NeelyEngineConfig.OverflowStrategy::BeamSearchFallback default)。
// P0 Gate 五檔實測後可能調整。

use super::power_rating_magnitude;
use crate::output::Scenario;
use std::cmp::Ordering;

/// 雙重排序保留 top-K:|power_rating| 級別 → 組內 rules_passed_count(architecture §10.3)。
pub fn keep_top_k_by_power_rating(mut scenarios: Vec<Scenario>, k: usize) -> Vec<Scenario> {
    if scenarios.len() <= k {
        return scenarios;
    }

    scenarios.sort_by(|a, b| {
        let ma = power_rating_magnitude(a.power_rating);
        let mb = power_rating_magnitude(b.power_rating);
        match mb.abs().cmp(&ma.abs()) {
            Ordering::Equal => match b.rules_passed_count.cmp(&a.rules_passed_count) {
                Ordering::Equal => mb.cmp(&ma), // 同組同 count 時 Bullish(正)排前
                other => other,
            },
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

    #[test]
    fn t5_level_n_with_summed_rules_not_ranked_bottom() {
        // G2.0 T-5(compaction v2 §8.4):同 PowerRating 組內,Level-1(P3 後帶
        // Σ(children) 計數)不因舊 count=0 墊底 — 雙重排序鍵 2 淘汰的是組內
        // 通過規則最少的 Level-0
        let mut level_1 = make("level_1_aggregated", PowerRating::Neutral);
        level_1.rules_passed_count = 12; // Σ(children):P3 暫填語意
        let mut l0_a = make("l0_a", PowerRating::Neutral);
        l0_a.rules_passed_count = 5;
        let mut l0_b = make("l0_b", PowerRating::Neutral);
        l0_b.rules_passed_count = 1;
        let mut l0_c = make("l0_c", PowerRating::Neutral);
        l0_c.rules_passed_count = 3;

        let kept = keep_top_k_by_power_rating(vec![l0_a, l0_b, level_1, l0_c], 3);
        let ids: Vec<&str> = kept.iter().map(|s| s.id.as_str()).collect();
        assert!(
            ids.contains(&"level_1_aggregated"),
            "Level-1 帶 Σrules=12 不得被淘汰"
        );
        assert!(!ids.contains(&"l0_b"), "組內 count 最低(1)者被淘汰");
        assert_eq!(kept[0].id, "level_1_aggregated", "組內 count 最高者排前");
    }
}
