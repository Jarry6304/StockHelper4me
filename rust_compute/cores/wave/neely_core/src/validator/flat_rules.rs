// flat_rules.rs — Validator Ch5 Flat 子規則
//
// 對齊 m3Spec/neely_rules.md §Flats (3-3-5) 最小建構條件(1425-1475 行)
//       + m3Spec/neely_core_architecture.md §9.3
//
// **Ch5 Flat 最小要求**(spec 1426-1427):
//   1. wave-b 至少回測 61.8% × wave-a  → Ch5_Flat_Min_BRatio
//   2. wave-c 至少 38.2% × wave-a      → Ch5_Flat_Min_CRatio
//
// **適用**:wave_count == 3 candidate(a-b-c 結構)
// **NotApplicable**:wave_count != 3,或 candidate magnitude 不適合 Flat 詮釋

use super::RuleResult;
use crate::candidates::WaveCandidate;
use crate::monowave::ClassifiedMonowave;
use crate::output::{RuleId, RuleRejection};

/// Flat min b/a ratio:0.618(Fibonacci),Phase 4 容差 ±4%(architecture §4.2)
const FLAT_MIN_B_RATIO: f64 = 0.618;
/// Flat min c/a ratio:0.382(Fibonacci),Phase 4 容差 ±4%
const FLAT_MIN_C_RATIO: f64 = 0.382;
/// Fibonacci 容差(architecture §4.2)
const FIB_TOL: f64 = 0.04;

pub fn run(
    candidate: &WaveCandidate,
    classified: &[ClassifiedMonowave],
) -> Vec<RuleResult> {
    vec![
        rule_flat_min_b_ratio(candidate, classified),
        rule_flat_min_c_ratio(candidate, classified),
    ]
}

/// Flat wave-b 至少回測 61.8% × wave-a。
fn rule_flat_min_b_ratio(
    candidate: &WaveCandidate,
    classified: &[ClassifiedMonowave],
) -> RuleResult {
    let rid = RuleId::Ch5_Flat_Min_BRatio;
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
    let min_required = FLAT_MIN_B_RATIO * (1.0 - FIB_TOL);

    if ratio >= min_required {
        RuleResult::Pass
    } else {
        RuleResult::Fail(RuleRejection {
            candidate_id: candidate.id.clone(),
            rule_id: rid,
            expected: format!(
                "Flat wave-b 須 ≥ 61.8% × wave-a(±4% 容差,下限 {:.4}),實際 {:.4}",
                min_required, ratio
            ),
            actual: format!("b/a = {:.4}", ratio),
            gap: (FLAT_MIN_B_RATIO - ratio) * 100.0,
            neely_page: "neely_rules.md §Flats 最小建構條件 1426 行".to_string(),
        })
    }
}

/// Flat wave-c 至少 38.2% × wave-a。
fn rule_flat_min_c_ratio(
    candidate: &WaveCandidate,
    classified: &[ClassifiedMonowave],
) -> RuleResult {
    let rid = RuleId::Ch5_Flat_Min_CRatio;
    if candidate.wave_count != 3 || candidate.monowave_indices.len() < 3 {
        return RuleResult::NotApplicable(rid);
    }
    let mi = &candidate.monowave_indices;
    let mag_a = magnitude(classified, mi[0]);
    let mag_c = magnitude(classified, mi[2]);
    if mag_a < 1e-12 {
        return RuleResult::NotApplicable(rid);
    }
    let ratio = mag_c / mag_a;
    let min_required = FLAT_MIN_C_RATIO * (1.0 - FIB_TOL);

    if ratio >= min_required {
        RuleResult::Pass
    } else {
        RuleResult::Fail(RuleRejection {
            candidate_id: candidate.id.clone(),
            rule_id: rid,
            expected: format!(
                "Flat wave-c 須 ≥ 38.2% × wave-a(±4% 容差,下限 {:.4}),實際 {:.4}",
                min_required, ratio
            ),
            actual: format!("c/a = {:.4}", ratio),
            gap: (FLAT_MIN_C_RATIO - ratio) * 100.0,
            neely_page: "neely_rules.md §Flats 最小建構條件 1427 行".to_string(),
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
    use crate::output::{Monowave, MonowaveDirection};
    use chrono::NaiveDate;

    fn cmw(mag: f64) -> ClassifiedMonowave {
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
            structure_label_candidates: Vec::new(),
        }
    }

    fn make_3wave(a: f64, b: f64, c: f64) -> (Vec<ClassifiedMonowave>, WaveCandidate) {
        let classified = vec![cmw(a), cmw(b), cmw(c)];
        let candidate = WaveCandidate {
            id: "c3-test".to_string(),
            monowave_indices: vec![0, 1, 2],
            wave_count: 3,
            initial_direction: MonowaveDirection::Up,
        };
        (classified, candidate)
    }

    #[test]
    fn flat_b_min_passes_when_b_meets_618() {
        let (classified, candidate) = make_3wave(10.0, 6.5, 5.0);
        assert!(rule_flat_min_b_ratio(&candidate, &classified).is_pass());
    }

    #[test]
    fn flat_b_min_fails_when_b_too_small() {
        // b/a = 0.3 < 0.618 - 0.04 = 0.578
        let (classified, candidate) = make_3wave(10.0, 3.0, 5.0);
        assert!(rule_flat_min_b_ratio(&candidate, &classified).is_fail());
    }

    #[test]
    fn flat_c_min_passes_when_c_meets_382() {
        // c/a = 0.5 ≥ 0.382
        let (classified, candidate) = make_3wave(10.0, 7.0, 5.0);
        assert!(rule_flat_min_c_ratio(&candidate, &classified).is_pass());
    }

    #[test]
    fn flat_c_min_fails_when_c_too_small() {
        // c/a = 0.2 < 0.382 - 0.04 = 0.342
        let (classified, candidate) = make_3wave(10.0, 7.0, 2.0);
        assert!(rule_flat_min_c_ratio(&candidate, &classified).is_fail());
    }

    #[test]
    fn flat_rules_not_applicable_to_5_wave() {
        let classified = vec![cmw(10.0); 5];
        let candidate = WaveCandidate {
            id: "c5".to_string(),
            monowave_indices: vec![0, 1, 2, 3, 4],
            wave_count: 5,
            initial_direction: MonowaveDirection::Up,
        };
        let results = run(&candidate, &classified);
        assert_eq!(results.len(), 2);
        for r in &results {
            assert!(matches!(r, RuleResult::NotApplicable(_)));
        }
    }
}
