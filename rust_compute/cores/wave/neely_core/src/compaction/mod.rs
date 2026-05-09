// compaction — Stage 8:Compaction(窮舉 Forest)+ Forest 上限保護
//
// 對齊 m2Spec/oldm2Spec/neely_core.md §三 / §七 Stage 8 / §十一 / §十二。
//
// 子模組:
//   - exhaustive.rs   — 窮舉模式(預設,留 PR-5b 完整實作)
//   - beam_search.rs  — Forest 上限保護的 fallback(§12)
//
// 關鍵設計:
//   - 純結構壓縮,**不**選最優,不附 primary(§9.3)
//   - 重寫 v1.1 的「貪心選分數」(§4.2)— 多種解讀路徑窮舉成 Forest
//
// **M3 PR-5 階段**(先實踐以後再改):
//   - 簡化版 Compaction:每個通過 Stage 5-7 的 Scenario 直接成 Forest 一棵樹
//     (尚未做「合法 compression paths 窮舉」— 那需要 sub-wave 嵌套結構,留 PR-5b)
//   - Forest 上限保護完整實作:超過 forest_max_size → BeamSearchFallback(top-K by power_rating)
//   - 逾時保護:elapsed > compaction_timeout_secs → 直接返回現有 forest + 標 compaction_timeout
//
// 留後續 PR(對齊 §十一):
//   - exhaustive.rs:窮舉所有合法 compression paths(需要 sub-wave 嵌套結構)
//   - beam_search.rs 進階:k 值 P0 Gate 校準

use crate::config::{NeelyEngineConfig, OverflowStrategy};
use crate::output::{PowerRating, Scenario};
use std::time::Instant;

pub mod beam_search;
pub mod exhaustive;

/// Compaction 結果。
#[derive(Debug, Clone, Default)]
pub struct CompactionResult {
    /// 最終 Forest(對齊 §9.3,順序不反映優先級)
    pub forest: Vec<Scenario>,
    /// 是否觸發 BeamSearchFallback(forest size 超過 max_size)
    pub overflow_triggered: bool,
    /// Compaction 是否逾時(超過 compaction_timeout_secs)
    pub timeout_triggered: bool,
    /// 本階段窮舉的合法 compression paths 數(M3 PR-5 簡化版 = scenarios.len())
    pub compaction_paths: usize,
}

/// Stage 8 主入口。
///
/// 流程:
///   1. exhaustive::compact() 跑窮舉壓縮(M3 PR-5 簡化版直接 pass-through)
///   2. 檢查 forest size 是否超過 cfg.forest_max_size
///   3. 超過 → 套 cfg.overflow_strategy(BeamSearchFallback / Unbounded)
///   4. 同時檢查 compaction_timeout_secs
pub fn compact(scenarios: Vec<Scenario>, cfg: &NeelyEngineConfig) -> CompactionResult {
    let start = Instant::now();
    let timeout_duration = std::time::Duration::from_secs(cfg.compaction_timeout_secs);

    // ── Step 1:exhaustive 窮舉(目前簡化 pass-through)
    let initial_forest = exhaustive::compact(scenarios);
    let initial_count = initial_forest.len();

    // ── Step 2-3:Forest 上限保護
    let mut overflow_triggered = false;
    let final_forest = if initial_count > cfg.forest_max_size {
        match cfg.overflow_strategy {
            OverflowStrategy::BeamSearchFallback { k } => {
                overflow_triggered = true;
                beam_search::keep_top_k_by_power_rating(initial_forest, k)
            }
            OverflowStrategy::Unbounded => {
                // P0 Gate 校準階段使用,不剪枝
                initial_forest
            }
        }
    } else {
        initial_forest
    };

    // ── Step 4:逾時檢查(本階段已跑完,只是紀錄)
    let timeout_triggered = start.elapsed() > timeout_duration;

    CompactionResult {
        forest: final_forest,
        overflow_triggered,
        timeout_triggered,
        compaction_paths: initial_count,
    }
}

/// PowerRating 排序(對齊 §9.1 enum):StrongBullish > Bullish > SlightBullish >
/// Neutral > SlightBearish > Bearish > StrongBearish。BeamSearch 用「magnitude」
/// 排序(|rating - Neutral|),保留兩端極值;同 magnitude 時保留 Bullish 側。
pub(crate) fn power_rating_magnitude(p: PowerRating) -> i32 {
    match p {
        PowerRating::StrongBullish => 3,
        PowerRating::Bullish => 2,
        PowerRating::SlightBullish => 1,
        PowerRating::Neutral => 0,
        PowerRating::SlightBearish => -1,
        PowerRating::Bearish => -2,
        PowerRating::StrongBearish => -3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::OverflowStrategy;
    use crate::output::*;
    use chrono::NaiveDate;

    fn make_scenario(id: &str, rating: PowerRating) -> Scenario {
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
        }
    }

    #[test]
    fn small_forest_passes_through_unchanged() {
        let cfg = NeelyEngineConfig::default(); // forest_max_size 1000
        let scenarios = vec![
            make_scenario("a", PowerRating::Bullish),
            make_scenario("b", PowerRating::Neutral),
        ];
        let result = compact(scenarios, &cfg);
        assert_eq!(result.forest.len(), 2);
        assert!(!result.overflow_triggered);
        assert_eq!(result.compaction_paths, 2);
    }

    #[test]
    fn forest_overflow_triggers_beam_search_fallback() {
        let cfg = NeelyEngineConfig {
            forest_max_size: 3,
            overflow_strategy: OverflowStrategy::BeamSearchFallback { k: 2 },
            ..NeelyEngineConfig::default()
        };
        let scenarios = vec![
            make_scenario("a", PowerRating::Neutral),
            make_scenario("b", PowerRating::StrongBullish),
            make_scenario("c", PowerRating::SlightBearish),
            make_scenario("d", PowerRating::Bearish),
            make_scenario("e", PowerRating::Neutral),
        ];
        let result = compact(scenarios, &cfg);
        assert!(result.overflow_triggered);
        assert_eq!(result.forest.len(), 2);
        assert_eq!(result.compaction_paths, 5);
    }

    #[test]
    fn forest_overflow_unbounded_keeps_all() {
        let cfg = NeelyEngineConfig {
            forest_max_size: 1,
            overflow_strategy: OverflowStrategy::Unbounded,
            ..NeelyEngineConfig::default()
        };
        let scenarios = vec![
            make_scenario("a", PowerRating::Bullish),
            make_scenario("b", PowerRating::Bearish),
        ];
        let result = compact(scenarios, &cfg);
        assert!(!result.overflow_triggered, "Unbounded 不應 trigger overflow");
        assert_eq!(result.forest.len(), 2);
    }

    #[test]
    fn power_rating_magnitude_ordering() {
        assert_eq!(power_rating_magnitude(PowerRating::StrongBullish), 3);
        assert_eq!(power_rating_magnitude(PowerRating::Neutral), 0);
        assert_eq!(power_rating_magnitude(PowerRating::StrongBearish), -3);
    }
}
