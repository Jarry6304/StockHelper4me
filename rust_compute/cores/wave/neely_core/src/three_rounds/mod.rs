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

use crate::output::{Round3PauseInfo, Scenario, StructureLabel};

/// Stage 8.5 主入口:Round 3 暫停偵測(m3Spec/neely_compaction_v2.md §5.3:
/// 判定規則沿用,輸入 = 凍結後 forest 末端狀態)。
///
/// nested Triangle context 不再於此推導 — compaction v2 §7.2 語意收緊:
/// `in_triangle_context` = 節點被 Triangle 節點**真包含**(同 tiling 血緣),
/// 由 round_engine 凍結時填值;原「日期範圍重疊即算」近似廢除。
///
/// 回傳 Round3PauseInfo(若觸發暫停)。
pub fn apply(forest: &mut [Scenario]) -> Option<Round3PauseInfo> {
    detect_round3_pause(forest)
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
            wave_count: 0,
            id: id.to_string(),
            wave_tree: WaveNode {
                degree_level: 0,
                base_label: crate::output::StructureLabel::Three,
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
            monowave_structure_labels: Vec::new(),
            round_state: RoundState::Round1,
            pattern_isolation_anchors: Vec::new(),
            triplexity_detected: false,
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
                    sub_kind: FlatKind::Common,
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
    fn apply_does_not_touch_frozen_in_triangle_context() {
        // compaction v2 §7.2:in_triangle_context 由凍結血緣填值,
        // Stage 8.5 不得覆寫(原日期重疊近似已廢除)
        let mut forest = vec![
            make_scenario(
                "t1",
                0,
                30,
                NeelyPatternType::Triangle {
                    sub_kind: TriangleKind::Contracting,
                },
                StructureLabel::Three,
            ),
            make_scenario(
                "i1",
                0,
                30,
                NeelyPatternType::Impulse,
                StructureLabel::Five,
            ),
        ];
        forest[1].in_triangle_context = true; // 模擬凍結填值
        apply(&mut forest);
        assert!(!forest[0].in_triangle_context);
        assert!(forest[1].in_triangle_context, "凍結值不得被 Stage 8.5 覆寫");
    }
}
