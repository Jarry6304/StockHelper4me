// core_rules.rs — Validator R1-R7(通用核心規則)
//
// 對齊 m2Spec/oldm2Spec/neely_core.md §十(規則組)。
//
// **M3 PR-3b 階段**:
//   - R1/R2/R3 完整實作(Elliott Wave 教科書通用規則,跨派系一致性高)
//   - R4-R7 回 Deferred — 具體門檻 oldm2Spec/ §10.1 寫「P0 開發時逐條建檔」
//     沒列細節,等 user 在 m3Spec/ 寫最新 neely 版本後 batch 補
//
// 規則摘要(best-guess based on Elliott Wave 通用,非嚴格 Neely 派系):
//   - R1: W2 不可完全回測 W1(W2 endpoint 不能跨過 W1 起點)
//   - R2: W3 不可是 W1/W3/W5 中最短
//   - R3: W4 不可重疊 W1 區間
//   - R4-R7:留後續 PR

use super::RuleResult;
use crate::candidates::WaveCandidate;
use crate::monowave::ClassifiedMonowave;
use crate::output::{MonowaveDirection, RuleId, RuleRejection};

/// 跑 R1-R7 對 candidate
pub fn run(
    candidate: &WaveCandidate,
    classified: &[ClassifiedMonowave],
) -> Vec<RuleResult> {
    vec![
        rule_r1(candidate, classified),
        rule_r2(candidate, classified),
        rule_r3(candidate, classified),
        rule_r4(candidate, classified),
        rule_r5(candidate, classified),
        rule_r6(candidate, classified),
        rule_r7(candidate, classified),
    ]
}

// ---------------------------------------------------------------------------
// R1:W2 不可完全回測 W1(nested-aware,PR-Stage3-nested 整合)
// ---------------------------------------------------------------------------
//
// 適用:wave_count >= 3
// 邏輯(用 top_level 端點):
//   - W1 起點 = top_level_start_price(0)
//   - W2 終點 = top_level_end_price(1)
//   - W1 整體 direction Up:W2 終點價格不可低於 W1 起點(W2.end >= W1.start)
//   - W1 整體 direction Down:W2 終點價格不可高於 W1 起點(W2.end <= W1.start)
//
// 違反 → Fail(附 gap = retracement % - 100%);通過 → Pass
fn rule_r1(candidate: &WaveCandidate, classified: &[ClassifiedMonowave]) -> RuleResult {
    let rid = RuleId::Ch5Essential(1);
    if candidate.wave_count < 2 || candidate.wave_segment_lengths.len() < 2 {
        return RuleResult::NotApplicable(rid);
    }

    let w1_start = candidate.top_level_start_price(0, classified);
    let w1_end = candidate.top_level_end_price(0, classified);
    let w2_end = candidate.top_level_end_price(1, classified);
    let direction = candidate.top_level_direction(0, classified);

    let violated = match direction {
        MonowaveDirection::Up => w2_end < w1_start,
        MonowaveDirection::Down => w2_end > w1_start,
        MonowaveDirection::Neutral => return RuleResult::NotApplicable(rid),
    };

    if violated {
        let w1_magnitude = (w1_end - w1_start).abs();
        let overshoot = match direction {
            MonowaveDirection::Up => w1_start - w2_end,
            MonowaveDirection::Down => w2_end - w1_start,
            _ => 0.0,
        };
        let gap_pct = if w1_magnitude > 0.0 {
            overshoot / w1_magnitude * 100.0
        } else {
            0.0
        };
        RuleResult::Fail(RuleRejection {
            candidate_id: candidate.id.clone(),
            rule_id: rid,
            expected: format!(
                "W2 endpoint {:.2} 應在 W1 起點 {:.2} 同側",
                w2_end, w1_start
            ),
            actual: format!(
                "W2 endpoint {:.2} 跨過 W1 起點 {:.2}(direction={:?})",
                w2_end, w1_start, direction
            ),
            gap: gap_pct,
            neely_page: "Ch5 Essential R1 — W2 不可完全回測 W1".to_string(),
        })
    } else {
        RuleResult::Pass
    }
}

// ---------------------------------------------------------------------------
// R2:W3 不可是 W1/W3/W5 中最短(nested-aware)
// ---------------------------------------------------------------------------
//
// 適用:wave_count == 5
// 邏輯:W3.top_level_magnitude >= min(W1, W5)
fn rule_r2(candidate: &WaveCandidate, classified: &[ClassifiedMonowave]) -> RuleResult {
    let rid = RuleId::Ch5Essential(2);
    if candidate.wave_count != 5 || candidate.wave_segment_lengths.len() < 5 {
        return RuleResult::NotApplicable(rid);
    }
    let w1_mag = candidate.top_level_magnitude(0, classified);
    let w3_mag = candidate.top_level_magnitude(2, classified);
    let w5_mag = candidate.top_level_magnitude(4, classified);
    let min_w1_w5 = w1_mag.min(w5_mag);

    if w3_mag < min_w1_w5 {
        RuleResult::Fail(RuleRejection {
            candidate_id: candidate.id.clone(),
            rule_id: rid,
            expected: format!("W3 magnitude {:.4} 須 ≥ min(W1, W5) = {:.4}", w3_mag, min_w1_w5),
            actual: format!(
                "W3 magnitude {:.4} 為最短(W1={:.4} / W5={:.4})",
                w3_mag, w1_mag, w5_mag
            ),
            gap: (min_w1_w5 - w3_mag) / min_w1_w5.max(1e-9) * 100.0,
            neely_page: "Ch5 Essential R2 — W3 不可是 actionable wave 中最短".to_string(),
        })
    } else {
        RuleResult::Pass
    }
}

// ---------------------------------------------------------------------------
// R3:W4 不可重疊 W1 區間(nested-aware)
// ---------------------------------------------------------------------------
//
// 適用:wave_count == 5
// 邏輯:
//   - W1 區間:[W1.top_level_start, W1.top_level_end]
//   - W4 終點:top_level_end_price(3)
//   - Up:W4 終點 ≥ W1 終點
//   - Down:W4 終點 ≤ W1 終點
//
// 注意:Terminal Impulse 容許 W4-W1 overlap(Neely r5 §9.6),classifier 用 R3 fail
// → TerminalImpulse 判定。
fn rule_r3(candidate: &WaveCandidate, classified: &[ClassifiedMonowave]) -> RuleResult {
    let rid = RuleId::Ch5Essential(3);
    if candidate.wave_count != 5 || candidate.wave_segment_lengths.len() < 5 {
        return RuleResult::NotApplicable(rid);
    }
    let w1_start = candidate.top_level_start_price(0, classified);
    let w1_end = candidate.top_level_end_price(0, classified);
    let w4_end = candidate.top_level_end_price(3, classified);
    let direction = candidate.top_level_direction(0, classified);

    let (violated, expected_relation) = match direction {
        MonowaveDirection::Up => (
            w4_end < w1_end,
            format!("W4 終點 {:.2} 須 ≥ W1 終點 {:.2}", w4_end, w1_end),
        ),
        MonowaveDirection::Down => (
            w4_end > w1_end,
            format!("W4 終點 {:.2} 須 ≤ W1 終點 {:.2}", w4_end, w1_end),
        ),
        MonowaveDirection::Neutral => return RuleResult::NotApplicable(rid),
    };

    if violated {
        let w1_magnitude = (w1_end - w1_start).abs();
        let overlap = (w4_end - w1_end).abs();
        let gap_pct = if w1_magnitude > 0.0 {
            overlap / w1_magnitude * 100.0
        } else {
            0.0
        };
        RuleResult::Fail(RuleRejection {
            candidate_id: candidate.id.clone(),
            rule_id: rid,
            expected: expected_relation,
            actual: format!("W4 終點 {:.2} 重疊 W1 區間(direction={:?})", w4_end, direction),
            gap: gap_pct,
            neely_page: "Ch5 Essential R3 — W4 不可重疊 W1(Terminal Impulse 容許)".to_string(),
        })
    } else {
        RuleResult::Pass
    }
}

// ---------------------------------------------------------------------------
// R4-R7:Ch3 Pre-Constructive Rules of Logic — m2/m1 ratio 分類(PR-3c-3 落地)
//
// 對齊 m3Spec/neely_rules.md Ch3 p.3-48~60 line 422-493。
//
// 邏輯:m2(第二 monowave,通常是 retracement)/ m1(第一 monowave)的比值範圍
// 決定哪條 Ch3 規則適用,規則決定可以 add 哪些 Structure Labels(:F3 / :c3 /
// :sL3 / :s5 / :L5)。
//
// PR-3c-3 階段:只實作 ratio range classification,Pass 表示「該 candidate
// 符合此 Ch3 規則範圍」。具體 Condition × Category × sub_rule_index 完整 200+
// 分支 Structure Label 決策樹留 PR-4b(classifier 的 structure_labeler 系統)。
// ---------------------------------------------------------------------------

/// R4 範圍:61.8% < m2/m1 < 100%(±4% 容差 → 57.8% < ratio < 104%)
/// 邊界值在 Cond 與相鄰規則間;簡化版用開區間
fn rule_r4(candidate: &WaveCandidate, classified: &[ClassifiedMonowave]) -> RuleResult {
    let rid = RuleId::Ch3PreConstructive {
        rule: 4, condition: 'a', category: None, sub_rule_index: None,
    };
    let ratio_pct = match m2_over_m1_pct(candidate, classified) {
        Some(r) => r,
        None => return RuleResult::NotApplicable(rid),
    };
    if (57.8..104.0).contains(&ratio_pct) {
        RuleResult::Pass
    } else {
        RuleResult::NotApplicable(rid)
    }
}

/// R5 範圍:100% ≤ m2/m1 < 161.8%(± 4% 容差 → 96% ≤ ratio < 165.8%)
fn rule_r5(candidate: &WaveCandidate, classified: &[ClassifiedMonowave]) -> RuleResult {
    let rid = RuleId::Ch3PreConstructive {
        rule: 5, condition: 'a', category: None, sub_rule_index: None,
    };
    let ratio_pct = match m2_over_m1_pct(candidate, classified) {
        Some(r) => r,
        None => return RuleResult::NotApplicable(rid),
    };
    if (96.0..165.8).contains(&ratio_pct) {
        RuleResult::Pass
    } else {
        RuleResult::NotApplicable(rid)
    }
}

/// R6 範圍:161.8% ≤ m2/m1 ≤ 261.8%(±4% 容差 → 157.8% ≤ ratio ≤ 265.8%)
fn rule_r6(candidate: &WaveCandidate, classified: &[ClassifiedMonowave]) -> RuleResult {
    let rid = RuleId::Ch3PreConstructive {
        rule: 6, condition: 'a', category: None, sub_rule_index: None,
    };
    let ratio_pct = match m2_over_m1_pct(candidate, classified) {
        Some(r) => r,
        None => return RuleResult::NotApplicable(rid),
    };
    if (157.8..=265.8).contains(&ratio_pct) {
        RuleResult::Pass
    } else {
        RuleResult::NotApplicable(rid)
    }
}

/// R7 範圍:m2/m1 > 261.8%(±4% 容差 → ratio > 257.8%)
fn rule_r7(candidate: &WaveCandidate, classified: &[ClassifiedMonowave]) -> RuleResult {
    let rid = RuleId::Ch3PreConstructive {
        rule: 7, condition: 'a', category: None, sub_rule_index: None,
    };
    let ratio_pct = match m2_over_m1_pct(candidate, classified) {
        Some(r) => r,
        None => return RuleResult::NotApplicable(rid),
    };
    if ratio_pct > 257.8 {
        RuleResult::Pass
    } else {
        RuleResult::NotApplicable(rid)
    }
}

/// helper:取 m2 / m1 比值(%)。m1 = monowave_indices[0],m2 = monowave_indices[1]。
/// 若不足 2 個 monowave 或 m1 magnitude ≈ 0 → None。
fn m2_over_m1_pct(
    candidate: &WaveCandidate,
    classified: &[ClassifiedMonowave],
) -> Option<f64> {
    if candidate.monowave_indices.len() < 2 {
        return None;
    }
    let mi = &candidate.monowave_indices;
    let m1_mag = classified.get(mi[0])?.metrics.magnitude;
    let m2_mag = classified.get(mi[1])?.metrics.magnitude;
    if m1_mag.abs() < 1e-9 {
        None
    } else {
        Some(m2_mag / m1_mag * 100.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::monowave::ProportionMetrics;
    use crate::output::{Monowave, MonowaveDirection};
    use chrono::NaiveDate;

    fn cmw(start_p: f64, end_p: f64, dir: MonowaveDirection) -> ClassifiedMonowave {
        ClassifiedMonowave {
            monowave: Monowave {
                start_date: NaiveDate::parse_from_str("2026-01-01", "%Y-%m-%d").unwrap(),
                end_date: NaiveDate::parse_from_str("2026-01-05", "%Y-%m-%d").unwrap(),
                start_price: start_p,
                end_price: end_p,
                direction: dir,
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

    fn make_candidate(wave_count: usize, indices: Vec<usize>) -> WaveCandidate {
        WaveCandidate {
            id: format!("c{}-test", wave_count),
            monowave_indices: indices,
            wave_count,
            initial_direction: MonowaveDirection::Up,
            wave_segment_lengths: vec![1; wave_count],
        }
    }

    // ---------- R1 ----------

    #[test]
    fn r1_w2_does_not_overshoot_passes() {
        // W1: 100→110, W2: 110→104(只回測 60% W1)
        let classified = vec![
            cmw(100.0, 110.0, MonowaveDirection::Up),
            cmw(110.0, 104.0, MonowaveDirection::Down),
        ];
        let candidate = make_candidate(3, vec![0, 1, 0]); // wave_count=3 placeholder
        let result = rule_r1(&candidate, &classified);
        assert!(result.is_pass());
    }

    #[test]
    fn r1_w2_overshoots_w1_start_fails() {
        // W1: 100→110, W2: 110→95(回測 150% > 100% W1 起點)
        let classified = vec![
            cmw(100.0, 110.0, MonowaveDirection::Up),
            cmw(110.0, 95.0, MonowaveDirection::Down),
        ];
        let candidate = make_candidate(3, vec![0, 1, 0]);
        let result = rule_r1(&candidate, &classified);
        assert!(result.is_fail(), "W2 跨過 W1 起點應 R1 fail");
        if let RuleResult::Fail(rej) = result {
            assert_eq!(rej.rule_id, RuleId::Ch5Essential(1));
            assert!(rej.gap > 0.0);
        }
    }

    #[test]
    fn r1_down_direction_handled_symmetrically() {
        // W1 Down: 100→90, W2 Up: 90→105(回測 150% — 跨過 W1 起點 100)
        let classified = vec![
            cmw(100.0, 90.0, MonowaveDirection::Down),
            cmw(90.0, 105.0, MonowaveDirection::Up),
        ];
        let candidate = make_candidate(3, vec![0, 1, 0]);
        let result = rule_r1(&candidate, &classified);
        assert!(result.is_fail());
    }

    #[test]
    fn r1_short_candidate_returns_n_a() {
        let classified: Vec<ClassifiedMonowave> = vec![cmw(100.0, 110.0, MonowaveDirection::Up)];
        let candidate = make_candidate(1, vec![0]);
        let result = rule_r1(&candidate, &classified);
        assert!(matches!(result, RuleResult::NotApplicable(_)));
    }

    // ---------- R2 ----------

    #[test]
    fn r2_w3_not_shortest_passes() {
        // W1=10, W3=20, W5=14 → W3 不是最短 ✓
        let classified = vec![
            cmw(100.0, 110.0, MonowaveDirection::Up),
            cmw(110.0, 105.0, MonowaveDirection::Down),
            cmw(105.0, 125.0, MonowaveDirection::Up),
            cmw(125.0, 118.0, MonowaveDirection::Down),
            cmw(118.0, 132.0, MonowaveDirection::Up),
        ];
        let candidate = make_candidate(5, vec![0, 1, 2, 3, 4]);
        let result = rule_r2(&candidate, &classified);
        assert!(result.is_pass());
    }

    #[test]
    fn r2_w3_shortest_fails() {
        // W1=10, W3=5, W5=14 → W3 最短 ✗
        let classified = vec![
            cmw(100.0, 110.0, MonowaveDirection::Up),
            cmw(110.0, 105.0, MonowaveDirection::Down),
            cmw(105.0, 110.0, MonowaveDirection::Up),
            cmw(110.0, 108.0, MonowaveDirection::Down),
            cmw(108.0, 122.0, MonowaveDirection::Up),
        ];
        let candidate = make_candidate(5, vec![0, 1, 2, 3, 4]);
        let result = rule_r2(&candidate, &classified);
        assert!(result.is_fail());
    }

    #[test]
    fn r2_3wave_candidate_n_a() {
        let classified = vec![cmw(100.0, 110.0, MonowaveDirection::Up); 3];
        let candidate = make_candidate(3, vec![0, 1, 2]);
        let result = rule_r2(&candidate, &classified);
        assert!(matches!(result, RuleResult::NotApplicable(_)));
    }

    // ---------- R3 ----------

    #[test]
    fn r3_w4_above_w1_top_passes() {
        // W1: 100→110, W4 終點 118 > W1 終點 110 ✓
        let classified = vec![
            cmw(100.0, 110.0, MonowaveDirection::Up),
            cmw(110.0, 105.0, MonowaveDirection::Down),
            cmw(105.0, 125.0, MonowaveDirection::Up),
            cmw(125.0, 118.0, MonowaveDirection::Down),
            cmw(118.0, 132.0, MonowaveDirection::Up),
        ];
        let candidate = make_candidate(5, vec![0, 1, 2, 3, 4]);
        let result = rule_r3(&candidate, &classified);
        assert!(result.is_pass());
    }

    #[test]
    fn r3_w4_overlaps_w1_fails() {
        // W1: 100→110, W4 終點 105 < W1 終點 110 → 重疊
        let classified = vec![
            cmw(100.0, 110.0, MonowaveDirection::Up),
            cmw(110.0, 108.0, MonowaveDirection::Down),
            cmw(108.0, 120.0, MonowaveDirection::Up),
            cmw(120.0, 105.0, MonowaveDirection::Down),
            cmw(105.0, 130.0, MonowaveDirection::Up),
        ];
        let candidate = make_candidate(5, vec![0, 1, 2, 3, 4]);
        let result = rule_r3(&candidate, &classified);
        assert!(result.is_fail());
    }

    #[test]
    fn r3_3wave_candidate_n_a() {
        let classified = vec![cmw(100.0, 110.0, MonowaveDirection::Up); 3];
        let candidate = make_candidate(3, vec![0, 1, 2]);
        let result = rule_r3(&candidate, &classified);
        assert!(matches!(result, RuleResult::NotApplicable(_)));
    }

    // ---------- R4-R7 Ch3 Pre-Constructive ratio range classification(PR-3c-3)----------

    #[test]
    fn r4_m2_80pct_passes() {
        // m1=10 (100→110), m2=8 (110→102) → ratio 80% → R4 range Pass
        let classified = vec![
            cmw(100.0, 110.0, MonowaveDirection::Up),
            cmw(110.0, 102.0, MonowaveDirection::Down),
        ];
        let candidate = make_candidate(3, vec![0, 1, 0]);
        assert!(rule_r4(&candidate, &classified).is_pass());
        // 其他 R5-R7 應 NotApplicable
        assert!(matches!(rule_r5(&candidate, &classified), RuleResult::NotApplicable(_)));
        assert!(matches!(rule_r6(&candidate, &classified), RuleResult::NotApplicable(_)));
        assert!(matches!(rule_r7(&candidate, &classified), RuleResult::NotApplicable(_)));
    }

    #[test]
    fn r5_m2_130pct_passes() {
        // m1=10, m2=13 → ratio 130% → R5 range Pass
        let classified = vec![
            cmw(100.0, 110.0, MonowaveDirection::Up),
            cmw(110.0, 97.0, MonowaveDirection::Down),
        ];
        let candidate = make_candidate(3, vec![0, 1, 0]);
        assert!(rule_r5(&candidate, &classified).is_pass());
        assert!(matches!(rule_r4(&candidate, &classified), RuleResult::NotApplicable(_)));
        assert!(matches!(rule_r6(&candidate, &classified), RuleResult::NotApplicable(_)));
    }

    #[test]
    fn r6_m2_200pct_passes() {
        // m1=10, m2=20 → ratio 200% → R6 range Pass
        let classified = vec![
            cmw(100.0, 110.0, MonowaveDirection::Up),
            cmw(110.0, 90.0, MonowaveDirection::Down),
        ];
        let candidate = make_candidate(3, vec![0, 1, 0]);
        assert!(rule_r6(&candidate, &classified).is_pass());
        assert!(matches!(rule_r5(&candidate, &classified), RuleResult::NotApplicable(_)));
        assert!(matches!(rule_r7(&candidate, &classified), RuleResult::NotApplicable(_)));
    }

    #[test]
    fn r7_m2_300pct_passes() {
        // m1=10, m2=30 → ratio 300% → R7 Pass
        let classified = vec![
            cmw(100.0, 110.0, MonowaveDirection::Up),
            cmw(110.0, 80.0, MonowaveDirection::Down),
        ];
        let candidate = make_candidate(3, vec![0, 1, 0]);
        assert!(rule_r7(&candidate, &classified).is_pass());
        assert!(matches!(rule_r6(&candidate, &classified), RuleResult::NotApplicable(_)));
    }

    #[test]
    fn r4_short_candidate_not_applicable() {
        // 只有 1 個 monowave → R4-R7 都 NotApplicable
        let classified = vec![cmw(100.0, 110.0, MonowaveDirection::Up)];
        let candidate = WaveCandidate {
            id: "c-short".to_string(),
            monowave_indices: vec![0],
            wave_count: 1,
            initial_direction: MonowaveDirection::Up,
            wave_segment_lengths: vec![1; 1],
        };
        assert!(matches!(rule_r4(&candidate, &classified), RuleResult::NotApplicable(_)));
        assert!(matches!(rule_r5(&candidate, &classified), RuleResult::NotApplicable(_)));
        assert!(matches!(rule_r6(&candidate, &classified), RuleResult::NotApplicable(_)));
        assert!(matches!(rule_r7(&candidate, &classified), RuleResult::NotApplicable(_)));
    }

    #[test]
    fn r4_zero_m1_not_applicable() {
        // m1 magnitude = 0 → safe ratio = None → NotApplicable
        let classified = vec![
            cmw(100.0, 100.0, MonowaveDirection::Up),  // zero magnitude
            cmw(100.0, 105.0, MonowaveDirection::Up),
        ];
        let candidate = make_candidate(3, vec![0, 1, 0]);
        assert!(matches!(rule_r4(&candidate, &classified), RuleResult::NotApplicable(_)));
    }

    #[test]
    fn r4_to_r7_at_most_two_overlap_at_boundaries() {
        // 對任一 candidate,R4-R7 中**至多 2 個** Pass(±4% 容差導致相鄰規則邊界重疊)
        // 邊界重疊區:96-104%(R4/R5)/ 157.8-165.8%(R5/R6)/ 257.8-265.8%(R6/R7)
        // 非邊界區只應 1 個 Pass
        let test_ratios = [50.0, 80.0, 130.0, 200.0, 300.0];
        for ratio in test_ratios {
            let m1_mag = 10.0;
            let m2_mag = m1_mag * ratio / 100.0;
            let classified = vec![
                cmw(100.0, 100.0 + m1_mag, MonowaveDirection::Up),
                cmw(100.0 + m1_mag, 100.0 + m1_mag - m2_mag, MonowaveDirection::Down),
            ];
            let candidate = make_candidate(3, vec![0, 1, 0]);
            let pass_count = [
                rule_r4(&candidate, &classified),
                rule_r5(&candidate, &classified),
                rule_r6(&candidate, &classified),
                rule_r7(&candidate, &classified),
            ]
            .iter()
            .filter(|r| r.is_pass())
            .count();
            assert!(pass_count <= 1, "非邊界 ratio={}% 有 {} 個 R4-R7 Pass(預期 ≤ 1)", ratio, pass_count);
        }
    }

    #[test]
    fn r4_r5_boundary_100pct_both_pass() {
        // 邊界 100%:R4 上限 104% / R5 下限 96% → 兩規則都 Pass(設計如此)
        let classified = vec![
            cmw(100.0, 110.0, MonowaveDirection::Up),
            cmw(110.0, 100.0, MonowaveDirection::Down), // m2=10, ratio=100%
        ];
        let candidate = make_candidate(3, vec![0, 1, 0]);
        assert!(rule_r4(&candidate, &classified).is_pass());
        assert!(rule_r5(&candidate, &classified).is_pass());
        // R6/R7 不在邊界
        assert!(matches!(rule_r6(&candidate, &classified), RuleResult::NotApplicable(_)));
        assert!(matches!(rule_r7(&candidate, &classified), RuleResult::NotApplicable(_)));
    }
}
