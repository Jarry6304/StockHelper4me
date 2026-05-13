// flat_rules.rs — Validator F1-F2(Flat 子規則,r5 §9.3 + Ch5)
//
// 對齊 m3Spec/neely_rules.md Ch5 p.5-34~36 + Ch11 Flat 7 variants(r5 §9.6)。
//
// **PR-3c-1 階段(2026-05-13)**:F1-F2 落地基礎實作。
// PR-4b 將擴展為「sub_kind 細分」(7 個 FlatVariant variant)。
//
// 規則語意:
//   - F1(Ch5FlatMinBRatio):Flat b-wave 必須充分回測 a-wave(81-138.2% × a 為典型範圍)
//   - F2(Ch5FlatMinCRatio):Flat c-wave 不可過短(≥ 38.2% × b 為下界)
//
// 設計選擇(PR-3c-1):
//   - 規則用 NotApplicable 表示「此 candidate 不是 Flat」,不阻塞 overall_pass
//   - 真正 Fail 只用於「結構違反(無法描述為任何 pattern type)」
//   - 對齊 spec §10.3 deferred 暫時通過 + NotApplicable 不阻塞原則

use super::helpers::{magnitude, safe_pct};
use super::RuleResult;
use crate::candidates::WaveCandidate;
use crate::monowave::ClassifiedMonowave;
use crate::output::{RuleId, RuleRejection};

/// Flat b-wave 範圍(±4% Fibonacci 容差):
///   - 下界:61.8% × a(Weak / B-Failure)± 4 = 57.8%
///   - 上界:138.2% × a(Strong / Irregular)± 4 = 142.2%
///   - 超出上界:仍 Pass(Running Correction / Irregular Failure;r5 §9.6 RunningCorrection
///     獨立 top-level variant,> 142.2% 進該分類)
pub const FLAT_B_MIN_PCT: f64 = 57.8;

/// F2 c-wave 最小範圍(避免 c 過短):38.2% × b - 4% tolerance = 34.2%
pub const FLAT_C_MIN_PCT: f64 = 34.2;

/// 跑 F1-F2 對 candidate
pub fn run(
    candidate: &WaveCandidate,
    classified: &[ClassifiedMonowave],
) -> Vec<RuleResult> {
    vec![
        rule_f1(candidate, classified),
        rule_f2(candidate, classified),
    ]
}

// ---------------------------------------------------------------------------
// F1:Flat b-wave 回測比(Ch5 p.5-34)
//
// 適用:wave_count == 3(a-b-c correction)
// 邏輯:
//   - a = monowaves[mi[0]],b = monowaves[mi[1]]
//   - b/a × 100 ≥ FLAT_B_MIN_PCT(57.8%)→ Pass(可能是 Flat / Running Correction)
//   - b/a × 100 < FLAT_B_MIN_PCT → NotApplicable(b 太小,屬 Zigzag-like)
// ---------------------------------------------------------------------------
fn rule_f1(candidate: &WaveCandidate, classified: &[ClassifiedMonowave]) -> RuleResult {
    let rid = RuleId::Ch5FlatMinBRatio;
    if candidate.wave_count != 3 || candidate.monowave_indices.len() < 3 {
        return RuleResult::NotApplicable(rid);
    }
    let mi = &candidate.monowave_indices;
    let a_mag = magnitude(&classified[mi[0]]);
    let b_mag = magnitude(&classified[mi[1]]);

    let ratio_pct = match safe_pct(b_mag, a_mag) {
        Some(r) => r,
        None => return RuleResult::NotApplicable(rid),
    };

    if ratio_pct >= FLAT_B_MIN_PCT {
        // Flat-consistent(含 Running Correction 上限)
        RuleResult::Pass
    } else {
        // b 太小,非 Flat(可能 Zigzag)
        RuleResult::NotApplicable(rid)
    }
}

// ---------------------------------------------------------------------------
// F2:Flat c-wave 比例(Ch5 p.5-34~36)
//
// 適用:wave_count == 3 且 b/a ≥ FLAT_B_MIN_PCT(否則非 Flat,F2 N/A)
// 邏輯:
//   - b = monowaves[mi[1]],c = monowaves[mi[2]]
//   - c/b × 100 ≥ FLAT_C_MIN_PCT(34.2%)→ Pass(c 足夠長,可能 Common/C-Failure/Elongated/...)
//   - c/b × 100 < FLAT_C_MIN_PCT → Fail(c 過短,結構無效)
// ---------------------------------------------------------------------------
fn rule_f2(candidate: &WaveCandidate, classified: &[ClassifiedMonowave]) -> RuleResult {
    let rid = RuleId::Ch5FlatMinCRatio;
    if candidate.wave_count != 3 || candidate.monowave_indices.len() < 3 {
        return RuleResult::NotApplicable(rid);
    }
    let mi = &candidate.monowave_indices;
    let a_mag = magnitude(&classified[mi[0]]);
    let b_mag = magnitude(&classified[mi[1]]);
    let c_mag = magnitude(&classified[mi[2]]);

    // 先確認是 Flat 候選(b 須充分大)
    let b_over_a = match safe_pct(b_mag, a_mag) {
        Some(r) => r,
        None => return RuleResult::NotApplicable(rid),
    };
    if b_over_a < FLAT_B_MIN_PCT {
        return RuleResult::NotApplicable(rid);
    }

    let c_over_b = match safe_pct(c_mag, b_mag) {
        Some(r) => r,
        None => return RuleResult::NotApplicable(rid),
    };

    if c_over_b >= FLAT_C_MIN_PCT {
        RuleResult::Pass
    } else {
        RuleResult::Fail(RuleRejection {
            candidate_id: candidate.id.clone(),
            rule_id: rid,
            expected: format!("c ≥ {:.1}% × b(Flat 最小 c-wave 範圍)", FLAT_C_MIN_PCT),
            actual: format!("c/b = {:.1}%", c_over_b),
            gap: FLAT_C_MIN_PCT - c_over_b,
            neely_page: "Ch5 p.5-34~36".to_string(),
        })
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

    fn cand_3wave() -> WaveCandidate {
        WaveCandidate {
            id: "c3-test".to_string(),
            monowave_indices: vec![0, 1, 2],
            wave_count: 3,
            initial_direction: MonowaveDirection::Up,
        }
    }

    #[test]
    fn f1_flat_b_in_range_passes() {
        // a=10, b=8 → b/a = 80% → Flat range
        let classified = vec![cmw(100.0, 110.0), cmw(110.0, 102.0), cmw(102.0, 115.0)];
        let r = rule_f1(&cand_3wave(), &classified);
        assert!(r.is_pass(), "b/a=80% 應為 Flat (Pass),實際: {:?}", r);
    }

    #[test]
    fn f1_zigzag_b_too_small_not_applicable() {
        // a=10, b=3 → b/a = 30% → Zigzag-like, F1 NotApplicable
        let classified = vec![cmw(100.0, 110.0), cmw(110.0, 107.0), cmw(107.0, 115.0)];
        let r = rule_f1(&cand_3wave(), &classified);
        assert!(
            matches!(r, RuleResult::NotApplicable(_)),
            "b/a=30% 應 NotApplicable,實際: {:?}", r
        );
    }

    #[test]
    fn f1_running_b_above_138_passes() {
        // a=10, b=15 → b/a = 150% → Running Correction,F1 仍 Pass
        let classified = vec![cmw(100.0, 110.0), cmw(110.0, 95.0), cmw(95.0, 102.0)];
        let r = rule_f1(&cand_3wave(), &classified);
        assert!(r.is_pass(), "b/a=150% Running Correction 應 Pass,實際: {:?}", r);
    }

    #[test]
    fn f1_5wave_not_applicable() {
        let candidate = WaveCandidate {
            id: "c5".to_string(),
            monowave_indices: vec![0, 1, 2, 3, 4],
            wave_count: 5,
            initial_direction: MonowaveDirection::Up,
        };
        let classified = vec![cmw(100.0, 110.0); 5];
        let r = rule_f1(&candidate, &classified);
        assert!(matches!(r, RuleResult::NotApplicable(_)));
    }

    #[test]
    fn f2_c_too_short_fails() {
        // a=10, b=9 (90%, Flat), c=2 (c/b=22% < 34.2%) → F2 Fail
        let classified = vec![cmw(100.0, 110.0), cmw(110.0, 101.0), cmw(101.0, 103.0)];
        let r = rule_f2(&cand_3wave(), &classified);
        match r {
            RuleResult::Fail(rej) => {
                assert_eq!(rej.rule_id, RuleId::Ch5FlatMinCRatio);
                assert!(rej.gap > 0.0);
            }
            _ => panic!("expected Fail, got {:?}", r),
        }
    }

    #[test]
    fn f2_c_normal_passes() {
        // a=10, b=9 (90%, Flat), c=10 (c/b=111%, Common Flat) → F2 Pass
        let classified = vec![cmw(100.0, 110.0), cmw(110.0, 101.0), cmw(101.0, 111.0)];
        let r = rule_f2(&cand_3wave(), &classified);
        assert!(r.is_pass(), "Common Flat c=a 應 Pass,實際: {:?}", r);
    }

    #[test]
    fn f2_non_flat_candidate_not_applicable() {
        // Zigzag candidate (b/a = 30%) → F2 NotApplicable
        let classified = vec![cmw(100.0, 110.0), cmw(110.0, 107.0), cmw(107.0, 115.0)];
        let r = rule_f2(&cand_3wave(), &classified);
        assert!(matches!(r, RuleResult::NotApplicable(_)));
    }
}
