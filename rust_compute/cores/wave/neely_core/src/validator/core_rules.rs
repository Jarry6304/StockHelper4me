// core_rules.rs — Validator Ch5 Essential Construction Rules R1-R7 + Overlap Rules
//
// 對齊 m3Spec/neely_rules.md §Impulsion(1291-1300 行)+ §Overlap Rule(1326-1329 行)。
// 對齊 m3Spec/neely_core_architecture.md §9.3 RuleId 編碼。
//
// **r5 spec 對應 7 條 Essential Construction Rules**(neely_rules.md 1292-1298):
//   1. R1:必須有 5 個相鄰段(structural — 由 candidate.wave_count 決定,本層僅
//      assert,不做數值判定)
//   2. R2:其中 3 段方向相同(W1/W3/W5 同向)
//   3. R3:W2 必定逆向,且不得完全回測 W1
//   4. R4:W3 須長於 W2
//   5. R5:W4 必定逆向(與 W2 同向),且不得完全回測 W3
//   6. R6:W5 幾乎總長於 W4,至少 38.2% × W4(短於 = 5th-Wave Failure)
//   7. R7:比較 1/3/5 垂直價格距離,W3 絕不可為三者中最短
//
// **r5 Overlap Rule**(neely_rules.md 1326-1329 行):
//   - Trending Impulse (5-3-5-3-5):wave-4 不可進入 wave-2 的價格區
//   - Terminal Impulse (3-3-3-3-3):wave-4 必須部分侵入 wave-2 的價格區
//
// **容差**(architecture §4.2 / §4.3):
//   - 38.2% Fib 比率採 ±4%(architecture §4.2 三檔表)
//   - r4 自編號 R1 / R2 / R3 → r5 編碼如下:
//     | r4(現有 best-guess) | r5 等價 |
//     | Core(1)「W2 不過 W1」    | Ch5_Essential(3) |
//     | Core(2)「W3 not shortest」 | Ch5_Essential(7) |
//     | Core(3)「W4 not overlap W1」| Ch5_Overlap_Trending |

use super::RuleResult;
use crate::candidates::WaveCandidate;
use crate::monowave::ClassifiedMonowave;
use crate::output::{MonowaveDirection, RuleId, RuleRejection};

/// R6 中的 38.2% Fibonacci 比率(neely_rules.md §Impulsion 第 6 條)。
/// architecture §4.2 Fib 容差 ±4% 在比對處套用,此處只放原始比率。
const W5_MIN_RATIO_TO_W4: f64 = 0.382;

/// architecture §4.2 三檔容差表 — Fibonacci 比率 ±4%
const FIB_TOLERANCE_PCT: f64 = 0.04;

/// 跑 Ch5 Essential R1-R7 + Overlap_Trending + Overlap_Terminal 對 candidate。
///
/// 共 9 條 RuleResult(7 Essential + 2 Overlap variant)— Overlap_Trending /
/// Overlap_Terminal 對「strict Impulse 假設」與「Diagonal/Terminal 假設」
/// 互相排他,classifier 根據哪個 fail 決定 pattern(Impulse vs Diagonal)。
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
        rule_overlap_trending(candidate, classified),
        rule_overlap_terminal(candidate, classified),
    ]
}

// ---------------------------------------------------------------------------
// 共用 helper
// ---------------------------------------------------------------------------

/// W{n} 段的 magnitude(start_price → end_price 的絕對差)
fn segment_magnitude(classified: &[ClassifiedMonowave], idx: usize) -> f64 {
    let mw = &classified[idx].monowave;
    (mw.end_price - mw.start_price).abs()
}

/// 拿 candidate 對應的 W1..W5 ClassifiedMonowave reference;wave_count 不足回 None
fn wave_refs<'a>(
    candidate: &WaveCandidate,
    classified: &'a [ClassifiedMonowave],
) -> Option<Vec<&'a crate::output::Monowave>> {
    let mi = &candidate.monowave_indices;
    if mi.len() < candidate.wave_count {
        return None;
    }
    Some(mi.iter().map(|&i| &classified[i].monowave).collect())
}

// ---------------------------------------------------------------------------
// R1 — 必須有 5 個相鄰段(structural assert)
// ---------------------------------------------------------------------------
// 適用:wave_count == 5
// 邏輯:candidate.wave_count == 5 → Pass;否則 NotApplicable
// 註:R1 是「5-wave 結構成立」前提條件,3-wave candidate 不適用 R2-R7。
fn rule_r1(candidate: &WaveCandidate, _classified: &[ClassifiedMonowave]) -> RuleResult {
    let rid = RuleId::Ch5_Essential(1);
    if candidate.wave_count == 5 && candidate.monowave_indices.len() == 5 {
        RuleResult::Pass
    } else {
        RuleResult::NotApplicable(rid)
    }
}

// ---------------------------------------------------------------------------
// R2 — W1/W3/W5 同向(3 段方向相同)
// ---------------------------------------------------------------------------
// 邏輯:三段 direction 相同 → Pass;不同 → Fail
// 註:Neutral monowave 不視為 directional → Fail
fn rule_r2(candidate: &WaveCandidate, classified: &[ClassifiedMonowave]) -> RuleResult {
    let rid = RuleId::Ch5_Essential(2);
    let Some(waves) = wave_refs(candidate, classified) else {
        return RuleResult::NotApplicable(rid);
    };
    if candidate.wave_count != 5 {
        return RuleResult::NotApplicable(rid);
    }
    let d1 = waves[0].direction;
    let d3 = waves[2].direction;
    let d5 = waves[4].direction;

    // Neutral 不算 directional
    let any_neutral = matches!(d1, MonowaveDirection::Neutral)
        || matches!(d3, MonowaveDirection::Neutral)
        || matches!(d5, MonowaveDirection::Neutral);
    if any_neutral {
        return RuleResult::Fail(RuleRejection {
            candidate_id: candidate.id.clone(),
            rule_id: rid,
            expected: "W1/W3/W5 必須同向(directional,非 Neutral)".to_string(),
            actual: format!("方向:W1={:?} / W3={:?} / W5={:?}(含 Neutral)", d1, d3, d5),
            gap: 0.0,
            neely_page: "neely_rules.md §Impulsion 第 2 條(p.5-2~5-3)".to_string(),
        });
    }

    if d1 == d3 && d3 == d5 {
        RuleResult::Pass
    } else {
        RuleResult::Fail(RuleRejection {
            candidate_id: candidate.id.clone(),
            rule_id: rid,
            expected: "W1/W3/W5 同向".to_string(),
            actual: format!("方向:W1={:?} / W3={:?} / W5={:?}", d1, d3, d5),
            gap: 0.0,
            neely_page: "neely_rules.md §Impulsion 第 2 條(p.5-2~5-3)".to_string(),
        })
    }
}

// ---------------------------------------------------------------------------
// R3 — W2 必定逆向,且不得完全回測 W1
// ---------------------------------------------------------------------------
// 邏輯:
//   - W2.direction != W1.direction(逆向)
//   - W1 Up:W2 終點 ≥ W1 起點(W2.end >= W1.start)
//   - W1 Down:W2 終點 ≤ W1 起點(W2.end <= W1.start)
// 容差:無 — 「完全回測」採絕對門檻(spec §Impulsion 第 3 條 原文「不得完全」)
fn rule_r3(candidate: &WaveCandidate, classified: &[ClassifiedMonowave]) -> RuleResult {
    let rid = RuleId::Ch5_Essential(3);
    let Some(waves) = wave_refs(candidate, classified) else {
        return RuleResult::NotApplicable(rid);
    };
    if candidate.wave_count < 3 {
        return RuleResult::NotApplicable(rid);
    }
    let w1 = waves[0];
    let w2 = waves[1];

    // 方向約束:W2 必須與 W1 逆向
    if w2.direction == w1.direction
        || matches!(w1.direction, MonowaveDirection::Neutral)
        || matches!(w2.direction, MonowaveDirection::Neutral)
    {
        return RuleResult::Fail(RuleRejection {
            candidate_id: candidate.id.clone(),
            rule_id: rid,
            expected: "W2 須與 W1 逆向".to_string(),
            actual: format!("W1 dir={:?} / W2 dir={:?}", w1.direction, w2.direction),
            gap: 0.0,
            neely_page: "neely_rules.md §Impulsion 第 3 條(p.5-2~5-3)".to_string(),
        });
    }

    // 回測約束:W2 終點不可跨過 W1 起點
    let (violated, overshoot) = match w1.direction {
        MonowaveDirection::Up => (w2.end_price < w1.start_price, w1.start_price - w2.end_price),
        MonowaveDirection::Down => (w2.end_price > w1.start_price, w2.end_price - w1.start_price),
        MonowaveDirection::Neutral => return RuleResult::NotApplicable(rid),
    };

    if violated {
        let w1_magnitude = (w1.end_price - w1.start_price).abs();
        let gap_pct = if w1_magnitude > 0.0 {
            overshoot / w1_magnitude * 100.0
        } else {
            0.0
        };
        RuleResult::Fail(RuleRejection {
            candidate_id: candidate.id.clone(),
            rule_id: rid,
            expected: format!(
                "W2 終點 {:.2} 不可跨 W1 起點 {:.2}(同側)",
                w2.end_price, w1.start_price
            ),
            actual: format!(
                "W2 完全回測 W1(overshoot {:.2},方向={:?})",
                overshoot, w1.direction
            ),
            gap: gap_pct,
            neely_page: "neely_rules.md §Impulsion 第 3 條(p.5-2~5-3)".to_string(),
        })
    } else {
        RuleResult::Pass
    }
}

// ---------------------------------------------------------------------------
// R4 — W3 須長於 W2
// ---------------------------------------------------------------------------
// 邏輯:|W3.magnitude| > |W2.magnitude|
// 容差:無(strict)— spec §Impulsion 第 4 條 原文「必須長於」
fn rule_r4(candidate: &WaveCandidate, classified: &[ClassifiedMonowave]) -> RuleResult {
    let rid = RuleId::Ch5_Essential(4);
    let Some(_) = wave_refs(candidate, classified) else {
        return RuleResult::NotApplicable(rid);
    };
    if candidate.wave_count < 3 {
        return RuleResult::NotApplicable(rid);
    }
    let mi = &candidate.monowave_indices;
    let w2_mag = segment_magnitude(classified, mi[1]);
    let w3_mag = segment_magnitude(classified, mi[2]);

    if w3_mag > w2_mag {
        RuleResult::Pass
    } else {
        let gap_pct = if w2_mag > 0.0 {
            (w2_mag - w3_mag) / w2_mag * 100.0
        } else {
            0.0
        };
        RuleResult::Fail(RuleRejection {
            candidate_id: candidate.id.clone(),
            rule_id: rid,
            expected: format!("W3 magnitude {:.4} 須 > W2 magnitude {:.4}", w3_mag, w2_mag),
            actual: format!("W3 {:.4} ≤ W2 {:.4}", w3_mag, w2_mag),
            gap: gap_pct,
            neely_page: "neely_rules.md §Impulsion 第 4 條(p.5-2~5-3)".to_string(),
        })
    }
}

// ---------------------------------------------------------------------------
// R5 — W4 必定逆向(與 W2 同向),且不得完全回測 W3
// ---------------------------------------------------------------------------
fn rule_r5(candidate: &WaveCandidate, classified: &[ClassifiedMonowave]) -> RuleResult {
    let rid = RuleId::Ch5_Essential(5);
    let Some(waves) = wave_refs(candidate, classified) else {
        return RuleResult::NotApplicable(rid);
    };
    if candidate.wave_count != 5 {
        return RuleResult::NotApplicable(rid);
    }
    let w2 = waves[1];
    let w3 = waves[2];
    let w4 = waves[3];

    // 方向約束:W4 必須與 W2 同向(W3 反向)
    if w4.direction != w2.direction
        || w4.direction == w3.direction
        || matches!(w4.direction, MonowaveDirection::Neutral)
    {
        return RuleResult::Fail(RuleRejection {
            candidate_id: candidate.id.clone(),
            rule_id: rid,
            expected: "W4 須與 W2 同向(與 W3 逆向)".to_string(),
            actual: format!(
                "方向:W2={:?} / W3={:?} / W4={:?}",
                w2.direction, w3.direction, w4.direction
            ),
            gap: 0.0,
            neely_page: "neely_rules.md §Impulsion 第 5 條(p.5-2~5-3)".to_string(),
        });
    }

    // 回測約束:W4 終點不可跨 W3 起點
    let (violated, overshoot) = match w3.direction {
        MonowaveDirection::Up => (w4.end_price < w3.start_price, w3.start_price - w4.end_price),
        MonowaveDirection::Down => (w4.end_price > w3.start_price, w4.end_price - w3.start_price),
        MonowaveDirection::Neutral => return RuleResult::NotApplicable(rid),
    };

    if violated {
        let w3_magnitude = (w3.end_price - w3.start_price).abs();
        let gap_pct = if w3_magnitude > 0.0 {
            overshoot / w3_magnitude * 100.0
        } else {
            0.0
        };
        RuleResult::Fail(RuleRejection {
            candidate_id: candidate.id.clone(),
            rule_id: rid,
            expected: format!(
                "W4 終點 {:.2} 不可跨 W3 起點 {:.2}",
                w4.end_price, w3.start_price
            ),
            actual: format!("W4 完全回測 W3(overshoot {:.2})", overshoot),
            gap: gap_pct,
            neely_page: "neely_rules.md §Impulsion 第 5 條(p.5-2~5-3)".to_string(),
        })
    } else {
        RuleResult::Pass
    }
}

// ---------------------------------------------------------------------------
// R6 — W5 幾乎總長於 W4,至少 38.2% × W4(短於 = 5th-Wave Failure)
// ---------------------------------------------------------------------------
// 容差:38.2% 採 architecture §4.2 ±4% 容差
//       下界 = 0.382 × (1 - 0.04) = 0.36672
fn rule_r6(candidate: &WaveCandidate, classified: &[ClassifiedMonowave]) -> RuleResult {
    let rid = RuleId::Ch5_Essential(6);
    if candidate.wave_count != 5 || candidate.monowave_indices.len() < 5 {
        return RuleResult::NotApplicable(rid);
    }
    let mi = &candidate.monowave_indices;
    let w4_mag = segment_magnitude(classified, mi[3]);
    let w5_mag = segment_magnitude(classified, mi[4]);

    let min_required = W5_MIN_RATIO_TO_W4 * (1.0 - FIB_TOLERANCE_PCT) * w4_mag;

    if w5_mag >= min_required {
        RuleResult::Pass
    } else {
        let actual_ratio = if w4_mag > 0.0 { w5_mag / w4_mag } else { 0.0 };
        let gap_pct = (W5_MIN_RATIO_TO_W4 - actual_ratio) * 100.0;
        RuleResult::Fail(RuleRejection {
            candidate_id: candidate.id.clone(),
            rule_id: rid,
            expected: format!(
                "W5 magnitude {:.4} 須 ≥ 38.2% × W4 {:.4}(下限 {:.4},±4% 容差)",
                w5_mag, w4_mag, min_required
            ),
            actual: format!("W5/W4 比 = {:.4}(5th-Wave Failure)", actual_ratio),
            gap: gap_pct,
            neely_page: "neely_rules.md §Impulsion 第 6 條(p.5-2~5-3,5th-Wave Failure)".to_string(),
        })
    }
}

// ---------------------------------------------------------------------------
// R7 — W3 絕不可為 W1/W3/W5 中最短
// ---------------------------------------------------------------------------
// 邏輯:W3.magnitude >= min(W1.magnitude, W5.magnitude)
// 容差:無 — spec §Impulsion 第 7 條 原文「絕不可」
fn rule_r7(candidate: &WaveCandidate, classified: &[ClassifiedMonowave]) -> RuleResult {
    let rid = RuleId::Ch5_Essential(7);
    if candidate.wave_count != 5 || candidate.monowave_indices.len() < 5 {
        return RuleResult::NotApplicable(rid);
    }
    let mi = &candidate.monowave_indices;
    let w1_mag = segment_magnitude(classified, mi[0]);
    let w3_mag = segment_magnitude(classified, mi[2]);
    let w5_mag = segment_magnitude(classified, mi[4]);
    let min_w1_w5 = w1_mag.min(w5_mag);

    if w3_mag >= min_w1_w5 {
        RuleResult::Pass
    } else {
        let gap_pct = (min_w1_w5 - w3_mag) / min_w1_w5.max(1e-9) * 100.0;
        RuleResult::Fail(RuleRejection {
            candidate_id: candidate.id.clone(),
            rule_id: rid,
            expected: format!(
                "W3 magnitude {:.4} 須 ≥ min(W1={:.4}, W5={:.4}) = {:.4}",
                w3_mag, w1_mag, w5_mag, min_w1_w5
            ),
            actual: "W3 為三者中最短".to_string(),
            gap: gap_pct,
            neely_page: "neely_rules.md §Impulsion 第 7 條(p.5-2~5-3)".to_string(),
        })
    }
}

// ---------------------------------------------------------------------------
// Ch5_Overlap_Trending — Trending Impulse:W4 不可進入 W2 區
// ---------------------------------------------------------------------------
// 邏輯:
//   - W1 Up(impulse 上漲):W4 endpoint 必須 ≥ W2 endpoint
//     (W2 endpoint 是 W1 之後的低點,W4 必須在 W2 高之上)
//     換句話說:W4 終點 > W1 終點(W2 終點 < W1 終點 < W4 終點)
//   - W1 Down(impulse 下跌):對稱
// Trending fail → classifier 認為這不是 Trending Impulse → 可能是 Terminal Diagonal
fn rule_overlap_trending(
    candidate: &WaveCandidate,
    classified: &[ClassifiedMonowave],
) -> RuleResult {
    let rid = RuleId::Ch5_Overlap_Trending;
    if candidate.wave_count != 5 || candidate.monowave_indices.len() < 5 {
        return RuleResult::NotApplicable(rid);
    }
    let mi = &candidate.monowave_indices;
    let w1 = &classified[mi[0]].monowave;
    let w2 = &classified[mi[1]].monowave;
    let w4 = &classified[mi[3]].monowave;

    let direction = w1.direction;
    // W2 終點 = W2.end_price(W1 之後的反向終點)
    // W4 終點 = W4.end_price(W3 之後的反向終點)
    // Trending:W4 終點不可進入 [W2.start, W2.end] 區間(W2.start = W1.end)
    let (violated, expected) = match direction {
        MonowaveDirection::Up => (
            // 上漲 trending:W4 end 必須 ≥ W2 end(= W1 之後反向最低點)
            w4.end_price < w2.end_price,
            format!("W4 終點 {:.2} 須 ≥ W2 終點 {:.2}", w4.end_price, w2.end_price),
        ),
        MonowaveDirection::Down => (
            // 下跌 trending:W4 end 必須 ≤ W2 end(W1 之後反向最高點)
            w4.end_price > w2.end_price,
            format!("W4 終點 {:.2} 須 ≤ W2 終點 {:.2}", w4.end_price, w2.end_price),
        ),
        MonowaveDirection::Neutral => return RuleResult::NotApplicable(rid),
    };

    if violated {
        let w2_magnitude = (w2.end_price - w2.start_price).abs();
        let overlap = (w4.end_price - w2.end_price).abs();
        let gap_pct = if w2_magnitude > 0.0 {
            overlap / w2_magnitude * 100.0
        } else {
            0.0
        };
        RuleResult::Fail(RuleRejection {
            candidate_id: candidate.id.clone(),
            rule_id: rid,
            expected,
            actual: format!("W4 進入 W2 區間(direction={:?})", direction),
            gap: gap_pct,
            neely_page: "neely_rules.md §Overlap Rule(1326-1329 行)".to_string(),
        })
    } else {
        RuleResult::Pass
    }
}

// ---------------------------------------------------------------------------
// Ch5_Overlap_Terminal — Terminal Impulse:W4 必須部分侵入 W2 區
// ---------------------------------------------------------------------------
// 邏輯:Trending 的反向 — W4 終點必須進入 W2 區(W2 終點 < W4 終點 < W1 終點,Up 方向)
// 註:此規則對「Terminal Impulse 假設」是必要條件。Trending 假設下會 Fail。
//     兩條 Overlap 規則對給定 candidate 互為排他:任一 candidate 只能滿足其一。
//     classifier 用 Trending pass / Terminal fail → Impulse;
//                Trending fail / Terminal pass → Diagonal(Terminal Impulse);
//                兩個都 fail → 結構錯亂(reject)
fn rule_overlap_terminal(
    candidate: &WaveCandidate,
    classified: &[ClassifiedMonowave],
) -> RuleResult {
    let rid = RuleId::Ch5_Overlap_Terminal;
    if candidate.wave_count != 5 || candidate.monowave_indices.len() < 5 {
        return RuleResult::NotApplicable(rid);
    }
    let mi = &candidate.monowave_indices;
    let w1 = &classified[mi[0]].monowave;
    let w2 = &classified[mi[1]].monowave;
    let w4 = &classified[mi[3]].monowave;

    let direction = w1.direction;
    let (violated, expected) = match direction {
        MonowaveDirection::Up => (
            // Terminal Up:W4 終點必須 < W2 終點(進入 W2 區 — W2 終點以下)
            w4.end_price >= w2.end_price,
            format!(
                "W4 終點 {:.2} 須 < W2 終點 {:.2}(Terminal 侵入)",
                w4.end_price, w2.end_price
            ),
        ),
        MonowaveDirection::Down => (
            w4.end_price <= w2.end_price,
            format!(
                "W4 終點 {:.2} 須 > W2 終點 {:.2}(Terminal 侵入)",
                w4.end_price, w2.end_price
            ),
        ),
        MonowaveDirection::Neutral => return RuleResult::NotApplicable(rid),
    };

    if violated {
        RuleResult::Fail(RuleRejection {
            candidate_id: candidate.id.clone(),
            rule_id: rid,
            expected,
            actual: "W4 未侵入 W2 區(Trending 假設,非 Terminal)".to_string(),
            gap: 0.0,
            neely_page: "neely_rules.md §Overlap Rule(1326-1329 行)".to_string(),
        })
    } else {
        RuleResult::Pass
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
            structure_label_candidates: Vec::new(),
        }
    }

    fn make_candidate(wave_count: usize, indices: Vec<usize>) -> WaveCandidate {
        WaveCandidate {
            id: format!("c{}-test", wave_count),
            monowave_indices: indices,
            wave_count,
            initial_direction: MonowaveDirection::Up,
        }
    }

    /// 經典 5-wave Up Impulse(對齊 spec §Impulsion 7 條全 pass):
    ///   W1: 100→110 (mag 10)
    ///   W2: 110→104 (mag 6,回測 60% W1,未跨 W1 起點 100 ✓)
    ///   W3: 104→125 (mag 21,> W2 ✓,> min(W1=10, W5=14) ✓)
    ///   W4: 125→118 (mag 7,未跨 W3 起點 104 ✓,W4 終點 118 > W2 終點 104 → Trending pass)
    ///   W5: 118→132 (mag 14,> 38.2% × W4=2.7 ✓)
    fn make_5wave_clean_impulse_up() -> Vec<ClassifiedMonowave> {
        vec![
            cmw(100.0, 110.0, MonowaveDirection::Up),
            cmw(110.0, 104.0, MonowaveDirection::Down),
            cmw(104.0, 125.0, MonowaveDirection::Up),
            cmw(125.0, 118.0, MonowaveDirection::Down),
            cmw(118.0, 132.0, MonowaveDirection::Up),
        ]
    }

    // ---------- R1 ----------

    #[test]
    fn r1_five_wave_candidate_passes() {
        let classified = make_5wave_clean_impulse_up();
        let candidate = make_candidate(5, vec![0, 1, 2, 3, 4]);
        assert!(rule_r1(&candidate, &classified).is_pass());
    }

    #[test]
    fn r1_three_wave_candidate_n_a() {
        let classified = vec![cmw(100.0, 110.0, MonowaveDirection::Up); 3];
        let candidate = make_candidate(3, vec![0, 1, 2]);
        assert!(matches!(
            rule_r1(&candidate, &classified),
            RuleResult::NotApplicable(_)
        ));
    }

    // ---------- R2 ----------

    #[test]
    fn r2_w1_w3_w5_same_direction_passes() {
        let classified = make_5wave_clean_impulse_up();
        let candidate = make_candidate(5, vec![0, 1, 2, 3, 4]);
        assert!(rule_r2(&candidate, &classified).is_pass());
    }

    #[test]
    fn r2_w1_w3_w5_mixed_directions_fails() {
        // W3 down(誤),其他 up
        let classified = vec![
            cmw(100.0, 110.0, MonowaveDirection::Up),
            cmw(110.0, 105.0, MonowaveDirection::Down),
            cmw(105.0, 95.0, MonowaveDirection::Down), // 應為 Up
            cmw(95.0, 100.0, MonowaveDirection::Up),
            cmw(100.0, 115.0, MonowaveDirection::Up),
        ];
        let candidate = make_candidate(5, vec![0, 1, 2, 3, 4]);
        assert!(rule_r2(&candidate, &classified).is_fail());
    }

    #[test]
    fn r2_three_wave_candidate_n_a() {
        let classified = vec![cmw(100.0, 110.0, MonowaveDirection::Up); 3];
        let candidate = make_candidate(3, vec![0, 1, 2]);
        assert!(matches!(
            rule_r2(&candidate, &classified),
            RuleResult::NotApplicable(_)
        ));
    }

    // ---------- R3 ----------

    #[test]
    fn r3_w2_does_not_overshoot_w1_start_passes() {
        let classified = make_5wave_clean_impulse_up();
        let candidate = make_candidate(5, vec![0, 1, 2, 3, 4]);
        assert!(rule_r3(&candidate, &classified).is_pass());
    }

    #[test]
    fn r3_w2_overshoots_w1_start_fails() {
        // W2 回測到 95 < W1 起點 100
        let classified = vec![
            cmw(100.0, 110.0, MonowaveDirection::Up),
            cmw(110.0, 95.0, MonowaveDirection::Down),
            cmw(95.0, 120.0, MonowaveDirection::Up),
            cmw(120.0, 115.0, MonowaveDirection::Down),
            cmw(115.0, 130.0, MonowaveDirection::Up),
        ];
        let candidate = make_candidate(5, vec![0, 1, 2, 3, 4]);
        let result = rule_r3(&candidate, &classified);
        assert!(result.is_fail(), "W2 完全回測 W1 應 R3 fail");
        if let RuleResult::Fail(rej) = result {
            assert!(matches!(rej.rule_id, RuleId::Ch5_Essential(3)));
            assert!(rej.gap > 0.0);
        }
    }

    #[test]
    fn r3_w2_same_direction_as_w1_fails() {
        // W2 同向 W1 → 方向約束 fail
        let classified = vec![
            cmw(100.0, 110.0, MonowaveDirection::Up),
            cmw(110.0, 120.0, MonowaveDirection::Up), // 應 Down
            cmw(120.0, 130.0, MonowaveDirection::Up),
        ];
        let candidate = make_candidate(3, vec![0, 1, 2]);
        assert!(rule_r3(&candidate, &classified).is_fail());
    }

    // ---------- R4 ----------

    #[test]
    fn r4_w3_longer_than_w2_passes() {
        let classified = make_5wave_clean_impulse_up();
        let candidate = make_candidate(5, vec![0, 1, 2, 3, 4]);
        assert!(rule_r4(&candidate, &classified).is_pass());
    }

    #[test]
    fn r4_w3_shorter_than_w2_fails() {
        // W2 mag 10 > W3 mag 5
        let classified = vec![
            cmw(100.0, 120.0, MonowaveDirection::Up),
            cmw(120.0, 110.0, MonowaveDirection::Down), // W2 mag 10
            cmw(110.0, 115.0, MonowaveDirection::Up),   // W3 mag 5 — 比 W2 短
            cmw(115.0, 113.0, MonowaveDirection::Down),
            cmw(113.0, 125.0, MonowaveDirection::Up),
        ];
        let candidate = make_candidate(5, vec![0, 1, 2, 3, 4]);
        assert!(rule_r4(&candidate, &classified).is_fail());
    }

    // ---------- R5 ----------

    #[test]
    fn r5_w4_does_not_overshoot_w3_start_passes() {
        let classified = make_5wave_clean_impulse_up();
        let candidate = make_candidate(5, vec![0, 1, 2, 3, 4]);
        assert!(rule_r5(&candidate, &classified).is_pass());
    }

    #[test]
    fn r5_w4_overshoots_w3_start_fails() {
        // W3: 104→125, W4: 125→100(< W3 起點 104 → fail)
        let classified = vec![
            cmw(100.0, 110.0, MonowaveDirection::Up),
            cmw(110.0, 104.0, MonowaveDirection::Down),
            cmw(104.0, 125.0, MonowaveDirection::Up),
            cmw(125.0, 100.0, MonowaveDirection::Down), // 跨 W3 起點 104
            cmw(100.0, 130.0, MonowaveDirection::Up),
        ];
        let candidate = make_candidate(5, vec![0, 1, 2, 3, 4]);
        assert!(rule_r5(&candidate, &classified).is_fail());
    }

    // ---------- R6 ----------

    #[test]
    fn r6_w5_at_least_38pct_of_w4_passes() {
        let classified = make_5wave_clean_impulse_up();
        let candidate = make_candidate(5, vec![0, 1, 2, 3, 4]);
        // W4 mag = 7, W5 mag = 14 — 14 / 7 = 2.0 >> 0.382
        assert!(rule_r6(&candidate, &classified).is_pass());
    }

    #[test]
    fn r6_w5_less_than_38pct_of_w4_fails() {
        // W4 mag 10, W5 mag 2 — 2/10 = 0.20 < 0.382 - 0.04 → 5th-Wave Failure
        let classified = vec![
            cmw(100.0, 110.0, MonowaveDirection::Up),
            cmw(110.0, 104.0, MonowaveDirection::Down),
            cmw(104.0, 125.0, MonowaveDirection::Up),
            cmw(125.0, 115.0, MonowaveDirection::Down), // W4 mag 10
            cmw(115.0, 117.0, MonowaveDirection::Up),   // W5 mag 2
        ];
        let candidate = make_candidate(5, vec![0, 1, 2, 3, 4]);
        assert!(rule_r6(&candidate, &classified).is_fail());
    }

    // ---------- R7 ----------

    #[test]
    fn r7_w3_not_shortest_passes() {
        let classified = make_5wave_clean_impulse_up();
        let candidate = make_candidate(5, vec![0, 1, 2, 3, 4]);
        // W1=10, W3=21, W5=14 — W3 不是最短
        assert!(rule_r7(&candidate, &classified).is_pass());
    }

    #[test]
    fn r7_w3_shortest_fails() {
        // W1=10, W3=5, W5=14 — W3 最短
        let classified = vec![
            cmw(100.0, 110.0, MonowaveDirection::Up),
            cmw(110.0, 105.0, MonowaveDirection::Down),
            cmw(105.0, 110.0, MonowaveDirection::Up),
            cmw(110.0, 108.0, MonowaveDirection::Down),
            cmw(108.0, 122.0, MonowaveDirection::Up),
        ];
        let candidate = make_candidate(5, vec![0, 1, 2, 3, 4]);
        let result = rule_r7(&candidate, &classified);
        assert!(result.is_fail());
        if let RuleResult::Fail(rej) = result {
            assert!(matches!(rej.rule_id, RuleId::Ch5_Essential(7)));
        }
    }

    // ---------- Ch5_Overlap_Trending ----------

    #[test]
    fn overlap_trending_w4_above_w2_passes() {
        let classified = make_5wave_clean_impulse_up();
        let candidate = make_candidate(5, vec![0, 1, 2, 3, 4]);
        // W2 終點 104, W4 終點 118 — W4 在 W2 之上 → Trending pass
        assert!(rule_overlap_trending(&candidate, &classified).is_pass());
    }

    #[test]
    fn overlap_trending_w4_enters_w2_zone_fails() {
        // W2 終點 104, W4 終點 100 — W4 進入 W2 區 → Trending fail(可能 Terminal Impulse)
        let classified = vec![
            cmw(100.0, 110.0, MonowaveDirection::Up),
            cmw(110.0, 104.0, MonowaveDirection::Down),
            cmw(104.0, 125.0, MonowaveDirection::Up),
            cmw(125.0, 100.0, MonowaveDirection::Down), // W4 終點 100 < W2 終點 104
            cmw(100.0, 130.0, MonowaveDirection::Up),
        ];
        let candidate = make_candidate(5, vec![0, 1, 2, 3, 4]);
        let result = rule_overlap_trending(&candidate, &classified);
        assert!(result.is_fail());
        if let RuleResult::Fail(rej) = result {
            assert!(matches!(rej.rule_id, RuleId::Ch5_Overlap_Trending));
        }
    }

    // ---------- Ch5_Overlap_Terminal ----------

    #[test]
    fn overlap_terminal_w4_above_w2_fails() {
        // Trending pattern → Terminal fail(W4 沒侵入 W2 區)
        let classified = make_5wave_clean_impulse_up();
        let candidate = make_candidate(5, vec![0, 1, 2, 3, 4]);
        let result = rule_overlap_terminal(&candidate, &classified);
        assert!(result.is_fail());
        if let RuleResult::Fail(rej) = result {
            assert!(matches!(rej.rule_id, RuleId::Ch5_Overlap_Terminal));
        }
    }

    #[test]
    fn overlap_terminal_w4_enters_w2_zone_passes() {
        // Terminal Diagonal:W4 進入 W2 區 → Terminal pass
        let classified = vec![
            cmw(100.0, 110.0, MonowaveDirection::Up),
            cmw(110.0, 104.0, MonowaveDirection::Down),
            cmw(104.0, 125.0, MonowaveDirection::Up),
            cmw(125.0, 100.0, MonowaveDirection::Down), // W4 終點 100 < W2 終點 104
            cmw(100.0, 130.0, MonowaveDirection::Up),
        ];
        let candidate = make_candidate(5, vec![0, 1, 2, 3, 4]);
        assert!(rule_overlap_terminal(&candidate, &classified).is_pass());
    }

    // ---------- run() 完整 9 條規則 ----------

    #[test]
    fn run_clean_impulse_returns_seven_pass_one_terminal_fail() {
        let classified = make_5wave_clean_impulse_up();
        let candidate = make_candidate(5, vec![0, 1, 2, 3, 4]);
        let results = run(&candidate, &classified);
        assert_eq!(results.len(), 9);

        // R1-R7 全 pass + Trending pass = 8 pass / Terminal fail = 1 fail
        let pass_count = results.iter().filter(|r| r.is_pass()).count();
        let fail_count = results.iter().filter(|r| r.is_fail()).count();
        assert_eq!(pass_count, 8, "clean impulse 應 8 條 pass(R1-R7 + Trending)");
        assert_eq!(fail_count, 1, "clean impulse 應 1 條 fail(Terminal)");
    }

    #[test]
    fn run_three_wave_candidate_returns_mixed_n_a() {
        let classified = vec![
            cmw(100.0, 110.0, MonowaveDirection::Up),
            cmw(110.0, 104.0, MonowaveDirection::Down),
            cmw(104.0, 115.0, MonowaveDirection::Up),
        ];
        let candidate = make_candidate(3, vec![0, 1, 2]);
        let results = run(&candidate, &classified);
        // R3 適用(W1/W2 都在);R1, R2, R4, R5, R6, R7, Overlap_Trending, Overlap_Terminal NotApplicable
        let n_a_count = results
            .iter()
            .filter(|r| matches!(r, RuleResult::NotApplicable(_)))
            .count();
        assert!(n_a_count >= 6, "3-wave candidate 應大部分 N/A");
    }
}
