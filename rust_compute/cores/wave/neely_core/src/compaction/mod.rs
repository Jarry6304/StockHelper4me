// compaction — Stage 8:Compaction(遞迴 aggregation Forest)+ Forest 上限保護
//
// 對齊 m3Spec/neely_core_architecture.md §三 / §七 Stage 8 / §十一 / §十二
//     + m3Spec/neely_compaction_v2.md(v2 規格;§1.2 缺陷表 D-1 ~ D-6)。
//
// 子模組:
//   - exhaustive.rs   — v3.7 遞迴迴圈:逐 level 呼叫 three_rounds::aggregate_one_level
//   - three_rounds.rs — Round 1-2 一層 aggregation(label 序列 + 方向交替 + S&B 弱比對)
//   - round_engine.rs — **Compaction v2 tiling-round 引擎**(G2.1 shadow 雙軌,
//                       只寫 diagnostics;Gate v3 過後取代 exhaustive/three_rounds)
//   - beam_search.rs  — Forest 上限保護的 fallback(§12;雙重排序 §10.3)
//
// 關鍵設計:
//   - 純結構壓縮,**不**選最優,不附 primary(§9.3)
//   - 重寫 v1.1 的「貪心選分數」(§4.2)— 多種解讀路徑窮舉成 Forest
//
// **G2.0 止血後現況(compaction v2 §8,2026-08-25)**:
//   - D-1/D-2(對重疊 forest 滑窗、無相鄰性檢查)→ P1 相鄰性硬檢查止血:
//     枚舉不完整但產出皆合法,不再拼出時間重疊/有間隙的 Level-N 結構
//   - D-3(聚合 scenario 規則欄全空)→ P3 暫填 Σ(children.rules_passed_count),
//     真值重驗留 G2.2(W5 端點泛化)
//   - D-4(邊界波以視窗內近似)/ D-5(Terminal Impulse 產不出)未修
//   - exhaustive.rs / three_rounds.rs 兩檔為 v2 規格 §3.3 的取代對象:
//     tiling-round 引擎(G2.1+)落地並過 P0 Gate v3 後刪除,在此之前不再演進
//     聚合邏輯;beam_search.rs 護欄保留(v2 §7.1 / architecture §10 原樣)
//
// Forest 上限保護 / 逾時保護(不隨 v2 改動,architecture §10):
//   - 超過 forest_max_size → BeamSearchFallback(雙重排序 top-K)
//   - elapsed > compaction_timeout_secs → 返回現有 forest + 標 compaction_timeout

use crate::config::{NeelyEngineConfig, OverflowStrategy};
use crate::output::{Monowave, PowerRating, Scenario};
use std::time::Instant;

pub mod beam_search;
pub mod exhaustive;
pub mod round_engine;
pub mod three_rounds;

/// Compaction 結果。
#[derive(Debug, Clone, Default)]
pub struct CompactionResult {
    /// 最終 Forest(對齊 §9.3,順序不反映優先級)
    pub forest: Vec<Scenario>,
    /// 是否觸發 BeamSearchFallback(forest size 超過 max_size)
    pub overflow_triggered: bool,
    /// Compaction 是否逾時(超過 compaction_timeout_secs)
    pub timeout_triggered: bool,
    /// 本階段產出的 forest 節點總數(Level 0 + 各 level aggregation,護欄剪枝前)
    pub compaction_paths: usize,
}

/// Stage 8 主入口。
///
/// 流程:
///   1. exhaustive::compact() 跑遞迴 aggregation(v3.7;G2.0 P1 相鄰性過濾在內層)
///   2. 檢查 forest size 是否超過 cfg.forest_max_size
///   3. 超過 → 套 cfg.overflow_strategy(BeamSearchFallback / Unbounded)
///   4. 同時檢查 compaction_timeout_secs
pub fn compact(
    scenarios: Vec<Scenario>,
    monowaves: &[Monowave],
    cfg: &NeelyEngineConfig,
) -> CompactionResult {
    let start = Instant::now();
    let timeout_duration = std::time::Duration::from_secs(cfg.compaction_timeout_secs);

    // ── Step 1:exhaustive 遞迴 aggregation(v3.7 迴圈 + G2.0 P1 過濾;
    //    輪數上限自 G2.1 起走 cfg.max_compaction_levels,A-8)
    let initial_forest = exhaustive::compact(scenarios, monowaves, cfg.max_compaction_levels);
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
            wave_count: 0,
            id: id.to_string(),
            wave_tree: WaveNode {
                degree_level: 0,
                base_label: crate::output::StructureLabel::Three,
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
    fn small_forest_passes_through_unchanged() {
        let cfg = NeelyEngineConfig::default(); // forest_max_size 1000
        let scenarios = vec![
            make_scenario("a", PowerRating::Bullish),
            make_scenario("b", PowerRating::Neutral),
        ];
        let result = compact(scenarios, &[], &cfg);
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
        let result = compact(scenarios, &[], &cfg);
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
        let result = compact(scenarios, &[], &cfg);
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
