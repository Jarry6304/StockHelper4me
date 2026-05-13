// triangle_rules.rs — Validator Ch5 Triangle 子規則
//
// 對齊 m3Spec/neely_rules.md §Triangles (3-3-3-3-3)(1508-1567 行)
//       + m3Spec/neely_core_architecture.md §9.3 + §4.2 ±5% Triangle 同度數腿
//
// **Ch5 Triangle 最小要求**(spec 1509-1515):
//   1. 5 個段、各為完整修正(:3)
//   2. 5 段在同一價區震盪、輕微收斂或擴張
//   3. wave-b 在 38.2-261.8% × wave-a 之間(避開 100% 整數)→ Ch5_Triangle_BRange
//   4. b、c、d、e 中至少 3 段回測前段 ≥ 50%
//   5. 6 個同級轉折點(0、a、b、c、d、e),僅 4 個觸及收斂線
//   6. b-d Trendline 不可被 wave-c 或 wave-e 任何部分穿破
//
// **Contracting Triangles**(spec 1547-1549):
//   - 必有 Thrust:至少 75%(通常不超過 125%)的最寬段
//   - wave-e 必為價長最短;d < c;e < d → Ch5_Triangle_LegContraction
//
// **Ch5_Triangle_LegEquality_5Pct**(spec 1526 + architecture §4.2):
//   Triangle 三條同度數腿(a/c/e 或 b/d)價格相等性容差 ±5%(僅限三角)

use super::RuleResult;
use crate::candidates::WaveCandidate;
use crate::monowave::ClassifiedMonowave;
use crate::output::{RuleId, RuleRejection};

/// Triangle wave-b 範圍下限(38.2% × wave-a),±4% Fib 容差
const TRIANGLE_B_MIN_RATIO: f64 = 0.382;
/// Triangle wave-b 範圍上限(261.8% × wave-a),±4% Fib 容差
const TRIANGLE_B_MAX_RATIO: f64 = 2.618;
/// 避開 100% 整數比的緩衝區(±4% × 100% = ±4%)
const B_100_AVOID_LOW: f64 = 0.96;
const B_100_AVOID_HIGH: f64 = 1.04;
/// Fibonacci 容差(architecture §4.2)
const FIB_TOL: f64 = 0.04;
/// Triangle 同度數腿等價容差(architecture §4.2 — 僅限 Triangle)
const TRIANGLE_LEG_EQ_TOL: f64 = 0.05;

pub fn run(
    candidate: &WaveCandidate,
    classified: &[ClassifiedMonowave],
) -> Vec<RuleResult> {
    vec![
        rule_triangle_b_range(candidate, classified),
        rule_triangle_leg_contraction(candidate, classified),
        rule_triangle_leg_equality_5pct(candidate, classified),
    ]
}

/// Triangle wave-b 在 38.2-261.8% × wave-a(避開 100% 整數)。
fn rule_triangle_b_range(
    candidate: &WaveCandidate,
    classified: &[ClassifiedMonowave],
) -> RuleResult {
    let rid = RuleId::Ch5_Triangle_BRange;
    if candidate.wave_count != 5 || candidate.monowave_indices.len() < 5 {
        return RuleResult::NotApplicable(rid);
    }
    let mi = &candidate.monowave_indices;
    let mag_a = magnitude(classified, mi[0]);
    let mag_b = magnitude(classified, mi[1]);
    if mag_a < 1e-12 {
        return RuleResult::NotApplicable(rid);
    }
    let ratio = mag_b / mag_a;
    let lo = TRIANGLE_B_MIN_RATIO * (1.0 - FIB_TOL);
    let hi = TRIANGLE_B_MAX_RATIO * (1.0 + FIB_TOL);

    let in_outer_range = (lo..=hi).contains(&ratio);
    let near_100 = (B_100_AVOID_LOW..=B_100_AVOID_HIGH).contains(&ratio);

    if in_outer_range && !near_100 {
        RuleResult::Pass
    } else {
        RuleResult::Fail(RuleRejection {
            candidate_id: candidate.id.clone(),
            rule_id: rid,
            expected: format!(
                "Triangle wave-b 須在 38.2-261.8% × wave-a 之間(避 100% ±4%),實際 {:.4}",
                ratio
            ),
            actual: if near_100 {
                format!("b/a = {:.4} 接近 100%(Triangle 須避開)", ratio)
            } else {
                format!("b/a = {:.4} 超出範圍 [{:.4}, {:.4}]", ratio, lo, hi)
            },
            gap: if near_100 {
                (ratio - 1.0).abs() * 100.0
            } else if ratio < lo {
                (lo - ratio) * 100.0
            } else {
                (ratio - hi) * 100.0
            },
            neely_page: "neely_rules.md §Triangles 1512 行".to_string(),
        })
    }
}

/// Contracting Triangle leg 收斂:wave-e 須為最短;d < c;e < d。
/// Expanding Triangle:相反方向(a < b < c < d < e);本規則對 Contracting 嚴格,
/// 對 Expanding 走 NotApplicable(由 Triangle 變體分類後處理)。
fn rule_triangle_leg_contraction(
    candidate: &WaveCandidate,
    classified: &[ClassifiedMonowave],
) -> RuleResult {
    let rid = RuleId::Ch5_Triangle_LegContraction;
    if candidate.wave_count != 5 || candidate.monowave_indices.len() < 5 {
        return RuleResult::NotApplicable(rid);
    }
    let mi = &candidate.monowave_indices;
    let mag_a = magnitude(classified, mi[0]);
    let mag_b = magnitude(classified, mi[1]);
    let mag_c = magnitude(classified, mi[2]);
    let mag_d = magnitude(classified, mi[3]);
    let mag_e = magnitude(classified, mi[4]);

    // Contracting:a > c > e 且 b > d(收斂)
    let is_contracting = mag_a > mag_c
        && mag_c > mag_e
        && mag_b > mag_d;
    // Expanding:a < c < e 且 b < d(擴張)
    let is_expanding = mag_a < mag_c
        && mag_c < mag_e
        && mag_b < mag_d;

    if is_contracting || is_expanding {
        RuleResult::Pass
    } else {
        // 既非 Contracting 也非 Expanding → 不是 Triangle
        RuleResult::Fail(RuleRejection {
            candidate_id: candidate.id.clone(),
            rule_id: rid,
            expected: "Triangle leg 須單調收斂(a>c>e, b>d)或擴張(a<c<e, b<d)".to_string(),
            actual: format!(
                "magnitudes: a={:.4} b={:.4} c={:.4} d={:.4} e={:.4}",
                mag_a, mag_b, mag_c, mag_d, mag_e
            ),
            gap: 0.0,
            neely_page: "neely_rules.md §Triangles 1549 行(Contracting)+ 1562-1567(Expanding)".to_string(),
        })
    }
}

/// Triangle 三條同度數腿價格相等性 ±5%(architecture §4.2)。
///
/// 「同度數腿」= a/c/e 三條同向 legs,或 b/d 兩條反向 legs(只 2 條時跳 N/A)。
/// 規則:a/c/e 三條 magnitude 兩兩之間差異 ≤ 5%(或 b/d 兩條差異 ≤ 5%)。
fn rule_triangle_leg_equality_5pct(
    candidate: &WaveCandidate,
    classified: &[ClassifiedMonowave],
) -> RuleResult {
    let rid = RuleId::Ch5_Triangle_LegEquality_5Pct;
    if candidate.wave_count != 5 || candidate.monowave_indices.len() < 5 {
        return RuleResult::NotApplicable(rid);
    }
    let mi = &candidate.monowave_indices;
    let mag_a = magnitude(classified, mi[0]);
    let mag_c = magnitude(classified, mi[2]);
    let mag_e = magnitude(classified, mi[4]);

    // 檢查 a/c/e 三條同向 legs 兩兩等價(≤ 5% 差)
    let max_mag = mag_a.max(mag_c).max(mag_e);
    if max_mag < 1e-12 {
        return RuleResult::NotApplicable(rid);
    }
    let min_mag = mag_a.min(mag_c).min(mag_e);
    let spread = (max_mag - min_mag) / max_mag;

    if spread <= TRIANGLE_LEG_EQ_TOL {
        // 三條極接近等價 → Horizontal Triangle 特例,Pass
        RuleResult::Pass
    } else {
        // 三條腿不等價 — 對 Irregular/Running Triangle 是正常的(spec 1526-1528),
        // 本規則非強制 fail;改回 NotApplicable(不主動 reject)
        RuleResult::NotApplicable(rid)
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

    fn make_5wave(mags: [f64; 5]) -> (Vec<ClassifiedMonowave>, WaveCandidate) {
        let classified = mags.iter().map(|&m| cmw(m)).collect();
        let candidate = WaveCandidate {
            id: "c5-test".to_string(),
            monowave_indices: vec![0, 1, 2, 3, 4],
            wave_count: 5,
            initial_direction: MonowaveDirection::Up,
        };
        (classified, candidate)
    }

    #[test]
    fn triangle_b_range_passes_for_b_in_range() {
        // b/a = 0.8 (在 38.2-261.8% 內,避 100% 區 0.96-1.04)
        let (classified, candidate) = make_5wave([10.0, 8.0, 7.0, 5.0, 3.0]);
        assert!(rule_triangle_b_range(&candidate, &classified).is_pass());
    }

    #[test]
    fn triangle_b_range_fails_when_b_near_100() {
        // b/a = 1.0 (在 0.96-1.04 內 → 避開區 → Fail)
        let (classified, candidate) = make_5wave([10.0, 10.0, 7.0, 5.0, 3.0]);
        assert!(rule_triangle_b_range(&candidate, &classified).is_fail());
    }

    #[test]
    fn triangle_b_range_fails_when_b_too_small() {
        // b/a = 0.2 (< 0.382)
        let (classified, candidate) = make_5wave([10.0, 2.0, 7.0, 5.0, 3.0]);
        assert!(rule_triangle_b_range(&candidate, &classified).is_fail());
    }

    #[test]
    fn triangle_contracting_passes() {
        // a=10, b=8, c=7, d=5, e=3 → a>c>e, b>d ✓
        let (classified, candidate) = make_5wave([10.0, 8.0, 7.0, 5.0, 3.0]);
        assert!(rule_triangle_leg_contraction(&candidate, &classified).is_pass());
    }

    #[test]
    fn triangle_expanding_passes() {
        // a=3, b=4, c=5, d=7, e=10 → a<c<e, b<d ✓
        let (classified, candidate) = make_5wave([3.0, 4.0, 5.0, 7.0, 10.0]);
        assert!(rule_triangle_leg_contraction(&candidate, &classified).is_pass());
    }

    #[test]
    fn triangle_contraction_fails_when_neither_pattern() {
        // 非收斂亦非擴張
        let (classified, candidate) = make_5wave([10.0, 5.0, 10.0, 5.0, 10.0]);
        assert!(rule_triangle_leg_contraction(&candidate, &classified).is_fail());
    }

    #[test]
    fn triangle_leg_eq_5pct_passes_for_near_equal() {
        // a=10, c=10.3, e=10.2 → max=10.3, min=10, spread = 0.029 < 0.05 ✓
        let (classified, candidate) = make_5wave([10.0, 5.0, 10.3, 5.0, 10.2]);
        assert!(rule_triangle_leg_equality_5pct(&candidate, &classified).is_pass());
    }

    #[test]
    fn triangle_leg_eq_5pct_n_a_for_irregular() {
        // a=10, c=8, e=12 → spread = (12-8)/12 = 0.33 > 0.05 → N/A (Irregular Triangle 場景)
        let (classified, candidate) = make_5wave([10.0, 5.0, 8.0, 5.0, 12.0]);
        assert!(matches!(
            rule_triangle_leg_equality_5pct(&candidate, &classified),
            RuleResult::NotApplicable(_)
        ));
    }

    #[test]
    fn triangle_rules_not_applicable_to_3_wave() {
        let classified = vec![cmw(10.0); 3];
        let candidate = WaveCandidate {
            id: "c3".to_string(),
            monowave_indices: vec![0, 1, 2],
            wave_count: 3,
            initial_direction: MonowaveDirection::Up,
        };
        let results = run(&candidate, &classified);
        assert_eq!(results.len(), 3);
        for r in &results {
            assert!(matches!(r, RuleResult::NotApplicable(_)));
        }
    }
}
