// reverse_logic — Stage 10.5:Reverse Logic Rule(Neely Extension)
//
// 對齊 m3Spec/neely_rules.md §Expansion of Possibilities — Reverse Logic Rule
//       (2598-2608 行)+ m3Spec/neely_core_architecture.md §7.1 Stage 10.5
//
// 核心定理(spec 2599 行):
//   「同一資料序列存在多個完美合法的計數時,市場必定處於某個修正/衝動形態的中央
//   (b 的 b、3 的 3、或 Non-Standard 複雜修正的 x)。可能性越多,越靠近中央。」
//
// 操作意涵(spec 2601-2604 行):
//   - 觀察到多套合理計數 → 自動剔除「形態即將完成」的選項,只保留「市場處於中段」的解讀
//   - 若尚未進場 → 等到可能性收斂為一再進場
//   - 若已持倉獲利 → 多套計數出現不代表頂底,而是還有路要走
//
// **Phase 11 PR**:
//   - REVERSE_LOGIC_THRESHOLD = 2(spec 2599 行「多個」,最小值 2)寫死,不可外部化
//   - 「形態即將完成」識別:5-wave Impulse + Triangle (5-segment 收尾)+ Combination
//     Triple* 在「非 in_triangle_context」狀態 → 屬於完成候選
//   - 「市場處於中段」識別:Zigzag / Flat / Combination Double* / in_triangle_context
//     的 scenario → 屬於中段候選
//   - Phase 11 不直接過濾 forest(仍交給 Aggregation Layer),只記錄
//     `suggested_filter_ids` 供下游用

use crate::output::{
    CombinationKind, NeelyPatternType, ReverseLogicObservation, Scenario, TriangleKind,
};

/// 多套計數判定閾值(spec 2599「多個」最小值,寫死不外部化)。
pub const REVERSE_LOGIC_THRESHOLD: usize = 2;

/// Stage 10.5 主入口:對 forest 套 Reverse Logic 觀察。
///
/// scenario_count < REVERSE_LOGIC_THRESHOLD → 回 None(無 ambiguity,
/// 不適用 Reverse Logic 操作意涵)。
pub fn observe(forest: &[Scenario]) -> Option<ReverseLogicObservation> {
    let scenario_count = forest.len();
    if scenario_count < REVERSE_LOGIC_THRESHOLD {
        return None;
    }

    let suggested_filter_ids: Vec<String> = forest
        .iter()
        .filter(|s| is_near_completion(s))
        .map(|s| s.id.clone())
        .collect();

    let message = format!(
        "Reverse Logic 觸發:同一資料上 {} 套合法計數 → 市場處於某更大形態中段;\
         建議過濾 {} 個「形態即將完成」候選,保留中段候選",
        scenario_count,
        suggested_filter_ids.len()
    );

    Some(ReverseLogicObservation {
        scenario_count,
        triggered: true,
        message,
        suggested_filter_ids,
    })
}

/// 「形態即將完成」識別 — spec 2602 行「自動剔除即將完成的選項」。
///
/// 規則:
///   - 完整 5-wave Impulse(Trending / Terminal):屬「完成候選」
///   - 5-segment Triangle 收尾後緊接 Thrust:屬「完成候選」
///   - in_triangle_context = true 的 scenario:屬「中段候選」(非完成)— 跳過
///   - Zigzag / Flat / Combination(Double / Triple):
///       * Triple* 變體本身是高度複合,接近「c.t.」終點 → 完成候選
///       * Double / 其餘 Combination:多半是中段段位 → 中段候選
fn is_near_completion(scenario: &Scenario) -> bool {
    // in_triangle_context 標明該 scenario 是更大 Triangle 的內部段
    // → 一定是「中段」,不該被過濾
    if scenario.in_triangle_context {
        return false;
    }

    match &scenario.pattern_type {
        NeelyPatternType::Impulse => true,
        NeelyPatternType::Diagonal { .. } => true, // Terminal Impulse 也是完成候選
        NeelyPatternType::Triangle { sub_kind } => {
            // Contracting / Expanding Triangle 五段完成後可能緊接 Thrust → 完成候選
            // Limiting Triangle 仍在收斂中(spec §Triangles)→ 中段候選
            !matches!(sub_kind, TriangleKind::Limiting)
        }
        NeelyPatternType::Combination { sub_kinds } => {
            // Triple* 變體屬複合修正末段 → 完成候選
            sub_kinds.iter().any(|k| {
                matches!(
                    k,
                    CombinationKind::TripleZigzag
                        | CombinationKind::TripleCombination
                        | CombinationKind::TripleThree
                        | CombinationKind::TripleThreeCombination
                        | CombinationKind::TripleThreeRunning
                )
            })
        }
        // Zigzag / Flat 單獨出現 → 中段候選(多半是更大 Combination 的子段)
        NeelyPatternType::Zigzag { .. } => false,
        NeelyPatternType::Flat { .. } => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::*;
    use chrono::NaiveDate;

    fn make_scenario(id: &str, pattern: NeelyPatternType, in_triangle: bool) -> Scenario {
        let date = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        Scenario {
            id: id.to_string(),
            wave_tree: WaveNode {
                label: "test".to_string(),
                start: date,
                end: date,
                children: Vec::new(),
            },
            pattern_type: pattern,
            initial_direction: MonowaveDirection::Up,
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
            in_triangle_context: in_triangle,
            awaiting_l_label: false,
        }
    }

    #[test]
    fn single_scenario_returns_none() {
        let forest = vec![make_scenario("s1", NeelyPatternType::Impulse, false)];
        assert!(observe(&forest).is_none());
    }

    #[test]
    fn empty_forest_returns_none() {
        assert!(observe(&[]).is_none());
    }

    #[test]
    fn two_scenarios_triggers_reverse_logic() {
        let forest = vec![
            make_scenario("s1", NeelyPatternType::Impulse, false),
            make_scenario(
                "s2",
                NeelyPatternType::Zigzag {
                    sub_kind: ZigzagKind::Single,
                },
                false,
            ),
        ];
        let obs = observe(&forest).expect("should trigger");
        assert!(obs.triggered);
        assert_eq!(obs.scenario_count, 2);
        // s1 Impulse 完整 → 過濾;s2 Zigzag 中段 → 保留
        assert_eq!(obs.suggested_filter_ids, vec!["s1".to_string()]);
    }

    #[test]
    fn in_triangle_context_protects_from_filter() {
        let forest = vec![
            make_scenario("s1", NeelyPatternType::Impulse, true), // in triangle context
            make_scenario("s2", NeelyPatternType::Impulse, false), // standalone
        ];
        let obs = observe(&forest).expect("should trigger");
        // s1 受 in_triangle_context 保護不算完成候選;s2 算
        assert_eq!(obs.suggested_filter_ids, vec!["s2".to_string()]);
    }

    #[test]
    fn triangle_limiting_is_mid_pattern() {
        let forest = vec![
            make_scenario(
                "s1",
                NeelyPatternType::Triangle {
                    sub_kind: TriangleKind::Limiting,
                },
                false,
            ),
            make_scenario(
                "s2",
                NeelyPatternType::Triangle {
                    sub_kind: TriangleKind::Contracting,
                },
                false,
            ),
        ];
        let obs = observe(&forest).expect("should trigger");
        // Limiting → 中段;Contracting → 完成候選
        assert_eq!(obs.suggested_filter_ids, vec!["s2".to_string()]);
    }

    #[test]
    fn triple_combination_is_near_completion() {
        let forest = vec![
            make_scenario(
                "s1",
                NeelyPatternType::Combination {
                    sub_kinds: vec![CombinationKind::TripleZigzag],
                },
                false,
            ),
            make_scenario(
                "s2",
                NeelyPatternType::Combination {
                    sub_kinds: vec![CombinationKind::DoubleZigzag],
                },
                false,
            ),
        ];
        let obs = observe(&forest).expect("should trigger");
        // Triple* → 完成候選;Double → 中段
        assert_eq!(obs.suggested_filter_ids, vec!["s1".to_string()]);
    }
}
