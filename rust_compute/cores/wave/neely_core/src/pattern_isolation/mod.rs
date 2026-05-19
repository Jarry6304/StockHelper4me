// pattern_isolation — Stage 3.5:Pattern Isolation Procedures + Zigzag DETOUR Test
//
// 對齊 m3Spec/neely_rules.md §Pattern Isolation Procedures(1064-1126 行)
//       + §Zigzag DETOUR Test(1283-1285 行)
//       + §Special Circumstances(1121-1123 行)
//
// **Phase 3 PR**(2026-05-13):
//   1. Pattern Isolation 6-step procedure 落地,輸出 Vec<PatternBound>
//      表「圖上可隔離的 Elliott 形態起終點對」
//   2. Zigzag DETOUR Test 輸出 Vec<DetourAnnotation>
//      標出「看似 Zigzag 但可優先嘗試 Impulse」的候選
//   3. NeelyCoreOutput 加 pattern_bounds + detour_annotations 兩個欄位供下游使用
//   4. Stage 4 Validator 不依賴 Stage 3.5 結果(本 PR 為「資訊性 stage」);
//      未來 PR 可以用 PatternBound 過濾 wave_candidates(P5+)
//
// **依賴**:必須先跑完 Stage 0 Pre-Constructive Logic
//          (classified[i].structure_label_candidates 已填好)。
//
// **Compaction validation**(Step 5):
//   - Phase 3:Compaction 未跑,所有 PatternBound `validated` 預設 false
//   - **v4.7.1 G1.1(2026-05-19)**:加 `validate_after_compaction(bounds, scenarios,
//     classified)` fn,在 lib.rs 主 pipeline 接 Compaction(Stage 8)後呼叫;
//     對 bounds 內的每筆,匹配 classified[start/end_idx].monowave.start_date/
//     end_date 與 Scenario.wave_tree.start/end 對應 → 找到 match → `validated = true`
//
// **Special Circumstances**(spec 1121-1123 行):
//   若 Compacted 形態 price action 超出自身起點 → 強制 base = `:3`
//   檢測方式:walk through monowaves [start_idx..=end_idx],
//   若任一 monowave 的 end_price 超越 start_monowave.start_price(在「相對 pattern 方向」上)
//   → forced_corrective = true

use crate::candidates::WaveCandidate;
use crate::monowave::ClassifiedMonowave;
use crate::output::{
    MonowaveDirection, PatternBound, DetourAnnotation, Scenario, StructureLabel,
    StructureLabelCandidate,
};

pub mod zigzag_detour;

/// Step 3 中合法的「往回退 3 個 label 後可作為起點」的標籤集合
/// (spec 1107 行:`:F3`、`x:c3`、`:L3`、`:s5`、`:L5`)
fn is_start_anchor_label(label: StructureLabel) -> bool {
    matches!(
        label,
        StructureLabel::F3
            | StructureLabel::XC3
            | StructureLabel::L3
            | StructureLabel::S5
            | StructureLabel::L5
    )
}

/// Step 2 中合法的「終點」標籤(sole `:L5` 或 `:L3`)
fn is_end_anchor_label(label: StructureLabel) -> bool {
    matches!(label, StructureLabel::L5 | StructureLabel::L3)
}

/// 取單一 monowave 的「sole label」— 若 candidates 只有 1 個且 certainty 為 Primary,回 Some(label)
/// 否則 None(spec 1101 行「sole `:L5/:L3`」的 sole 解讀為「single primary label」)
fn sole_label(candidates: &[StructureLabelCandidate]) -> Option<StructureLabel> {
    use crate::output::Certainty;
    let primaries: Vec<&StructureLabelCandidate> = candidates
        .iter()
        .filter(|c| matches!(c.certainty, Certainty::Primary))
        .collect();
    if primaries.len() == 1 {
        Some(primaries[0].label)
    } else {
        None
    }
}

/// Pattern Isolation 主程序(spec Step 1-6)。
///
/// 演算法:
///   1. 從最左側往前掃,找第一個 sole `:L5/:L3` monowave 作為「圓圈點」end_idx
///   2. 從 end_idx 往回退 3 個 label,找第一個 sole label ∈ {F3, XC3, L3, S5, L5} → start_idx
///   3. 檢查 (start, end) 內波數為奇(3 或 5);否則繼續往回退一波
///   4. F3 → 起點在 start_idx.start_date,其他 → 起點在 start_idx.end_date(即 [start_idx+1, end_idx])
///   5. 檢查 Special Circumstances 強制 :3 forced_corrective flag
///   6. push PatternBound,從 end_idx + 1 繼續找下一個
pub fn run(classified: &[ClassifiedMonowave]) -> Vec<PatternBound> {
    let mut bounds = Vec::new();
    let mut search_start = 0usize;

    loop {
        let Some(end_idx) = find_next_end_anchor(classified, search_start) else {
            break;
        };

        let end_label = sole_label(&classified[end_idx].structure_label_candidates)
            .expect("end_idx 已篩過 sole_label");

        let Some(bound) = isolate_pattern(classified, end_idx, end_label) else {
            // 找不到合法起點 → 跳過此 end_idx,繼續找下一個
            search_start = end_idx + 1;
            continue;
        };
        let next_start = bound.end_idx + 1;
        bounds.push(bound);
        search_start = next_start;
    }
    bounds
}

/// Step 2:從 search_start 起,找第一個 sole `:L5/:L3` monowave 的 index
fn find_next_end_anchor(classified: &[ClassifiedMonowave], search_start: usize) -> Option<usize> {
    (search_start..classified.len()).find(|&i| {
        sole_label(&classified[i].structure_label_candidates)
            .is_some_and(is_end_anchor_label)
    })
}

/// Step 3-5:從 end_idx 往回退,找合法起點 + 驗 odd count + Special Circumstances
fn isolate_pattern(
    classified: &[ClassifiedMonowave],
    end_idx: usize,
    end_label: StructureLabel,
) -> Option<PatternBound> {
    // Step 3:從 end_idx 往回退至少 3 個 label
    //   起手點為 end_idx - 3(若可能);若該 monowave 不符 sole anchor,繼續退一波
    if end_idx < 3 {
        return None;
    }

    let mut candidate_start = end_idx - 3;
    loop {
        // 取 candidate_start 的 sole label
        let start_label_opt = sole_label(&classified[candidate_start].structure_label_candidates);

        if let Some(start_label) = start_label_opt {
            if is_start_anchor_label(start_label) {
                // 找到合法起點 → 驗 Step 4 odd count
                let (actual_start, wave_count) = compute_pattern_range(
                    candidate_start,
                    end_idx,
                    start_label,
                );
                if wave_count % 2 == 1 && (3..=5).contains(&wave_count) {
                    // 符合 odd count + range 3 or 5 → 接受
                    let forced_corrective = check_special_circumstances(
                        classified,
                        actual_start,
                        end_idx,
                    );
                    return Some(PatternBound {
                        start_idx: actual_start,
                        end_idx,
                        start_label,
                        end_label,
                        validated: false,
                        forced_corrective,
                    });
                }
                // odd count 不符或 wave_count 超出 3/5 → 繼續往回退一波
            }
        }

        if candidate_start == 0 {
            return None;
        }
        candidate_start -= 1;
    }
}

/// Step 4 邏輯:
///   - F3 → 起點在 candidate_start.start_date,pattern 範圍 [candidate_start, end_idx]
///   - 其他 → 起點在 candidate_start.end_date(= candidate_start+1.start_date),
///     pattern 範圍 [candidate_start+1, end_idx]
fn compute_pattern_range(
    candidate_start: usize,
    end_idx: usize,
    start_label: StructureLabel,
) -> (usize, usize) {
    if matches!(start_label, StructureLabel::F3) {
        let count = end_idx - candidate_start + 1;
        (candidate_start, count)
    } else {
        // 起點在 candidate_start.end_date = candidate_start + 1 的 start_date
        let actual_start = candidate_start + 1;
        let count = end_idx - actual_start + 1;
        (actual_start, count)
    }
}

/// Special Circumstances 檢測(spec 1121-1123 行):
///   若 pattern 中任一 monowave 的 end_price 超越 pattern 起點 monowave 的 start_price
///   (在「相對 pattern 方向」上)→ forced_corrective = true
///
/// 「pattern 方向」= 起點 monowave 的 direction(Up/Down)
fn check_special_circumstances(
    classified: &[ClassifiedMonowave],
    start_idx: usize,
    end_idx: usize,
) -> bool {
    if start_idx > end_idx || end_idx >= classified.len() {
        return false;
    }
    let start_mw = &classified[start_idx].monowave;
    let start_price = start_mw.start_price;
    let pattern_dir = start_mw.direction;

    for cmw in &classified[(start_idx + 1)..=end_idx] {
        let mw = &cmw.monowave;
        let exceeded = match pattern_dir {
            MonowaveDirection::Up => mw.end_price < start_price,
            MonowaveDirection::Down => mw.end_price > start_price,
            MonowaveDirection::Neutral => false,
        };
        if exceeded {
            return true;
        }
    }
    false
}

/// **v4.7.1 G1.1**:Compaction-after validation hook(spec Step 5)。
///
/// 對齊 plan §G1.1:接 Stage 8 Compaction 跑完後,對 `bounds` 內每筆 PatternBound:
///   - 取 `classified[bound.start_idx].monowave.start_date` 與
///     `classified[bound.end_idx].monowave.end_date` 作為 pattern 邊界日期
///   - 找 `scenarios` 內是否有 Scenario 之 `wave_tree.start == start_date` 且
///     `wave_tree.end == end_date`(date exact match;Compaction 產出的 scenario
///     對應到該 isolated pattern)
///   - 若 match → `bound.validated = true`;否則保 false
///
/// 設計原則(對齊 architecture §三 pattern_isolation Step 5):
///   - Compaction Three Rounds 跑完後產生的 scenarios 是「合法 Elliott 形態」
///   - PatternBound 是 Stage 3.5 從 sole `:L5/:L3` anchor 推出的「可能形態邊界」
///   - 兩者邊界重合 → 該 PatternBound 通過 Compaction 驗證(`validated = true`)
///   - 沒重合 → 該 anchor 推出的 bound 沒被 Compaction 接受,留 `validated = false`
///     供 LLM diagnostics 看(NEoWave 仍承認其為「可能的形態邊界」資訊)
pub fn validate_after_compaction(
    bounds: &mut [PatternBound],
    scenarios: &[Scenario],
    classified: &[ClassifiedMonowave],
) {
    for bound in bounds.iter_mut() {
        if bound.start_idx >= classified.len() || bound.end_idx >= classified.len() {
            continue;
        }
        let start_date = classified[bound.start_idx].monowave.start_date;
        let end_date = classified[bound.end_idx].monowave.end_date;
        bound.validated = scenarios
            .iter()
            .any(|s| s.wave_tree.start == start_date && s.wave_tree.end == end_date);
    }
}

/// Zigzag DETOUR Test 對外入口 — 對 Stage 3 wave_candidates 跑 DETOUR
///
/// 對齊 spec 1283-1285 行:Zigzag(`:L5` 結尾)→ 檢查前兩個 Structure Label 能否組成更大 Impulse。
///
/// 注意:DETOUR Test 與 Pattern Isolation 是兩個獨立過程,共用 Stage 3.5 stage 命名。
pub fn run_detour(
    candidates: &[WaveCandidate],
    classified: &[ClassifiedMonowave],
) -> Vec<DetourAnnotation> {
    zigzag_detour::test(candidates, classified)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::monowave::ProportionMetrics;
    use crate::output::{Certainty, Monowave};
    use chrono::NaiveDate;

    fn cmw_with_labels(
        start_p: f64,
        end_p: f64,
        dir: MonowaveDirection,
        labels: Vec<(StructureLabel, Certainty)>,
    ) -> ClassifiedMonowave {
        let candidates = labels
            .into_iter()
            .map(|(label, certainty)| StructureLabelCandidate { label, certainty })
            .collect();
        ClassifiedMonowave {
            monowave: Monowave {
                start_date: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                end_date: NaiveDate::from_ymd_opt(2026, 1, 5).unwrap(),
                start_price: start_p,
                end_price: end_p,
                direction: dir,
                bar_indices: (0, 0),
            },
            atr_at_start: 1.0,
            metrics: ProportionMetrics {
                magnitude: (end_p - start_p).abs(),
                duration_bars: 5,
                atr_relative: 1.0,
                slope_vs_45deg: 1.0,
            },
            structure_label_candidates: candidates,
            polywave_size: 0,
        }
    }

    #[test]
    fn sole_label_detects_single_primary() {
        let cands = vec![StructureLabelCandidate {
            label: StructureLabel::L5,
            certainty: Certainty::Primary,
        }];
        assert!(matches!(sole_label(&cands), Some(StructureLabel::L5)));
    }

    #[test]
    fn sole_label_rejects_multi_primary() {
        let cands = vec![
            StructureLabelCandidate {
                label: StructureLabel::L5,
                certainty: Certainty::Primary,
            },
            StructureLabelCandidate {
                label: StructureLabel::C3,
                certainty: Certainty::Primary,
            },
        ];
        assert!(sole_label(&cands).is_none());
    }

    #[test]
    fn sole_label_ignores_non_primary() {
        let cands = vec![
            StructureLabelCandidate {
                label: StructureLabel::L5,
                certainty: Certainty::Primary,
            },
            StructureLabelCandidate {
                label: StructureLabel::C3,
                certainty: Certainty::Possible,
            },
        ];
        assert!(matches!(sole_label(&cands), Some(StructureLabel::L5)));
    }

    #[test]
    fn run_empty_classified_yields_no_bounds() {
        assert!(run(&[]).is_empty());
    }

    #[test]
    fn run_no_l5_l3_anchor_yields_no_bounds() {
        let classified = vec![
            cmw_with_labels(
                100.0, 110.0, MonowaveDirection::Up,
                vec![(StructureLabel::Five, Certainty::Primary)],
            ),
            cmw_with_labels(
                110.0, 105.0, MonowaveDirection::Down,
                vec![(StructureLabel::F3, Certainty::Primary)],
            ),
        ];
        assert!(run(&classified).is_empty());
    }

    #[test]
    fn run_detects_pattern_with_f3_start_l5_end() {
        // F3 → c3 → c3 → c3 → L5(5-wave Zigzag-in-Complex 或 Terminal Impulse 結構)
        // Step 3 從 L5(idx=4)往回退 3 → idx=1(c3)、idx=0(F3),idx=1 不是 anchor
        //   → 再退至 idx=0(F3,anchor!)→ wave count = 4-0+1 = 5,odd ✓ → accept
        // 預期 PatternBound { start_idx: 0, end_idx: 4, start_label: F3, end_label: L5 }
        let classified = vec![
            cmw_with_labels(
                100.0, 105.0, MonowaveDirection::Up,
                vec![(StructureLabel::F3, Certainty::Primary)],
            ), // idx=0
            cmw_with_labels(
                105.0, 102.0, MonowaveDirection::Down,
                vec![(StructureLabel::C3, Certainty::Primary)],
            ), // idx=1
            cmw_with_labels(
                102.0, 108.0, MonowaveDirection::Up,
                vec![(StructureLabel::C3, Certainty::Primary)],
            ), // idx=2
            cmw_with_labels(
                108.0, 104.0, MonowaveDirection::Down,
                vec![(StructureLabel::C3, Certainty::Primary)],
            ), // idx=3
            cmw_with_labels(
                104.0, 115.0, MonowaveDirection::Up,
                vec![(StructureLabel::L5, Certainty::Primary)],
            ), // idx=4
        ];
        let bounds = run(&classified);
        assert_eq!(bounds.len(), 1);
        let b = &bounds[0];
        assert_eq!(b.start_idx, 0);
        assert_eq!(b.end_idx, 4);
        assert!(matches!(b.start_label, StructureLabel::F3));
        assert!(matches!(b.end_label, StructureLabel::L5));
        assert!(!b.validated, "Phase 3 validated 預設 false");
    }

    #[test]
    fn special_circumstances_forced_corrective() {
        // Up pattern 起點 100,中間某 monowave end 跌破 100 → forced_corrective
        let classified = vec![
            cmw_with_labels(
                100.0, 110.0, MonowaveDirection::Up,
                vec![(StructureLabel::F3, Certainty::Primary)],
            ), // start 100, dir Up
            cmw_with_labels(
                110.0, 95.0, MonowaveDirection::Down,
                vec![(StructureLabel::C3, Certainty::Primary)],
            ), // end 95 < 100 → exceeded
            cmw_with_labels(
                95.0, 105.0, MonowaveDirection::Up,
                vec![(StructureLabel::C3, Certainty::Primary)],
            ),
            cmw_with_labels(
                105.0, 100.0, MonowaveDirection::Down,
                vec![(StructureLabel::C3, Certainty::Primary)],
            ),
            cmw_with_labels(
                100.0, 115.0, MonowaveDirection::Up,
                vec![(StructureLabel::L5, Certainty::Primary)],
            ),
        ];
        let bounds = run(&classified);
        assert_eq!(bounds.len(), 1);
        assert!(bounds[0].forced_corrective, "起點 100,idx=1 跌至 95 → 強制 :3");
    }

    // v4.7.1 G1.1 validate_after_compaction tests ------------------------

    fn cmw_at_dates(
        start_date: NaiveDate,
        end_date: NaiveDate,
        labels: Vec<(StructureLabel, Certainty)>,
    ) -> ClassifiedMonowave {
        let candidates = labels
            .into_iter()
            .map(|(label, certainty)| StructureLabelCandidate { label, certainty })
            .collect();
        ClassifiedMonowave {
            monowave: Monowave {
                start_date,
                end_date,
                start_price: 100.0,
                end_price: 110.0,
                direction: MonowaveDirection::Up,
                bar_indices: (0, 0),
            },
            atr_at_start: 1.0,
            metrics: ProportionMetrics {
                magnitude: 10.0,
                duration_bars: 5,
                atr_relative: 1.0,
                slope_vs_45deg: 1.0,
            },
            structure_label_candidates: candidates,
            polywave_size: 0,
        }
    }

    fn make_scenario_at(start_date: NaiveDate, end_date: NaiveDate) -> Scenario {
        use crate::output::{
            ComplexityLevel, NeelyPatternType, PostBehavior, PowerRating, RoundState,
            StructuralFacts, WaveNode,
        };
        Scenario {
            id: "test_scenario".to_string(),
            wave_tree: WaveNode {
                label: "test".to_string(),
                start: start_date,
                end: end_date,
                children: Vec::new(),
            },
            pattern_type: NeelyPatternType::Impulse,
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
            in_triangle_context: false,
            awaiting_l_label: false,
            monowave_structure_labels: Vec::new(),
            round_state: RoundState::Round1,
            pattern_isolation_anchors: Vec::new(),
            triplexity_detected: false,
        }
    }

    #[test]
    fn validate_after_compaction_marks_validated_when_scenario_boundary_matches() {
        // 3 classified monowaves;bound (idx 0..=2) start_date=Jan 1 / end_date=Jan 15
        let classified = vec![
            cmw_at_dates(
                NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                NaiveDate::from_ymd_opt(2026, 1, 5).unwrap(),
                vec![],
            ),
            cmw_at_dates(
                NaiveDate::from_ymd_opt(2026, 1, 6).unwrap(),
                NaiveDate::from_ymd_opt(2026, 1, 10).unwrap(),
                vec![],
            ),
            cmw_at_dates(
                NaiveDate::from_ymd_opt(2026, 1, 11).unwrap(),
                NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
                vec![],
            ),
        ];
        let mut bounds = vec![PatternBound {
            start_idx: 0,
            end_idx: 2,
            start_label: StructureLabel::F3,
            end_label: StructureLabel::L5,
            validated: false,
            forced_corrective: false,
        }];
        // Scenario 邊界對齊:start = classified[0].start_date,end = classified[2].end_date
        let scenarios = vec![make_scenario_at(
            NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
        )];
        validate_after_compaction(&mut bounds, &scenarios, &classified);
        assert!(bounds[0].validated, "scenario 邊界匹配 → validated=true");
    }

    #[test]
    fn validate_after_compaction_keeps_false_when_no_scenario_matches() {
        let classified = vec![
            cmw_at_dates(
                NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                NaiveDate::from_ymd_opt(2026, 1, 5).unwrap(),
                vec![],
            ),
            cmw_at_dates(
                NaiveDate::from_ymd_opt(2026, 1, 6).unwrap(),
                NaiveDate::from_ymd_opt(2026, 1, 10).unwrap(),
                vec![],
            ),
            cmw_at_dates(
                NaiveDate::from_ymd_opt(2026, 1, 11).unwrap(),
                NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
                vec![],
            ),
        ];
        let mut bounds = vec![PatternBound {
            start_idx: 0,
            end_idx: 2,
            start_label: StructureLabel::F3,
            end_label: StructureLabel::L5,
            validated: false,
            forced_corrective: false,
        }];
        // Scenario 邊界不對齊(end 差 1 天)
        let scenarios = vec![make_scenario_at(
            NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2026, 1, 14).unwrap(),
        )];
        validate_after_compaction(&mut bounds, &scenarios, &classified);
        assert!(!bounds[0].validated, "scenario 邊界不匹配 → validated 保 false");
    }

    #[test]
    fn validate_after_compaction_skips_out_of_range_bounds() {
        // bound.end_idx 超出 classified.len() → 跳過(保持原本 validated=false)
        let classified = vec![cmw_at_dates(
            NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2026, 1, 5).unwrap(),
            vec![],
        )];
        let mut bounds = vec![PatternBound {
            start_idx: 0,
            end_idx: 99, // 越界
            start_label: StructureLabel::F3,
            end_label: StructureLabel::L5,
            validated: false,
            forced_corrective: false,
        }];
        let scenarios = vec![make_scenario_at(
            NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2026, 1, 5).unwrap(),
        )];
        validate_after_compaction(&mut bounds, &scenarios, &classified);
        assert!(!bounds[0].validated, "out-of-range bound 不應 mutate");
    }
}
