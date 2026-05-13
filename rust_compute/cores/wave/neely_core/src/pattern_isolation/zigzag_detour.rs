// zigzag_detour.rs — Zigzag DETOUR Test
//
// 對齊 m3Spec/neely_rules.md §Zigzag DETOUR Test(1283-1285 行)
//       + §Three Rounds 教學流程(1198+ 行)
//
// **Spec 描述**:
//   找到 `:L5` 結尾的可能 Zigzag 時,先檢查其前兩個 Structure Label 能否組成更大的 Impulse;
//   若可且通過所有衝動規則,優先採衝動解讀;若衝動失敗,再回到 Zigzag。
//   DETOUR Test 是 Round 1 識別出 Zigzag Series 後進入 Round 2 之前的必要篩選步驟。
//
// **Phase 3 PR 範圍**:
//   - 對每個 wave_count == 3 candidate(可能 Zigzag),檢查 candidate 末端 monowave
//     的 structure_label_candidates 是否含 `:L5`
//   - 若 candidate 起點 ≥ 2,檢查 candidate.monowave_indices[0] 之前兩個 monowave
//     是否可組成「擴充版 5-wave 結構」(`:5-:F3-:?5-:F3-:L5` Trending Impulse 序列)
//   - 標出 DetourAnnotation,標明 candidate 與其 impulse_alternative 5-monowave 序列
//   - Stage 4 Validator 可優先以 5-wave 重跑 Ch5 Essential R1-R7,fail 才回到 Zigzag
//
// **限制**(Phase 3 best-guess):
//   - 「組成更大的 Impulse」的判定本 PR 只檢查 Structure Label 序列匹配,不跑完整 Ch5
//     Validator;完整版需 P5(Stage 5 Classifier + Compaction)接 polywave 嵌套後才完整

use crate::candidates::WaveCandidate;
use crate::monowave::ClassifiedMonowave;
use crate::output::{DetourAnnotation, StructureLabel, StructureLabelCandidate};

/// Trending Impulse Standard Series 對應的 5-step Structure 序列
/// 對齊 neely_rules.md §Figure 4-3(1143-1147 行):
///   :5 - :F3 - :?5 - :F3 - :L5
/// 其中 `:?5` 為 `:F5` 或 `:L5`(未定)
const TRENDING_IMPULSE_SEQUENCE: [StructureLabel; 5] = [
    StructureLabel::Five,
    StructureLabel::F3,
    StructureLabel::UnknownFive, // 或 F5 / L5
    StructureLabel::F3,
    StructureLabel::L5,
];

/// 對每個 wave_count == 3 candidate 跑 DETOUR Test
pub fn test(
    candidates: &[WaveCandidate],
    classified: &[ClassifiedMonowave],
) -> Vec<DetourAnnotation> {
    let mut annotations = Vec::new();
    for candidate in candidates {
        if candidate.wave_count != 3 {
            continue;
        }
        let mi = &candidate.monowave_indices;
        if mi.len() != 3 {
            continue;
        }

        // Step 1:確認末端是 :L5(或 candidate 末端 monowave 含 :L5)
        let end_mw_idx = mi[2];
        if end_mw_idx >= classified.len() {
            continue;
        }
        let end_labels = &classified[end_mw_idx].structure_label_candidates;
        if !has_label_anywhere(end_labels, StructureLabel::L5) {
            continue;
        }

        // Step 2:檢查 candidate 起點之前是否有 2 個 monowaves 可作 5-wave 起手(Five + F3)
        let start_mw_idx = mi[0];
        if start_mw_idx < 2 {
            continue;
        }
        let impulse_start_idx = start_mw_idx - 2;
        // 5-wave 候選序列:[impulse_start, impulse_start+1, ..., end_mw_idx]
        let impulse_seq: Vec<usize> = (impulse_start_idx..=end_mw_idx).collect();
        if impulse_seq.len() != 5 {
            continue;
        }

        // Step 3:比對 Trending Impulse Structure 序列
        if matches_trending_impulse_sequence(classified, &impulse_seq) {
            annotations.push(DetourAnnotation {
                candidate_id: candidate.id.clone(),
                impulse_alternative: Some(impulse_seq),
            });
        }
    }
    annotations
}

/// 序列中 5 個 monowave 的 Structure Labels 是否匹配 Trending Impulse Series
/// `:5 - :F3 - :?5 - :F3 - :L5`
fn matches_trending_impulse_sequence(
    classified: &[ClassifiedMonowave],
    seq: &[usize],
) -> bool {
    if seq.len() != 5 {
        return false;
    }
    for (i, &mw_idx) in seq.iter().enumerate() {
        if mw_idx >= classified.len() {
            return false;
        }
        let labels = &classified[mw_idx].structure_label_candidates;
        let expected = TRENDING_IMPULSE_SEQUENCE[i];

        let matched = match expected {
            StructureLabel::UnknownFive => {
                // :?5 接受 F5、L5、UnknownFive
                has_label_anywhere(labels, StructureLabel::F5)
                    || has_label_anywhere(labels, StructureLabel::L5)
                    || has_label_anywhere(labels, StructureLabel::UnknownFive)
            }
            _ => has_label_anywhere(labels, expected),
        };
        if !matched {
            return false;
        }
    }
    true
}

/// 任一 candidate 為指定 label(無論 certainty)
fn has_label_anywhere(candidates: &[StructureLabelCandidate], label: StructureLabel) -> bool {
    candidates.iter().any(|c| c.label == label)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::candidates::WaveCandidate;
    use crate::monowave::ProportionMetrics;
    use crate::output::{Certainty, Monowave, MonowaveDirection, StructureLabelCandidate};
    use chrono::NaiveDate;

    fn cmw_with_labels(labels: Vec<StructureLabel>) -> ClassifiedMonowave {
        let candidates = labels
            .into_iter()
            .map(|label| StructureLabelCandidate {
                label,
                certainty: Certainty::Primary,
            })
            .collect();
        ClassifiedMonowave {
            monowave: Monowave {
                start_date: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                end_date: NaiveDate::from_ymd_opt(2026, 1, 5).unwrap(),
                start_price: 100.0,
                end_price: 110.0,
                direction: MonowaveDirection::Up,
            },
            atr_at_start: 1.0,
            metrics: ProportionMetrics {
                magnitude: 10.0,
                duration_bars: 5,
                atr_relative: 1.0,
                slope_vs_45deg: 1.0,
            },
            structure_label_candidates: candidates,
        }
    }

    fn make_3wave_candidate_ending_at(start: usize) -> WaveCandidate {
        WaveCandidate {
            id: format!("c3-mw{}-mw{}", start, start + 2),
            monowave_indices: vec![start, start + 1, start + 2],
            wave_count: 3,
            initial_direction: MonowaveDirection::Up,
        }
    }

    #[test]
    fn test_empty_yields_no_annotations() {
        assert!(test(&[], &[]).is_empty());
    }

    #[test]
    fn test_non_3wave_candidates_skipped() {
        let classified = vec![cmw_with_labels(vec![StructureLabel::L5])];
        let candidate = WaveCandidate {
            id: "c5-test".to_string(),
            monowave_indices: vec![0],
            wave_count: 5,
            initial_direction: MonowaveDirection::Up,
        };
        assert!(test(&[candidate], &classified).is_empty());
    }

    #[test]
    fn test_zigzag_at_start_without_prior_monowaves_skipped() {
        // candidate 起點 idx=0,無前兩個 monowaves → 不能形成 5-wave alternative
        let classified = vec![
            cmw_with_labels(vec![StructureLabel::Five]),
            cmw_with_labels(vec![StructureLabel::F3]),
            cmw_with_labels(vec![StructureLabel::L5]),
        ];
        let candidate = make_3wave_candidate_ending_at(0);
        assert!(test(&[candidate], &classified).is_empty());
    }

    #[test]
    fn test_zigzag_with_matching_trending_impulse_prior_emits_annotation() {
        // 5 個 monowaves:[Five, F3, UnknownFive 或 L5, F3, L5]
        //   candidate 是 wave_count=3 [idx 2, 3, 4]
        //   candidate 起點 2,需 monowaves at 0,1 前置:
        //     - idx 0:Five (對應 Trending Impulse 第 1 個 :5)
        //     - idx 1:F3 (對應 Trending Impulse 第 2 個 :F3)
        //     - idx 2:UnknownFive (對應 :?5)
        //     - idx 3:F3 (對應 :F3)
        //     - idx 4:L5 (對應 :L5)
        let classified = vec![
            cmw_with_labels(vec![StructureLabel::Five]),
            cmw_with_labels(vec![StructureLabel::F3]),
            cmw_with_labels(vec![StructureLabel::UnknownFive, StructureLabel::F5]),
            cmw_with_labels(vec![StructureLabel::F3]),
            cmw_with_labels(vec![StructureLabel::L5]),
        ];
        let candidate = make_3wave_candidate_ending_at(2);
        let annotations = test(&[candidate], &classified);
        assert_eq!(annotations.len(), 1);
        assert_eq!(annotations[0].impulse_alternative.as_ref().unwrap().len(), 5);
        assert_eq!(annotations[0].impulse_alternative.as_ref().unwrap()[0], 0);
        assert_eq!(annotations[0].impulse_alternative.as_ref().unwrap()[4], 4);
    }

    #[test]
    fn test_zigzag_no_l5_at_end_skipped() {
        // candidate 末端不含 L5 → 不視為 Zigzag(spec DETOUR 只對 :L5-ending Zigzag)
        let classified = vec![
            cmw_with_labels(vec![StructureLabel::Five]),
            cmw_with_labels(vec![StructureLabel::F3]),
            cmw_with_labels(vec![StructureLabel::Five]),
            cmw_with_labels(vec![StructureLabel::F3]),
            cmw_with_labels(vec![StructureLabel::F5]), // 末端 F5,非 L5
        ];
        let candidate = make_3wave_candidate_ending_at(2);
        assert!(test(&[candidate], &classified).is_empty());
    }
}
