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
//   - 收斂條件:`max_levels`(config `max_compaction_levels`)或 next level 為空(進 Round 3 暫停)
//
// **G2.0 現況(m3Spec/neely_compaction_v2.md,2026-08-25)**:本檔遞迴迴圈與
// `three_rounds.rs` 弱比對為 v2 規格的**取代對象**(§3.3 — tiling-round 引擎,G2.1+);
// 止血後狀態:D-1/D-2 由 P1 相鄰性過濾擋下(枚舉不完整但產出皆合法)、D-3 由 P3
// 暫填 Σ(children) 止血,D-4/D-5 未修 — 完整缺陷表見 v2 規格 §1.2。
//
// 留 tiling-round 引擎(G2.1+)接手,不再於本檔演進:
//   - Round 2 動作 B「邊界波 Retracement Rules 重評」真鄰居版(v2 §6.1;現行視窗內近似 = D-4)
//   - sub-wave 嵌套真實 monowave price(v2 §5.1 CompactionNode 端點合約)

use crate::output::{Monowave, Scenario};
use super::three_rounds;

/// 窮舉所有合法 compression paths,產出 Forest。
///
/// **v3.7 真窮舉版**:對輸入 scenarios 跑遞迴 aggregation:
///   - Level 0:原始 base scenarios(對齊 v2.0 pass-through 行為)
///   - Level 1~max:對前一 level 跑 `three_rounds::aggregate_one_level`
///   - 收斂條件:next level 為空(Round 3 暫停)or hit `max_levels`
///
/// `max_levels` 自 G2.1 起由 `NeelyEngineConfig.max_compaction_levels` 傳入
/// (compaction v2 A-8:常數升級 config 欄,預設 4)。
///
/// 結果 Forest 含**所有 levels** 的 scenarios,順序不反映優先級(對齊 §9.3)。
/// 由 upstream 的 forest_max_size + BeamSearchFallback 接管上限保護。
pub fn compact(
    scenarios: Vec<Scenario>,
    monowaves: &[Monowave],
    max_levels: usize,
) -> Vec<Scenario> {
    let mut forest = scenarios.clone(); // Level 0
    let mut current_level = scenarios;

    for _level in 1..=max_levels {
        let next_level = three_rounds::aggregate_one_level(&current_level, monowaves);
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

    // G2.0 T-6(compaction v2 §8.4):同日退化日期改真實日期鏈(測試債清償,D-6)

    #[test]
    fn empty_input_yields_empty() {
        assert!(compact(vec![], &[], 4).is_empty());
    }

    #[test]
    fn single_scenario_pass_through() {
        let scenarios = vec![make_scenario(
            "a",
            StructureLabel::Five,
            MonowaveDirection::Up,
            "2026-01-01",
            "2026-01-10",
        )];
        let forest = compact(scenarios, &[], 4);
        // 1 scenario < 3 → 無 aggregation,Level 0 pass-through
        assert_eq!(forest.len(), 1);
        assert_eq!(forest[0].id, "a");
    }

    #[test]
    fn two_scenarios_pass_through() {
        let scenarios = vec![
            make_scenario("a", StructureLabel::Five, MonowaveDirection::Up, "2026-01-01", "2026-01-10"),
            make_scenario("b", StructureLabel::Three, MonowaveDirection::Down, "2026-01-10", "2026-01-20"),
        ];
        let forest = compact(scenarios, &[], 4);
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
        let forest = compact(scenarios, &[], 4);
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
        let forest = compact(scenarios, &[], 4);
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
        let forest = compact(scenarios, &[], 4);
        // 全 Up 方向 → 無 aggregation,Level 0 pass-through
        assert_eq!(forest.len(), 3);
    }

    #[test]
    fn max_compaction_levels_respected() {
        // 構造大量可 aggregate 的場景:50 段連續 alternating scenarios。
        // T-6:原「同月 % 28 wrap」會產生 end < start 的退化日期;改真實連續
        // 日期鏈(50 段 × 5 天),P1 相鄰性檢查下 aggregation 仍發生
        let base = date("2026-01-01");
        let mut scenarios = Vec::new();
        for i in 0..50i64 {
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
            let start = (base + chrono::Duration::days(i * 5)).format("%Y-%m-%d").to_string();
            let end = (base + chrono::Duration::days((i + 1) * 5)).format("%Y-%m-%d").to_string();
            scenarios.push(make_scenario(
                &format!("s{}", i),
                label,
                dir,
                &start,
                &end,
            ));
        }
        let forest = compact(scenarios, &[], 4);
        // Level 0:50 + 各 level 多次 aggregation,有限數量(max_levels 終止)
        assert!(forest.len() > 50, "至少 Level 0 50 個 + Level 1+ aggregated");
        // 確認沒有 runaway:total < 一個合理上限(對齊 forest_max_size 1000 預設值;
        // 真實 production 由 upstream beam_search 保護)
        assert!(forest.len() < 5000, "Level 4 收斂保護有效");
    }
}
