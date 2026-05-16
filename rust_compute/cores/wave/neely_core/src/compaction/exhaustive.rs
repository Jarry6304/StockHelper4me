// exhaustive.rs — Compaction 真窮舉模式(v3.7 Phase B)
//
// 對齊 m3Spec/neely_core_architecture.md §十一(Compaction 重新定位)
//     + m3Spec/neely_rules.md §Three Rounds 教學流程(line 1198-1256)。
//
// 設計目標:
//   - 純結構壓縮(§11.2)— 不選最優,窮舉所有合法 compression paths
//   - 對齊 v2.0「展示式」哲學(§2.1)
//
// **v3.7 升級(2026-05-16)**(對齊 plan v3.7 Phase B):
//   - 從 M3 PR-5 簡化版 pass-through 升為真遞迴 aggregation
//   - 對齊 spec §Three Rounds:Round 1 識別 → Round 2 壓縮 → 遞迴回 Round 1 在更大級
//   - 每 level 跑 `three_rounds::aggregate_one_level` 對 Figure 4-3 五大序列比對
//   - 收斂條件:`MAX_COMPACTION_LEVELS` 或 next level 為空(進 Round 3 暫停)
//
// 留 V3 後續:
//   - Round 2 動作 B「邊界波 Retracement Rules 重評」(spec line 1249-1251),需要部分
//     Stage 3-4 rerun,複雜度高 — 目前在 `three_rounds.rs` 留 `AdvisoryFinding` 註解
//   - sub-wave 嵌套真實 monowave price(目前 Level-0 placeholder = wave_tree.children.len()),
//     接 Stage 3 Bottom-up Generator 進階 + 5-wave-of-3 嵌套

use crate::output::Scenario;
use super::three_rounds;

/// Compaction Level 上限(對齊 architecture §13 warmup_periods 隱含的 Degree 階層):
/// 一般股票歷史 ~20 年 daily ≈ 5000 K 線,理論 Degree 約 Minute → Cycle 6-7 級。
/// 4 級窮舉足以涵蓋 Subminuette → Primary,超過走 beam_search fallback。
const MAX_COMPACTION_LEVELS: usize = 4;

/// 窮舉所有合法 compression paths,產出 Forest。
///
/// **v3.7 真窮舉版**:對輸入 scenarios 跑遞迴 aggregation:
///   - Level 0:原始 base scenarios(對齊 v2.0 pass-through 行為)
///   - Level 1~MAX:對前一 level 跑 `three_rounds::aggregate_one_level`
///   - 收斂條件:next level 為空(Round 3 暫停)or hit MAX_COMPACTION_LEVELS
///
/// 結果 Forest 含**所有 levels** 的 scenarios,順序不反映優先級(對齊 §9.3)。
/// 由 upstream 的 forest_max_size + BeamSearchFallback 接管上限保護。
pub fn compact(scenarios: Vec<Scenario>) -> Vec<Scenario> {
    let mut forest = scenarios.clone(); // Level 0
    let mut current_level = scenarios;

    for _level in 1..=MAX_COMPACTION_LEVELS {
        let next_level = three_rounds::aggregate_one_level(&current_level);
        if next_level.is_empty() {
            break; // Round 3 暫停:沒新 aggregation 發生
        }
        forest.extend(next_level.clone());
        current_level = next_level;
    }

    forest
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::*;
    use chrono::NaiveDate;

    fn date(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn make_scenario(
        id: &str,
        label: StructureLabel,
        dir: MonowaveDirection,
        start: &str,
        end: &str,
    ) -> Scenario {
        Scenario {
            id: id.to_string(),
            wave_tree: WaveNode {
                label: id.to_string(),
                start: date(start),
                end: date(end),
                children: Vec::new(),
            },
            pattern_type: if label == StructureLabel::Five {
                NeelyPatternType::Impulse
            } else {
                NeelyPatternType::Zigzag {
                    sub_kind: ZigzagKind::Single,
                }
            },
            initial_direction: dir,
            compacted_base_label: label,
            structure_label: id.to_string(),
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

    fn make_simple(id: &str) -> Scenario {
        make_scenario(
            id,
            StructureLabel::Five,
            MonowaveDirection::Up,
            "2026-01-01",
            "2026-01-01",
        )
    }

    #[test]
    fn empty_input_yields_empty() {
        assert!(compact(vec![]).is_empty());
    }

    #[test]
    fn single_scenario_pass_through() {
        let scenarios = vec![make_simple("a")];
        let forest = compact(scenarios);
        // 1 scenario < 3 → 無 aggregation,Level 0 pass-through
        assert_eq!(forest.len(), 1);
        assert_eq!(forest[0].id, "a");
    }

    #[test]
    fn two_scenarios_pass_through() {
        let scenarios = vec![make_simple("a"), make_simple("b")];
        let forest = compact(scenarios);
        // 2 scenarios < 3 → 無 aggregation
        assert_eq!(forest.len(), 2);
    }

    #[test]
    fn three_alternating_zigzag_aggregates_to_level_1() {
        // [:_5(Up), :_3(Down), :_5(Up)] — Zigzag
        let scenarios = vec![
            make_scenario("a", StructureLabel::Five, MonowaveDirection::Up, "2026-01-01", "2026-01-10"),
            make_scenario("b", StructureLabel::Three, MonowaveDirection::Down, "2026-01-10", "2026-01-15"),
            make_scenario("c", StructureLabel::Five, MonowaveDirection::Up, "2026-01-15", "2026-01-25"),
        ];
        let forest = compact(scenarios);
        // Level 0:3 個 + Level 1:1 個 Zigzag = 4
        assert_eq!(forest.len(), 4);
        let level_1 = forest
            .iter()
            .find(|s| matches!(s.pattern_type, NeelyPatternType::Zigzag { .. }) && s.wave_tree.children.len() == 3);
        assert!(level_1.is_some(), "Level 1 Zigzag 應存在");
    }

    #[test]
    fn five_alternating_trending_impulse_aggregates_to_level_1() {
        // [:_5(Up), :_3(Down), :_5(Up), :_3(Down), :_5(Up)] — Trending Impulse
        let scenarios = vec![
            make_scenario("a", StructureLabel::Five, MonowaveDirection::Up, "2026-01-01", "2026-01-10"),
            make_scenario("b", StructureLabel::Three, MonowaveDirection::Down, "2026-01-10", "2026-01-15"),
            make_scenario("c", StructureLabel::Five, MonowaveDirection::Up, "2026-01-15", "2026-01-25"),
            make_scenario("d", StructureLabel::Three, MonowaveDirection::Down, "2026-01-25", "2026-01-30"),
            make_scenario("e", StructureLabel::Five, MonowaveDirection::Up, "2026-01-30", "2026-02-10"),
        ];
        let forest = compact(scenarios);
        // Level 0:5 個 + Level 1:有 5-pattern Impulse 與內含的 3-pattern Zigzag(滑窗 a-b-c / c-d-e)
        let impulses: Vec<_> = forest
            .iter()
            .filter(|s| matches!(s.pattern_type, NeelyPatternType::Impulse) && s.wave_tree.children.len() == 5)
            .collect();
        assert!(!impulses.is_empty(), "5-pattern Impulse 應 aggregate 至 Level 1");
    }

    #[test]
    fn no_alternation_no_aggregation() {
        let scenarios = vec![
            make_scenario("a", StructureLabel::Five, MonowaveDirection::Up, "2026-01-01", "2026-01-10"),
            make_scenario("b", StructureLabel::Three, MonowaveDirection::Up, "2026-01-10", "2026-01-15"),
            make_scenario("c", StructureLabel::Five, MonowaveDirection::Up, "2026-01-15", "2026-01-25"),
        ];
        let forest = compact(scenarios);
        // 全 Up 方向 → 無 aggregation,Level 0 pass-through
        assert_eq!(forest.len(), 3);
    }

    #[test]
    fn max_compaction_levels_respected() {
        // 構造可無限 aggregate 的場景:極多 alternating zigzag scenarios
        let mut scenarios = Vec::new();
        for i in 0..50 {
            let dir = if i % 2 == 0 {
                MonowaveDirection::Up
            } else {
                MonowaveDirection::Down
            };
            let label = if i % 2 == 0 {
                StructureLabel::Five
            } else {
                StructureLabel::Three
            };
            let start = format!("2026-01-{:02}", (i % 28) + 1);
            let end = format!("2026-01-{:02}", ((i + 1) % 28) + 1);
            scenarios.push(make_scenario(
                &format!("s{}", i),
                label,
                dir,
                &start,
                &end,
            ));
        }
        let forest = compact(scenarios);
        // Level 0:50 + 各 level 多次 aggregation,有限數量(MAX_COMPACTION_LEVELS=4 終止)
        assert!(forest.len() > 50, "至少 Level 0 50 個 + Level 1+ aggregated");
        // 確認沒有 runaway:total < 一個合理上限(對齊 forest_max_size 1000 預設值;
        // 真實 production 由 upstream beam_search 保護)
        assert!(forest.len() < 5000, "Level 4 收斂保護有效");
    }
}
