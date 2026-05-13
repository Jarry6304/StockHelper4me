// triangle_rules.rs — Validator T1-T10(Triangle 子規則,r5 §9.3)
//
// 對齊 m3Spec/neely_rules.md Ch5 + Ch11 p.11-19~30 + architecture.md TriangleVariant(9 種)。
//
// **PR-3c-2 階段(2026-05-13)**:T4-T6 通用 Ch5 Triangle 規則落地;T1-T3 / T7-T10
// 維持 Deferred(sub-variant 特定規則,PR-4b classifier 識別 sub_kind 後 dispatch)。
//
// 規則語意:
//   - T4 Ch5TriangleBRange:Triangle b/a ∈ [38.2%, 261.8%](spec line 1387)
//   - T5 Ch5TriangleLegContraction:Contracting Triangle 各段更短(b > c > d > e,
//     wave-e 為最短段;spec line 1549)
//   - T6 Ch5TriangleLegEquality5Pct:至少一對 leg 等價(a ≈ c 或 b ≈ d,±5% 容差)
//   - T1-T3:Contracting Limiting × Horizontal/Irregular/Running 變體 wave-c(留 PR-4b)
//   - T7-T9:Expanding × Horizontal/Irregular/Running 變體 wave-e(留 PR-4b)
//   - T10 Ch6TriangleExpandingNonConfirmation:Expanding 失敗模式(留 PR-4b)

use super::helpers::{magnitude, safe_pct};
use super::RuleResult;
use crate::candidates::WaveCandidate;
use crate::monowave::ClassifiedMonowave;
use crate::output::{RuleId, RuleRejection, TriangleVariant, TriangleWave};

/// T4 Triangle b-wave 範圍:38.2% × a ≤ b ≤ 261.8% × a(±4% Fib 容差)
/// 下界:38.2% - 4% = 34.2%;上界:261.8% + 4% = 265.8%
pub const TRIANGLE_B_MIN_PCT: f64 = 34.2;
pub const TRIANGLE_B_MAX_PCT: f64 = 265.8;

/// T6 leg equality 容差(±5%,§4.2 Triangle leg equality 寫死)
pub const TRIANGLE_LEG_EQ_TOLERANCE_PCT: f64 = 5.0;

/// 跑 T1-T10 對 candidate
pub fn run(
    candidate: &WaveCandidate,
    classified: &[ClassifiedMonowave],
) -> Vec<RuleResult> {
    vec![
        // T1-T3: Contracting Limiting 3 種(Horizontal / Irregular / Running)wave-c
        // 仍 Deferred — 需 classifier 識別 sub_kind 後 dispatch
        RuleResult::Deferred(RuleId::Ch11TriangleVariantRules {
            variant: TriangleVariant::HorizontalLimiting, wave: TriangleWave::C,
        }),
        RuleResult::Deferred(RuleId::Ch11TriangleVariantRules {
            variant: TriangleVariant::IrregularLimiting, wave: TriangleWave::C,
        }),
        RuleResult::Deferred(RuleId::Ch11TriangleVariantRules {
            variant: TriangleVariant::RunningLimiting, wave: TriangleWave::C,
        }),
        // T4-T6: Ch5 通用 Triangle 規則(PR-3c-2 落地實作)
        rule_t4(candidate, classified),
        rule_t5(candidate, classified),
        rule_t6(candidate, classified),
        // T7-T9: Expanding 3 種 wave-e 仍 Deferred
        RuleResult::Deferred(RuleId::Ch11TriangleVariantRules {
            variant: TriangleVariant::HorizontalExpanding, wave: TriangleWave::E,
        }),
        RuleResult::Deferred(RuleId::Ch11TriangleVariantRules {
            variant: TriangleVariant::IrregularExpanding, wave: TriangleWave::E,
        }),
        RuleResult::Deferred(RuleId::Ch11TriangleVariantRules {
            variant: TriangleVariant::RunningExpanding, wave: TriangleWave::E,
        }),
        // T10: Ch6 Expanding Non-Confirmation 仍 Deferred
        RuleResult::Deferred(RuleId::Ch6TriangleExpandingNonConfirmation),
    ]
}

// ---------------------------------------------------------------------------
// T4:Triangle b-wave 範圍(Ch5 line 1387)
//
// 適用:wave_count == 5(Triangle 為 a-b-c-d-e 5 段)
// 邏輯:
//   - b/a ratio ∈ [34.2%, 265.8%](= [38.2%-4%, 261.8%+4%])→ Pass(Triangle-consistent)
//   - 超出範圍 → NotApplicable(b 過小或過大,非 Triangle b-wave)
// ---------------------------------------------------------------------------
fn rule_t4(candidate: &WaveCandidate, classified: &[ClassifiedMonowave]) -> RuleResult {
    let rid = RuleId::Ch5TriangleBRange;
    if candidate.wave_count != 5 || candidate.monowave_indices.len() < 5 {
        return RuleResult::NotApplicable(rid);
    }
    let mi = &candidate.monowave_indices;
    let a_mag = magnitude(&classified[mi[0]]);
    let b_mag = magnitude(&classified[mi[1]]);

    let ratio_pct = match safe_pct(b_mag, a_mag) {
        Some(r) => r,
        None => return RuleResult::NotApplicable(rid),
    };

    if ratio_pct >= TRIANGLE_B_MIN_PCT && ratio_pct <= TRIANGLE_B_MAX_PCT {
        RuleResult::Pass
    } else {
        RuleResult::NotApplicable(rid)
    }
}

// ---------------------------------------------------------------------------
// T5:Contracting Triangle Leg Contraction(Ch5 line 1549)
//
// 適用:wave_count == 5 且 T4 Pass-like(b 在 Triangle 範圍)
// 邏輯(Contracting):wave-e 必為最短;d < c;e < d
// 預設檢查 Contracting(最常見變體);Expanding 規則由 T7-T9 處理(留 PR-4b)
//
// 注意:本 rule 對 Expanding Triangle 會 NotApplicable(非 Contracting)
// ---------------------------------------------------------------------------
fn rule_t5(candidate: &WaveCandidate, classified: &[ClassifiedMonowave]) -> RuleResult {
    let rid = RuleId::Ch5TriangleLegContraction;
    if candidate.wave_count != 5 || candidate.monowave_indices.len() < 5 {
        return RuleResult::NotApplicable(rid);
    }
    let mi = &candidate.monowave_indices;
    let a_mag = magnitude(&classified[mi[0]]);
    let b_mag = magnitude(&classified[mi[1]]);
    let c_mag = magnitude(&classified[mi[2]]);
    let d_mag = magnitude(&classified[mi[3]]);
    let e_mag = magnitude(&classified[mi[4]]);

    // T4 預檢:Triangle b 範圍
    let b_over_a = match safe_pct(b_mag, a_mag) {
        Some(r) => r,
        None => return RuleResult::NotApplicable(rid),
    };
    if b_over_a < TRIANGLE_B_MIN_PCT || b_over_a > TRIANGLE_B_MAX_PCT {
        return RuleResult::NotApplicable(rid);
    }

    // Contracting 條件:c < b AND d < c AND e < d(各段更短)
    // 嚴格條件易 false negative,放寬為「e 最短 AND d < c」
    let is_contracting = e_mag < d_mag && d_mag < c_mag;

    if is_contracting {
        RuleResult::Pass
    } else if e_mag >= a_mag {
        // wave-e ≥ wave-a → 看起來像 Expanding(或非 Triangle),
        // T5 NotApplicable(Expanding 規則 T7-T9 處理)
        RuleResult::NotApplicable(rid)
    } else {
        // 既非 Contracting 也非 Expanding 顯著 → Fail(結構不一致)
        let gap = if e_mag >= d_mag {
            (e_mag - d_mag) / d_mag * 100.0
        } else {
            (d_mag - c_mag) / c_mag * 100.0
        };
        RuleResult::Fail(RuleRejection {
            candidate_id: candidate.id.clone(),
            rule_id: rid,
            expected: "Contracting Triangle: e < d < c".to_string(),
            actual: format!("a={:.2} b={:.2} c={:.2} d={:.2} e={:.2}",
                a_mag, b_mag, c_mag, d_mag, e_mag),
            gap: gap.abs(),
            neely_page: "Ch5 line 1549".to_string(),
        })
    }
}

// ---------------------------------------------------------------------------
// T6:Triangle Leg Equality(±5% 容差,§4.2)
//
// 適用:wave_count == 5 且 T4 Pass-like
// 邏輯:Triangle 至少有一對 leg 大致等價(a ≈ c 或 b ≈ d 在 ±5% 內)
// 這是 Triangle 形態的辨識特徵(Ch5 + spec line 1387 「Triangle 必有 Fibonacci 關係」)
// ---------------------------------------------------------------------------
fn rule_t6(candidate: &WaveCandidate, classified: &[ClassifiedMonowave]) -> RuleResult {
    let rid = RuleId::Ch5TriangleLegEquality5Pct;
    if candidate.wave_count != 5 || candidate.monowave_indices.len() < 5 {
        return RuleResult::NotApplicable(rid);
    }
    let mi = &candidate.monowave_indices;
    let a_mag = magnitude(&classified[mi[0]]);
    let b_mag = magnitude(&classified[mi[1]]);
    let c_mag = magnitude(&classified[mi[2]]);
    let d_mag = magnitude(&classified[mi[3]]);

    // T4 預檢:Triangle b 範圍
    let b_over_a = match safe_pct(b_mag, a_mag) {
        Some(r) => r,
        None => return RuleResult::NotApplicable(rid),
    };
    if b_over_a < TRIANGLE_B_MIN_PCT || b_over_a > TRIANGLE_B_MAX_PCT {
        return RuleResult::NotApplicable(rid);
    }

    // 檢查 leg equality:|a-c|/a ≤ 5% OR |b-d|/b ≤ 5%
    let ac_gap_pct = if a_mag > 0.0 {
        (a_mag - c_mag).abs() / a_mag * 100.0
    } else {
        100.0
    };
    let bd_gap_pct = if b_mag > 0.0 {
        (b_mag - d_mag).abs() / b_mag * 100.0
    } else {
        100.0
    };

    if ac_gap_pct <= TRIANGLE_LEG_EQ_TOLERANCE_PCT
        || bd_gap_pct <= TRIANGLE_LEG_EQ_TOLERANCE_PCT
    {
        RuleResult::Pass
    } else {
        // 無 leg equality — Triangle 缺乏典型 Fibonacci 對稱
        // 不直接 Fail(其他 Fib ratios 可能存在),改 NotApplicable
        RuleResult::NotApplicable(rid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::monowave::ProportionMetrics;
    use crate::output::{Monowave, MonowaveDirection};
    use chrono::NaiveDate;

    fn cmw(start_p: f64, end_p: f64) -> ClassifiedMonowave {
        let date = NaiveDate::parse_from_str("2026-01-01", "%Y-%m-%d").unwrap();
        ClassifiedMonowave {
            monowave: Monowave {
                start_date: date,
                end_date: date,
                start_price: start_p,
                end_price: end_p,
                direction: if end_p >= start_p {
                    MonowaveDirection::Up
                } else {
                    MonowaveDirection::Down
                },
            },
            atr_at_start: 1.0,
            metrics: ProportionMetrics {
                magnitude: (end_p - start_p).abs(),
                duration_bars: 5,
                atr_relative: 5.0,
                slope_vs_45deg: 1.0,
            },
        }
    }

    fn cand_5wave() -> WaveCandidate {
        WaveCandidate {
            id: "c5-triangle".to_string(),
            monowave_indices: vec![0, 1, 2, 3, 4],
            wave_count: 5,
            initial_direction: MonowaveDirection::Up,
            wave_segment_lengths: vec![1; 5],
        }
    }

    fn cand_3wave() -> WaveCandidate {
        WaveCandidate {
            id: "c3".to_string(),
            monowave_indices: vec![0, 1, 2],
            wave_count: 3,
            initial_direction: MonowaveDirection::Up,
            wave_segment_lengths: vec![1; 3],
        }
    }

    #[test]
    fn t4_triangle_b_in_range_passes() {
        // a=10, b=6 (60%, Triangle range 38.2-261.8) → Pass
        let classified = vec![
            cmw(100.0, 110.0), cmw(110.0, 104.0), cmw(104.0, 109.0),
            cmw(109.0, 106.0), cmw(106.0, 108.0),
        ];
        assert!(rule_t4(&cand_5wave(), &classified).is_pass());
    }

    #[test]
    fn t4_b_too_small_not_applicable() {
        // a=10, b=2 (20%, < 34.2%) → NotApplicable
        let classified = vec![
            cmw(100.0, 110.0), cmw(110.0, 108.0), cmw(108.0, 109.5),
            cmw(109.5, 108.5), cmw(108.5, 109.0),
        ];
        assert!(matches!(rule_t4(&cand_5wave(), &classified), RuleResult::NotApplicable(_)));
    }

    #[test]
    fn t4_b_too_large_not_applicable() {
        // a=10, b=30 (300%, > 265.8%) → NotApplicable
        let classified = vec![
            cmw(100.0, 110.0), cmw(110.0, 80.0), cmw(80.0, 105.0),
            cmw(105.0, 90.0), cmw(90.0, 100.0),
        ];
        assert!(matches!(rule_t4(&cand_5wave(), &classified), RuleResult::NotApplicable(_)));
    }

    #[test]
    fn t4_3wave_not_applicable() {
        let classified = vec![cmw(100.0, 110.0), cmw(110.0, 104.0), cmw(104.0, 109.0)];
        assert!(matches!(rule_t4(&cand_3wave(), &classified), RuleResult::NotApplicable(_)));
    }

    #[test]
    fn t5_contracting_triangle_passes() {
        // a=10, b=8 (80%), c=6 (b>c), d=4 (c>d), e=2 (d>e) → Contracting → Pass
        let classified = vec![
            cmw(100.0, 110.0), cmw(110.0, 102.0), cmw(102.0, 108.0),
            cmw(108.0, 104.0), cmw(104.0, 106.0),
        ];
        let r = rule_t5(&cand_5wave(), &classified);
        assert!(r.is_pass(), "Contracting Triangle 應 Pass, got {:?}", r);
    }

    #[test]
    fn t5_non_contracting_fails() {
        // a=10, b=6 (60%, in range), c=4, d=5(d > c violates contraction), e=2
        // → 既非 Contracting(d ≥ c)也非 Expanding(e < a)→ Fail
        let classified = vec![
            cmw(100.0, 110.0), cmw(110.0, 104.0), cmw(104.0, 108.0),
            cmw(108.0, 103.0), cmw(103.0, 105.0),
        ];
        let r = rule_t5(&cand_5wave(), &classified);
        match r {
            RuleResult::Fail(rej) => assert!(rej.gap > 0.0),
            _ => panic!("expected Fail, got {:?}", r),
        }
    }

    #[test]
    fn t5_expanding_triangle_not_applicable() {
        // a=10, b=6 (60%, in range), e=15 (≥ a) → Expanding-like → NotApplicable
        let classified = vec![
            cmw(100.0, 110.0), cmw(110.0, 104.0), cmw(104.0, 112.0),
            cmw(112.0, 95.0), cmw(95.0, 110.0),
        ];
        assert!(matches!(rule_t5(&cand_5wave(), &classified), RuleResult::NotApplicable(_)));
    }

    #[test]
    fn t6_leg_ac_equal_passes() {
        // a=10, b=6 (60%, in range), c=10 (a ≈ c), d=5, e=3 → leg equality (a ≈ c) → Pass
        let classified = vec![
            cmw(100.0, 110.0), cmw(110.0, 104.0), cmw(104.0, 114.0),
            cmw(114.0, 109.0), cmw(109.0, 112.0),
        ];
        let r = rule_t6(&cand_5wave(), &classified);
        assert!(r.is_pass(), "a ≈ c 應 Pass, got {:?}", r);
    }

    #[test]
    fn t6_leg_bd_equal_passes() {
        // a=10, b=8 (80%, in range), c=12, d=8 (b ≈ d), e=4 → b ≈ d → Pass
        let classified = vec![
            cmw(100.0, 110.0), cmw(110.0, 102.0), cmw(102.0, 114.0),
            cmw(114.0, 106.0), cmw(106.0, 110.0),
        ];
        let r = rule_t6(&cand_5wave(), &classified);
        assert!(r.is_pass(), "b ≈ d 應 Pass, got {:?}", r);
    }

    #[test]
    fn t6_no_leg_equality_not_applicable() {
        // a=10, b=6, c=20 (a≠c far), d=15 (b≠d far) → no leg equality → NotApplicable
        let classified = vec![
            cmw(100.0, 110.0), cmw(110.0, 104.0), cmw(104.0, 124.0),
            cmw(124.0, 109.0), cmw(109.0, 119.0),
        ];
        assert!(matches!(rule_t6(&cand_5wave(), &classified), RuleResult::NotApplicable(_)));
    }

    #[test]
    fn t4_t5_t6_constants_match_spec() {
        assert_eq!(TRIANGLE_B_MIN_PCT, 34.2);
        assert_eq!(TRIANGLE_B_MAX_PCT, 265.8);
        assert_eq!(TRIANGLE_LEG_EQ_TOLERANCE_PCT, 5.0);
    }

    #[test]
    fn run_returns_10_results() {
        // run() 應回 10 條 (T1-T3 Deferred + T4-T6 active + T7-T10 Deferred)
        let classified = vec![
            cmw(100.0, 110.0), cmw(110.0, 104.0), cmw(104.0, 108.0),
            cmw(108.0, 105.0), cmw(105.0, 107.0),
        ];
        let results = run(&cand_5wave(), &classified);
        assert_eq!(results.len(), 10);

        // T1-T3 應 Deferred
        for i in 0..3 {
            assert!(results[i].is_deferred(), "T{} should be Deferred", i + 1);
        }
        // T4-T6 should be Pass / NotApplicable / Fail(not Deferred)
        for i in 3..6 {
            assert!(!results[i].is_deferred(), "T{} should not be Deferred", i + 1);
        }
        // T7-T10 Deferred
        for i in 6..10 {
            assert!(results[i].is_deferred(), "T{} should be Deferred", i + 1);
        }
    }
}
