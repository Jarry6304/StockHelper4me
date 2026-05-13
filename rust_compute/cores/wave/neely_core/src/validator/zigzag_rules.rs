// zigzag_rules.rs — Validator Z1-Z4(Zigzag 子規則,r5 §9.3)
//
// 對齊 m3Spec/neely_rules.md Ch5 p.5-41~42 + Ch4 p.4-15~20 + Ch11 p.11-17~18。
//
// **PR-3c-1 階段(2026-05-13)**:Z1/Z2/Z3 落地基礎實作;Z4 留 Deferred
// (需 Triangle context,PR-3c-2 提供 Triangle 判定後可解)。
//
// 規則語意:
//   - Z1(Ch5ZigzagMaxBRetracement):Zigzag b-wave ≤ 61.8% × a(v1.9 修正,刪除下限 1%)
//   - Z2(Ch11ZigzagWaveByWave wave:C):c-wave 範圍 ≥ 38.2% × a
//     (Truncated 38.2-61.8 / Normal 61.8-161.8 / Elongated > 161.8 都 valid)
//   - Z3(Ch4ZigzagDetour):DETOUR Test — 避免誤判 Impulse Wave 1-2-3 為 Zigzag
//   - Z4(Ch5ZigzagCTriangleException):Triangle 內 Zigzag c-wave 例外,需 Triangle context

use super::helpers::{magnitude, safe_pct};
use super::RuleResult;
use crate::candidates::WaveCandidate;
use crate::monowave::ClassifiedMonowave;
use crate::output::{RuleId, RuleRejection, WaveAbc};

/// Z1 上限:b/a ≤ 61.8% × a,加 ±4% Fibonacci 容差 = 65.8%
pub const ZIGZAG_B_MAX_PCT: f64 = 65.8;

/// Z2 c-wave 最小範圍:38.2% - 4% = 34.2%
pub const ZIGZAG_C_MIN_PCT: f64 = 34.2;

/// Z3 DETOUR threshold:若 c 終點價格相對於 a 起點偏離 > 3×a-magnitude 且 b 太小,
/// 看起來像 Impulse Wave 1-2-3(W3 是 W1 的延伸)而非 Zigzag。
/// 簡化版:c 終點偏離 a 起點 > 2.5×a-magnitude(經驗值,留 PR-4b 對齊 Ch4 校準)
pub const DETOUR_OVERSHOOT_RATIO: f64 = 2.5;

/// 跑 Z1-Z4 對 candidate
pub fn run(
    candidate: &WaveCandidate,
    classified: &[ClassifiedMonowave],
) -> Vec<RuleResult> {
    vec![
        rule_z1(candidate, classified),
        rule_z2(candidate, classified),
        rule_z3(candidate, classified),
        rule_z4_stub(),
    ]
}

// ---------------------------------------------------------------------------
// Z1:Zigzag b-wave ≤ 61.8% × a(Ch5 p.5-41 v1.9 修正)
//
// 適用:wave_count == 3
// 邏輯:
//   - b/a ≤ ZIGZAG_B_MAX_PCT(65.8% = 61.8% + 4% 容差)→ Pass(Zigzag-consistent)
//   - b/a > ZIGZAG_B_MAX_PCT → NotApplicable(b 過大,非 Zigzag,可能 Flat)
// ---------------------------------------------------------------------------
fn rule_z1(candidate: &WaveCandidate, classified: &[ClassifiedMonowave]) -> RuleResult {
    let rid = RuleId::Ch5ZigzagMaxBRetracement;
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

    if ratio_pct <= ZIGZAG_B_MAX_PCT {
        RuleResult::Pass
    } else {
        RuleResult::NotApplicable(rid)
    }
}

// ---------------------------------------------------------------------------
// Z2:Zigzag c-wave 範圍(Ch11 p.11-17 + Ch5 p.5-41~42)
//
// 適用:wave_count == 3 且 Zigzag-consistent(b/a ≤ 65.8%)
// 邏輯:
//   - c/a ≥ ZIGZAG_C_MIN_PCT(34.2% = 38.2% - 4% 容差)→ Pass
//     涵蓋 Truncated(38.2-61.8) / Normal(61.8-161.8) / Elongated(>161.8)
//   - c/a < ZIGZAG_C_MIN_PCT → Fail(c 過短,結構無效)
// ---------------------------------------------------------------------------
fn rule_z2(candidate: &WaveCandidate, classified: &[ClassifiedMonowave]) -> RuleResult {
    let rid = RuleId::Ch11ZigzagWaveByWave { wave: WaveAbc::C };
    if candidate.wave_count != 3 || candidate.monowave_indices.len() < 3 {
        return RuleResult::NotApplicable(rid);
    }
    let mi = &candidate.monowave_indices;
    let a_mag = magnitude(&classified[mi[0]]);
    let b_mag = magnitude(&classified[mi[1]]);
    let c_mag = magnitude(&classified[mi[2]]);

    // 先確認是 Zigzag 候選(b 須 ≤ 65.8% × a)
    let b_over_a = match safe_pct(b_mag, a_mag) {
        Some(r) => r,
        None => return RuleResult::NotApplicable(rid),
    };
    if b_over_a > ZIGZAG_B_MAX_PCT {
        return RuleResult::NotApplicable(rid);
    }

    let c_over_a = match safe_pct(c_mag, a_mag) {
        Some(r) => r,
        None => return RuleResult::NotApplicable(rid),
    };

    if c_over_a >= ZIGZAG_C_MIN_PCT {
        RuleResult::Pass
    } else {
        RuleResult::Fail(RuleRejection {
            candidate_id: candidate.id.clone(),
            rule_id: rid,
            expected: format!("c ≥ {:.1}% × a(Zigzag c-wave 下界)", ZIGZAG_C_MIN_PCT),
            actual: format!("c/a = {:.1}%", c_over_a),
            gap: ZIGZAG_C_MIN_PCT - c_over_a,
            neely_page: "Ch5 p.5-41~42 + Ch11 p.11-17".to_string(),
        })
    }
}

// ---------------------------------------------------------------------------
// Z3:DETOUR Test(Ch4 p.4-15~20)
//
// 適用:wave_count == 3
// 邏輯:防止誤把 Impulse Wave 1-2-3 認成 Zigzag a-b-c。
//   - 若 c 終點相對 a 起點偏離 > 2.5×a-magnitude(過度延伸)
//     且 b-wave 過小(b/a < 38.2%,看起來像 Impulse W2 淺回測)
//     → NotApplicable(看起來像 Impulse W1-W2-W3,非 Zigzag)
//   - 否則 Pass
//
// 簡化版:留 PR-4b 對齊 Ch4 校準完整 DETOUR Test 細節
// ---------------------------------------------------------------------------
fn rule_z3(candidate: &WaveCandidate, classified: &[ClassifiedMonowave]) -> RuleResult {
    let rid = RuleId::Ch4ZigzagDetour;
    if candidate.wave_count != 3 || candidate.monowave_indices.len() < 3 {
        return RuleResult::NotApplicable(rid);
    }
    let mi = &candidate.monowave_indices;
    let a = &classified[mi[0]].monowave;
    let c = &classified[mi[2]].monowave;
    let a_mag = magnitude(&classified[mi[0]]);
    let b_mag = magnitude(&classified[mi[1]]);

    if a_mag <= 0.0 {
        return RuleResult::NotApplicable(rid);
    }

    // 計算 c 終點相對 a 起點的「越過 a 終點」距離
    // a 是 Up:c 終點越過 a 終點越遠 → 越像 Impulse W3
    // a 是 Down:同理(對稱)
    let overshoot_distance = (c.end_price - a.start_price).abs();
    let overshoot_ratio = overshoot_distance / a_mag;

    let b_over_a = safe_pct(b_mag, a_mag).unwrap_or(100.0);

    // DETOUR fail 條件:overshoot > 2.5x AND b < 38.2% (Impulse W2 淺回測 pattern)
    if overshoot_ratio > DETOUR_OVERSHOOT_RATIO && b_over_a < 38.2 {
        RuleResult::NotApplicable(rid) // 看起來像 Impulse,Zigzag 不適用
    } else {
        RuleResult::Pass
    }
}

// ---------------------------------------------------------------------------
// Z4:Triangle 內 Zigzag c-wave 例外(留 PR-3c-2 / PR-4b)
//
// 需 Triangle context(知道 candidate 是否在 Triangle 內),validator 階段沒有
// 這個資訊。PR-3c-2 Triangle 規則落地後,classifier 可重新評估。
// ---------------------------------------------------------------------------
fn rule_z4_stub() -> RuleResult {
    RuleResult::Deferred(RuleId::Ch5ZigzagCTriangleException)
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
    fn z1_zigzag_b_small_passes() {
        // a=10, b=3 → b/a = 30% ≤ 65.8% → Pass
        let classified = vec![cmw(100.0, 110.0), cmw(110.0, 107.0), cmw(107.0, 115.0)];
        assert!(rule_z1(&cand_3wave(), &classified).is_pass());
    }

    #[test]
    fn z1_zigzag_b_at_618_boundary_passes() {
        // a=10, b=6.5 → b/a = 65% ≤ 65.8% → Pass(剛好邊界內)
        let classified = vec![cmw(100.0, 110.0), cmw(110.0, 103.5), cmw(103.5, 115.0)];
        assert!(rule_z1(&cand_3wave(), &classified).is_pass());
    }

    #[test]
    fn z1_flat_b_too_big_not_applicable() {
        // a=10, b=8 → b/a = 80% > 65.8% → NotApplicable(Flat range)
        let classified = vec![cmw(100.0, 110.0), cmw(110.0, 102.0), cmw(102.0, 115.0)];
        let r = rule_z1(&cand_3wave(), &classified);
        assert!(matches!(r, RuleResult::NotApplicable(_)));
    }

    #[test]
    fn z1_5wave_not_applicable() {
        let candidate = WaveCandidate {
            id: "c5".to_string(),
            monowave_indices: vec![0, 1, 2, 3, 4],
            wave_count: 5,
            initial_direction: MonowaveDirection::Up,
        };
        let classified = vec![cmw(100.0, 110.0); 5];
        assert!(matches!(rule_z1(&candidate, &classified), RuleResult::NotApplicable(_)));
    }

    #[test]
    fn z2_normal_zigzag_c_passes() {
        // a=10, b=4(40%, Zigzag), c=8(80%) → Normal Zigzag → Pass
        let classified = vec![cmw(100.0, 110.0), cmw(110.0, 106.0), cmw(106.0, 114.0)];
        assert!(rule_z2(&cand_3wave(), &classified).is_pass());
    }

    #[test]
    fn z2_truncated_zigzag_c_passes() {
        // a=10, b=4, c=5(c/a=50%, Truncated 38.2-61.8) → Pass
        let classified = vec![cmw(100.0, 110.0), cmw(110.0, 106.0), cmw(106.0, 111.0)];
        assert!(rule_z2(&cand_3wave(), &classified).is_pass());
    }

    #[test]
    fn z2_elongated_zigzag_c_passes() {
        // a=10, b=4, c=20(200%, Elongated > 161.8) → Pass
        let classified = vec![cmw(100.0, 110.0), cmw(110.0, 106.0), cmw(106.0, 126.0)];
        assert!(rule_z2(&cand_3wave(), &classified).is_pass());
    }

    #[test]
    fn z2_c_too_short_fails() {
        // a=10, b=4(Zigzag), c=3(c/a=30% < 34.2%) → Fail
        let classified = vec![cmw(100.0, 110.0), cmw(110.0, 106.0), cmw(106.0, 109.0)];
        let r = rule_z2(&cand_3wave(), &classified);
        match r {
            RuleResult::Fail(rej) => {
                assert!(rej.gap > 0.0);
            }
            _ => panic!("expected Fail, got {:?}", r),
        }
    }

    #[test]
    fn z2_flat_candidate_not_applicable() {
        // a=10, b=8(80%, Flat-like) → Z2 NotApplicable(不是 Zigzag)
        let classified = vec![cmw(100.0, 110.0), cmw(110.0, 102.0), cmw(102.0, 115.0)];
        assert!(matches!(rule_z2(&cand_3wave(), &classified), RuleResult::NotApplicable(_)));
    }

    #[test]
    fn z3_normal_zigzag_passes() {
        // 正常 Zigzag,DETOUR 不觸發 → Pass
        let classified = vec![cmw(100.0, 110.0), cmw(110.0, 106.0), cmw(106.0, 114.0)];
        assert!(rule_z3(&cand_3wave(), &classified).is_pass());
    }

    #[test]
    fn z3_impulse_like_detour_not_applicable() {
        // a=10, b=2(b/a=20% < 38.2%, 淺回測), c=30(overshoot ratio = (130-100)/10 = 3.0 > 2.5)
        // → 看起來像 Impulse W1-W2-W3 → NotApplicable
        let classified = vec![cmw(100.0, 110.0), cmw(110.0, 108.0), cmw(108.0, 138.0)];
        let r = rule_z3(&cand_3wave(), &classified);
        assert!(matches!(r, RuleResult::NotApplicable(_)),
            "Impulse-like overshoot + 淺 b 應 NotApplicable Z3, got {:?}", r);
    }

    #[test]
    fn z3_5wave_not_applicable() {
        let candidate = WaveCandidate {
            id: "c5".to_string(),
            monowave_indices: vec![0, 1, 2, 3, 4],
            wave_count: 5,
            initial_direction: MonowaveDirection::Up,
        };
        let classified = vec![cmw(100.0, 110.0); 5];
        assert!(matches!(rule_z3(&candidate, &classified), RuleResult::NotApplicable(_)));
    }

    #[test]
    fn z4_always_deferred_pr_3c_1() {
        let result = rule_z4_stub();
        assert!(matches!(result, RuleResult::Deferred(RuleId::Ch5ZigzagCTriangleException)));
    }
}
