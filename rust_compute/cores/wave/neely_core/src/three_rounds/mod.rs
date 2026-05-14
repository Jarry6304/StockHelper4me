// three_rounds — Stage 8 後處理:Three Rounds nested context + Round 3 暫停
//
// 對齊 m3Spec/neely_rules.md §Ch4 Three Rounds(1198-1276 行)
//       + §Ch7 Compaction(1801-1817 行)
//       + §Ch10 Pattern Implications(2021 行 — 三角內 Power = 0 例外)
//       + m3Spec/neely_core_architecture.md §7.1 Stage 8 + §8.4 Round3PauseInfo
//
// **Phase 8 PR**:
//   - Round 1 已隱含於 Stage 5 Classifier(從 Pre-Constructive Logic 標的 monowaves 找出 Standard / Non-Standard Series)
//   - Round 2 已於 Phase 6 落地(Scenario.compacted_base_label 透過 Ch7 Compaction Reassessment)
//   - **Round 3**:本 PR 偵測「forest 中無任何 scenario 帶 :L5/:L3 base」→ 標 awaiting_l_label
//   - **Nested context**:本 PR 偵測 scenario A 範圍涵蓋於 scenario B (Triangle) 內
//     → A.in_triangle_context = true,供 Power Rating 套 in_triangle = 0 例外

use crate::output::{
    NeelyPatternType, Round3PauseInfo, Scenario, StructureLabel,
};

/// Stage 8 後處理主入口:對 forest 套 Three Rounds nested context + Round 3 偵測。
///
/// 步驟:
///   1. nested context:對每對 (scenario_a, scenario_b) 檢查 a 是否完全涵蓋於 b 的時間範圍
///      且 b 為 Triangle → a.in_triangle_context = true
///   2. Round 3 偵測:若 forest 中無任何 scenario.compacted_base_label 為 Five (= :L5/:L3 對應)
///      或 Three(若為 Three 但 wave_count < 5,即 Zigzag/Flat 仍視 :L 標)→
///      標 awaiting_l_label;否則無動作
///
/// 回傳 Round3PauseInfo(若觸發暫停)。
pub fn apply(forest: &mut [Scenario]) -> Option<Round3PauseInfo> {
    apply_nested_triangle_context(forest);
    detect_round3_pause(forest)
}

/// Step 1:nested context 偵測。
///
/// 對每個 scenario A,若存在另一 scenario B (Triangle) 使得:
///   B.start ≤ A.start AND A.end ≤ B.end AND A.id != B.id
/// → A.in_triangle_context = true
fn apply_nested_triangle_context(forest: &mut [Scenario]) {
    // 蒐集 Triangle scenario 的範圍(避免 double mutable borrow)
    let triangle_ranges: Vec<(String, chrono::NaiveDate, chrono::NaiveDate)> = forest
        .iter()
        .filter(|s| matches!(s.pattern_type, NeelyPatternType::Triangle { .. }))
        .map(|s| (s.id.clone(), s.wave_tree.start, s.wave_tree.end))
        .collect();

    for scenario in forest.iter_mut() {
        let a_start = scenario.wave_tree.start;
        let a_end = scenario.wave_tree.end;
        let nested = triangle_ranges
            .iter()
            .any(|(t_id, t_start, t_end)| {
                t_id != &scenario.id && *t_start <= a_start && a_end <= *t_end
            });
        scenario.in_triangle_context = nested;
    }
}

/// Step 2:Round 3 暫停偵測。
///
/// 對齊 spec 1258-1265 行:「圖中無任何 L 標(僅剩 :_3/:_5 序列)」→ Round 3 暫停。
///
/// 簡化判定:Phase 8 採「forest 中無任何 scenario 的 compacted_base_label 是 Five
/// 或 Three」(即 forest 為空 或 全部 Scenario 沒被識別出 Standard pattern)→ Round 3 暫停。
///
/// **設計選擇**:本 PR 採嚴格判定 — 只有 forest 完全空時才觸發 Round 3 暫停,
/// 若 forest 非空表示已有 scenario 帶 base label。完整 «:L5/:L3 sole label » 偵測
/// 需 Pattern Isolation 結果整合,留 P9+。
fn detect_round3_pause(forest: &mut [Scenario]) -> Option<Round3PauseInfo> {
    let total_count = forest.len();
    if total_count == 0 {
        // Forest 完全空 → 圖中沒有任何 confirmed scenario → 等待新 :L5/:L3
        return Some(Round3PauseInfo {
            reason: "Forest 為空,圖中尚未識別出任何 Standard/Non-Standard Elliott pattern;\
                     等待新 :L5/:L3 出現才能進入下一輪 Round 1"
                .to_string(),
            affected_scenario_count: 0,
        });
    }

    // 進階:檢查是否所有 scenario 的 base label 都不在 anchor 集合
    //   (`:F3` / `x:c3` / `:L3` / `:s5` / `:L5` 為 Pattern Isolation 用的 anchor)
    //   spec 上 Round 3 觸發 = 圖上無新 :L3/:L5 出現
    //   compacted_base_label 是 Three / Five 兩種,Five 已對應 :5;Three 對應 :3
    //   若想嚴格區別「:L5 vs :5」需 Pattern Isolation 整合(留 P9+)
    //   Phase 8 採:forest 中至少有 1 個 Five → 不暫停;否則(全 Three) → 暫停
    let has_five_label = forest
        .iter()
        .any(|s| matches!(s.compacted_base_label, StructureLabel::Five));

    if !has_five_label {
        // 所有 scenarios 都是 Three(corrective)→ 沒新 impulse 收尾 → Round 3 暫停
        for scenario in forest.iter_mut() {
            scenario.awaiting_l_label = true;
        }
        Some(Round3PauseInfo {
            reason: format!(
                "Forest 中全部 {} 個 scenarios 都是 corrective(:3),\
                 無新 :L5 收尾 — 進入 Round 3 暫停;持有原方向,維持原計數",
                total_count
            ),
            affected_scenario_count: total_count,
        })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::*;
    use chrono::NaiveDate;

    fn make_scenario(
        id: &str,
        start_day: i64,
        end_day: i64,
        pattern: NeelyPatternType,
        base_label: StructureLabel,
    ) -> Scenario {
        let base = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        Scenario {
            id: id.to_string(),
            wave_tree: WaveNode {
                label: id.to_string(),
                start: base + chrono::Duration::days(start_day),
                end: base + chrono::Duration::days(end_day),
                children: Vec::new(),
            },
            pattern_type: pattern,
            initial_direction: MonowaveDirection::Up,
            compacted_base_label: base_label,
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
        }
    }

    #[test]
    fn empty_forest_triggers_round3_pause() {
        let mut forest: Vec<Scenario> = Vec::new();
        let pause = apply(&mut forest);
        assert!(pause.is_some());
        assert_eq!(pause.unwrap().affected_scenario_count, 0);
    }

    #[test]
    fn forest_with_only_three_base_triggers_round3_pause() {
        let mut forest = vec![
            make_scenario(
                "z1",
                0,
                10,
                NeelyPatternType::Zigzag {
                    sub_kind: ZigzagKind::Single,
                },
                StructureLabel::Three,
            ),
            make_scenario(
                "f1",
                10,
                20,
                NeelyPatternType::Flat {
                    sub_kind: FlatKind::Regular,
                },
                StructureLabel::Three,
            ),
        ];
        let pause = apply(&mut forest);
        assert!(pause.is_some());
        assert_eq!(pause.unwrap().affected_scenario_count, 2);
        assert!(forest.iter().all(|s| s.awaiting_l_label));
    }

    #[test]
    fn forest_with_five_base_does_not_trigger_round3_pause() {
        let mut forest = vec![
            make_scenario(
                "i1",
                0,
                10,
                NeelyPatternType::Impulse,
                StructureLabel::Five,
            ),
            make_scenario(
                "z1",
                10,
                20,
                NeelyPatternType::Zigzag {
                    sub_kind: ZigzagKind::Single,
                },
                StructureLabel::Three,
            ),
        ];
        let pause = apply(&mut forest);
        assert!(pause.is_none());
        assert!(forest.iter().all(|s| !s.awaiting_l_label));
    }

    #[test]
    fn nested_zigzag_inside_triangle_marks_in_triangle_context() {
        let mut forest = vec![
            // 外層 Triangle:0..30
            make_scenario(
                "t1",
                0,
                30,
                NeelyPatternType::Triangle {
                    sub_kind: TriangleKind::Contracting,
                },
                StructureLabel::Three,
            ),
            // 內層 Zigzag:5..15(完全涵蓋於 Triangle 範圍內)
            make_scenario(
                "z1",
                5,
                15,
                NeelyPatternType::Zigzag {
                    sub_kind: ZigzagKind::Single,
                },
                StructureLabel::Three,
            ),
            // 外層 Impulse:0..30(同範圍但不是 Triangle)
            make_scenario(
                "i1",
                0,
                30,
                NeelyPatternType::Impulse,
                StructureLabel::Five,
            ),
        ];
        apply(&mut forest);
        // Triangle (t1) 自身:不應 in_triangle_context = true(自己不能 nest 自己)
        let t1 = forest.iter().find(|s| s.id == "t1").unwrap();
        assert!(!t1.in_triangle_context);
        // Zigzag (z1):被 Triangle 涵蓋 → true
        let z1 = forest.iter().find(|s| s.id == "z1").unwrap();
        assert!(z1.in_triangle_context);
        // Impulse (i1):同範圍但 Triangle 也是 0..30,t1.start ≤ i1.start AND i1.end ≤ t1.end
        // → 也算 nested(同範圍邊界 inclusive)
        let i1 = forest.iter().find(|s| s.id == "i1").unwrap();
        assert!(i1.in_triangle_context);
    }
}
