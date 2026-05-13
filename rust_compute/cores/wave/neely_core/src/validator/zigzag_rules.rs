// zigzag_rules.rs — Validator Ch5 Zigzag 子規則
//
// 對齊 m3Spec/neely_rules.md §Zigzags (5-3-5)(1477-1506 行)
//       + m3Spec/neely_core_architecture.md §9.3
//
// **Ch5 Zigzag 規則**(spec 1478-1481):
//   1. wave-a 不應回測「上一級衝動波」超過 61.8%(本層不檢,需 polywave 嵌套)
//   2. wave-b ≤ 61.8% × wave-a(上限,無下限)→ Ch5_Zigzag_Max_BRetracement
//   3. wave-c 為衝動(:5);長度依 Normal / Elongated / Truncated 變體不同
//      → Ch5_Zigzag_C_TriangleException 處理 c-wave Triangle 例外
//
// **r5 修補**(spec 1483-1485):
//   - wave-b 沒有「至少回測 X%」下限要求(原書無此規定,r5 刪除 v1.8 的 1% 下限)
//   - wave-c 不必超越 wave-a 終點(Prechter 派規則,Neely 派允許 Truncated Zigzag)

use super::RuleResult;
use crate::candidates::WaveCandidate;
use crate::monowave::ClassifiedMonowave;
use crate::output::{RuleId, RuleRejection, StructureLabel};

/// Zigzag wave-b 上限:0.618 × wave-a(Fibonacci),Phase 4 容差 ±4%
const ZIGZAG_MAX_B_RATIO: f64 = 0.618;
const FIB_TOL: f64 = 0.04;

pub fn run(
    candidate: &WaveCandidate,
    classified: &[ClassifiedMonowave],
) -> Vec<RuleResult> {
    vec![
        rule_zigzag_max_b_retracement(candidate, classified),
        rule_zigzag_c_triangle_exception(candidate, classified),
    ]
}

/// Zigzag wave-b ≤ 61.8% × wave-a(上限規則)。
fn rule_zigzag_max_b_retracement(
    candidate: &WaveCandidate,
    classified: &[ClassifiedMonowave],
) -> RuleResult {
    let rid = RuleId::Ch5_Zigzag_Max_BRetracement;
    if candidate.wave_count != 3 || candidate.monowave_indices.len() < 3 {
        return RuleResult::NotApplicable(rid);
    }
    let mi = &candidate.monowave_indices;
    let mag_a = magnitude(classified, mi[0]);
    let mag_b = magnitude(classified, mi[1]);
    if mag_a < 1e-12 {
        return RuleResult::NotApplicable(rid);
    }
    let ratio = mag_b / mag_a;
    let max_allowed = ZIGZAG_MAX_B_RATIO * (1.0 + FIB_TOL);

    if ratio <= max_allowed {
        RuleResult::Pass
    } else {
        RuleResult::Fail(RuleRejection {
            candidate_id: candidate.id.clone(),
            rule_id: rid,
            expected: format!(
                "Zigzag wave-b 須 ≤ 61.8% × wave-a(±4% 容差,上限 {:.4}),實際 {:.4}",
                max_allowed, ratio
            ),
            actual: format!("b/a = {:.4}(b 過深,可能不是 Zigzag)", ratio),
            gap: (ratio - ZIGZAG_MAX_B_RATIO) * 100.0,
            neely_page: "neely_rules.md §Zigzags 1480 行".to_string(),
        })
    }
}

/// Zigzag c-wave Triangle 例外:Zigzag 的 c-wave 通常為 :5(impulse),
/// 但在 Triangle 內可以是 :3(Triangle correction)。
///
/// 本規則檢查 candidate[2](c-wave 對應 monowave)的 structure_label_candidates
/// 是否含 :5 或 :3 之一;若兩者皆無,該 candidate 不適合 Zigzag 詮釋。
fn rule_zigzag_c_triangle_exception(
    candidate: &WaveCandidate,
    classified: &[ClassifiedMonowave],
) -> RuleResult {
    let rid = RuleId::Ch5_Zigzag_C_TriangleException;
    if candidate.wave_count != 3 || candidate.monowave_indices.len() < 3 {
        return RuleResult::NotApplicable(rid);
    }
    let mi = &candidate.monowave_indices;
    let c_labels = &classified[mi[2]].structure_label_candidates;

    let has_five = c_labels.iter().any(|c| {
        matches!(
            c.label,
            StructureLabel::Five | StructureLabel::F5 | StructureLabel::L5 | StructureLabel::S5
        )
    });
    let has_three = c_labels.iter().any(|c| {
        matches!(
            c.label,
            StructureLabel::Three
                | StructureLabel::F3
                | StructureLabel::C3
                | StructureLabel::L3
                | StructureLabel::SL3
        )
    });

    if has_five || has_three {
        RuleResult::Pass
    } else {
        // c-wave 無任何 :5 或 :3 標記 → Stage 0 未跑,或結構不符 Zigzag
        RuleResult::Fail(RuleRejection {
            candidate_id: candidate.id.clone(),
            rule_id: rid,
            expected: "Zigzag c-wave 須含 :5(Normal/Elongated)或 :3(Triangle 例外)Structure".to_string(),
            actual: format!("c-wave (mw{}) 無 :5/:3 標記", mi[2]),
            gap: 0.0,
            neely_page: "neely_rules.md §Zigzags 1481 行 + Truncated Zigzag 1492 行".to_string(),
        })
    }
}

#[inline]
fn magnitude(classified: &[ClassifiedMonowave], idx: usize) -> f64 {
    classified[idx].metrics.magnitude
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::monowave::ProportionMetrics;
    use crate::output::{Certainty, Monowave, MonowaveDirection, StructureLabelCandidate};
    use chrono::NaiveDate;

    fn cmw_with_label(mag: f64, label: Option<StructureLabel>) -> ClassifiedMonowave {
        let candidates = match label {
            Some(l) => vec![StructureLabelCandidate {
                label: l,
                certainty: Certainty::Primary,
            }],
            None => Vec::new(),
        };
        ClassifiedMonowave {
            monowave: Monowave {
                start_date: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                end_date: NaiveDate::from_ymd_opt(2026, 1, 5).unwrap(),
                start_price: 100.0,
                end_price: 100.0 + mag,
                direction: MonowaveDirection::Up,
            },
            atr_at_start: 1.0,
            metrics: ProportionMetrics {
                magnitude: mag,
                duration_bars: 5,
                atr_relative: 1.0,
                slope_vs_45deg: 1.0,
            },
            structure_label_candidates: candidates,
        }
    }

    fn make_3wave(a: f64, b: f64, c: f64, c_label: Option<StructureLabel>) -> (Vec<ClassifiedMonowave>, WaveCandidate) {
        let classified = vec![
            cmw_with_label(a, None),
            cmw_with_label(b, None),
            cmw_with_label(c, c_label),
        ];
        let candidate = WaveCandidate {
            id: "c3-test".to_string(),
            monowave_indices: vec![0, 1, 2],
            wave_count: 3,
            initial_direction: MonowaveDirection::Up,
        };
        (classified, candidate)
    }

    #[test]
    fn zigzag_b_passes_when_b_le_618() {
        // b/a = 0.5 ≤ 0.618
        let (classified, candidate) = make_3wave(10.0, 5.0, 8.0, Some(StructureLabel::Five));
        assert!(rule_zigzag_max_b_retracement(&candidate, &classified).is_pass());
    }

    #[test]
    fn zigzag_b_fails_when_b_too_deep() {
        // b/a = 0.9 > 0.618 + 0.04 = 0.658
        let (classified, candidate) = make_3wave(10.0, 9.0, 8.0, Some(StructureLabel::Five));
        assert!(rule_zigzag_max_b_retracement(&candidate, &classified).is_fail());
    }

    #[test]
    fn zigzag_c_passes_when_c_has_five_label() {
        let (classified, candidate) = make_3wave(10.0, 5.0, 8.0, Some(StructureLabel::L5));
        assert!(rule_zigzag_c_triangle_exception(&candidate, &classified).is_pass());
    }

    #[test]
    fn zigzag_c_passes_when_c_has_three_label_triangle_exception() {
        let (classified, candidate) = make_3wave(10.0, 5.0, 8.0, Some(StructureLabel::C3));
        assert!(rule_zigzag_c_triangle_exception(&candidate, &classified).is_pass());
    }

    #[test]
    fn zigzag_c_fails_when_c_has_no_5_or_3_label() {
        // candidate without any label
        let (classified, candidate) = make_3wave(10.0, 5.0, 8.0, None);
        assert!(rule_zigzag_c_triangle_exception(&candidate, &classified).is_fail());
    }
}
