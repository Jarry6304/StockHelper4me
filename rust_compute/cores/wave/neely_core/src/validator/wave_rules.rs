// wave_rules.rs — Validator W1-W2(通用波浪規則,r5 §9.3)
//
// 對齊 m3Spec/neely_rules.md Ch5 p.5-34~47 + Ch11 p.11-4~18 + Ch12 Fibonacci。
//
// **PR-3c-1 階段(2026-05-13)**:W1-W2 落地基礎實作。
//
// 規則語意:
//   - W1(Ch11ImpulseWaveByWave):Impulse Extension 分類(1st/3rd/5th Ext / Non-Ext);
//     5-wave 中識別哪條 wave 最長(magnitude 顯著大於其他 actionable wave)
//   - W2(Ch12FibonacciInternal):Essential Construction + Fibonacci 內部比例
//     (wave-c Zigzag 常 = a;Triangle 常有 61.8%;Impulse W5/W1 常 = 100/161.8%)

use super::helpers::{magnitude, matches_any_fib_ratio, safe_pct};
use super::RuleResult;
use crate::candidates::WaveCandidate;
use crate::monowave::ClassifiedMonowave;
use crate::output::{ImpulseExtension, RuleId, WaveNumber};

/// W1 Extension 判定的「顯著長」門檻:longest / next-longest ≥ 1.1(略大 10%)
/// 對齊 §4.2「一般容差 ±10%」— 兩 wave magnitude 差距需 > 10% 才算顯著
pub const W1_EXTENSION_RATIO: f64 = 1.1;

/// 跑 W1-W2 對 candidate
pub fn run(
    candidate: &WaveCandidate,
    classified: &[ClassifiedMonowave],
) -> Vec<RuleResult> {
    vec![
        rule_w1(candidate, classified),
        rule_w2(candidate, classified),
    ]
}

// ---------------------------------------------------------------------------
// W1:Impulse Extension 分類(Ch11 p.11-4~18)
//
// 適用:wave_count == 5(Impulse 5-wave)
// 邏輯:
//   - 比較 W1 / W3 / W5(3 個 actionable wave)magnitude
//   - 最長 wave > 次長 wave × 1.1 → 該 wave 為延長波(1st/3rd/5th Ext)
//   - 無顯著最長 → Non-Ext
//   - 永遠 Pass(W1 是分類規則,不會 Fail);wave_count != 5 → NotApplicable
// ---------------------------------------------------------------------------
fn rule_w1(candidate: &WaveCandidate, classified: &[ClassifiedMonowave]) -> RuleResult {
    // RuleId 帶 best-guess ext + wave:具體 ext 在實際分類時填,留 PR-4b
    // sub_kind 細分時改 dynamic enum variant
    let rid = RuleId::Ch11ImpulseWaveByWave {
        ext: ImpulseExtension::ThirdExt,
        wave: WaveNumber::Three,
    };
    if candidate.wave_count != 5 || candidate.monowave_indices.len() < 5 {
        return RuleResult::NotApplicable(rid);
    }
    let mi = &candidate.monowave_indices;
    let w1 = magnitude(&classified[mi[0]]);
    let w3 = magnitude(&classified[mi[2]]);
    let w5 = magnitude(&classified[mi[4]]);

    // W1 規則:任一 actionable wave magnitude > 0 即 Pass(基礎 sanity)
    if w1 <= 0.0 || w3 <= 0.0 || w5 <= 0.0 {
        return RuleResult::NotApplicable(rid);
    }

    // 此 rule 不會 Fail — 是分類規則,W1/W3/W5 都有 magnitude 即 Pass
    // 實際 Extension 分類由 classifier 用 same 邏輯決定 sub_kind(留 PR-4b)
    RuleResult::Pass
}

// ---------------------------------------------------------------------------
// W2:Fibonacci 內部比例(Ch12 Fibonacci Internal)
//
// 適用:wave_count == 3(c/a)或 5(W5/W1 或 W3/W1)
// 邏輯:
//   - 3-wave:c/a × 100 匹配任一 Neely 標準 Fib 比率(38.2/61.8/100/161.8/261.8 ±4%)
//   - 5-wave:W5/W1 或 W3/W1 ratio 匹配任一 Fib 比率
//   - 至少匹配一個 → Pass(Fibonacci alignment 存在)
//   - 無匹配 → NotApplicable(沒有顯著 Fib relationship,但結構可能仍合法)
//
// 注意:W2 不會 Fail — Fibonacci 缺席不代表結構違反,只代表「無 Fib 對齊事實」。
// ---------------------------------------------------------------------------
fn rule_w2(candidate: &WaveCandidate, classified: &[ClassifiedMonowave]) -> RuleResult {
    let rid = RuleId::Ch12FibonacciInternal;

    if candidate.monowave_indices.is_empty() {
        return RuleResult::NotApplicable(rid);
    }
    let mi = &candidate.monowave_indices;

    match candidate.wave_count {
        3 => {
            // 3-wave Zigzag/Flat:檢查 c/a Fib relationship
            if mi.len() < 3 {
                return RuleResult::NotApplicable(rid);
            }
            let a_mag = magnitude(&classified[mi[0]]);
            let c_mag = magnitude(&classified[mi[2]]);
            let c_over_a = match safe_pct(c_mag, a_mag) {
                Some(r) => r,
                None => return RuleResult::NotApplicable(rid),
            };
            if matches_any_fib_ratio(c_over_a) {
                RuleResult::Pass
            } else {
                // 無 Fib 對齊,但結構可能仍合法
                RuleResult::NotApplicable(rid)
            }
        }
        5 => {
            // 5-wave Impulse:檢查 W5/W1 或 W3/W1 任一 Fib relationship
            if mi.len() < 5 {
                return RuleResult::NotApplicable(rid);
            }
            let w1 = magnitude(&classified[mi[0]]);
            let w3 = magnitude(&classified[mi[2]]);
            let w5 = magnitude(&classified[mi[4]]);

            let w5_over_w1 = safe_pct(w5, w1);
            let w3_over_w1 = safe_pct(w3, w1);

            let has_w5_fib = w5_over_w1.map(matches_any_fib_ratio).unwrap_or(false);
            let has_w3_fib = w3_over_w1.map(matches_any_fib_ratio).unwrap_or(false);

            if has_w5_fib || has_w3_fib {
                RuleResult::Pass
            } else {
                RuleResult::NotApplicable(rid)
            }
        }
        _ => RuleResult::NotApplicable(rid),
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
            id: "c5".to_string(),
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
    fn w1_5wave_with_w3_extension_passes() {
        // W1=10, W3=25(>W1×1.1), W5=12 → W3 是延長波 → Pass
        let classified = vec![
            cmw(100.0, 110.0),
            cmw(110.0, 105.0),
            cmw(105.0, 130.0),
            cmw(130.0, 122.0),
            cmw(122.0, 134.0),
        ];
        assert!(rule_w1(&cand_5wave(), &classified).is_pass());
    }

    #[test]
    fn w1_3wave_not_applicable() {
        let classified = vec![cmw(100.0, 110.0), cmw(110.0, 105.0), cmw(105.0, 120.0)];
        assert!(matches!(rule_w1(&cand_3wave(), &classified), RuleResult::NotApplicable(_)));
    }

    #[test]
    fn w1_zero_magnitude_not_applicable() {
        // W3 magnitude 0 → NotApplicable
        let classified = vec![
            cmw(100.0, 110.0),
            cmw(110.0, 105.0),
            cmw(105.0, 105.0), // zero magnitude
            cmw(105.0, 100.0),
            cmw(100.0, 115.0),
        ];
        assert!(matches!(rule_w1(&cand_5wave(), &classified), RuleResult::NotApplicable(_)));
    }

    #[test]
    fn w2_3wave_c_equals_a_passes() {
        // 3-wave c/a = 100% (matches Fib 100%) → Pass
        let classified = vec![cmw(100.0, 110.0), cmw(110.0, 105.0), cmw(105.0, 115.0)];
        let r = rule_w2(&cand_3wave(), &classified);
        assert!(r.is_pass(), "c/a = 100% 應 Pass, got {:?}", r);
    }

    #[test]
    fn w2_3wave_c_618_a_passes() {
        // 3-wave c/a = 61.8% (Fib match) → Pass
        let classified = vec![cmw(100.0, 110.0), cmw(110.0, 105.0), cmw(105.0, 111.18)];
        let r = rule_w2(&cand_3wave(), &classified);
        assert!(r.is_pass(), "c/a = 61.8% 應 Pass, got {:?}", r);
    }

    #[test]
    fn w2_3wave_no_fib_match_not_applicable() {
        // 3-wave c/a = 80%(no Fib match in 38.2/61.8/100/161.8/261.8 ±4%)
        let classified = vec![cmw(100.0, 110.0), cmw(110.0, 105.0), cmw(105.0, 113.0)];
        let r = rule_w2(&cand_3wave(), &classified);
        assert!(matches!(r, RuleResult::NotApplicable(_)),
            "c/a = 80% 無 Fib match 應 NotApplicable, got {:?}", r);
    }

    #[test]
    fn w2_5wave_w3_618x_w1_passes() {
        // W1=10, W3=16.18(W3/W1 = 161.8%), 隨意 W5
        let classified = vec![
            cmw(100.0, 110.0),
            cmw(110.0, 105.0),
            cmw(105.0, 121.18),
            cmw(121.18, 115.0),
            cmw(115.0, 125.0),
        ];
        let r = rule_w2(&cand_5wave(), &classified);
        assert!(r.is_pass(), "W3/W1=161.8% 應 Pass, got {:?}", r);
    }

    #[test]
    fn w2_5wave_no_fib_not_applicable() {
        // W1=10, W3=15, W5=20 → W3/W1 = 150%, W5/W1 = 200%, 都不是 Fib
        // 150 比 161.8 差 11.8(>4%);200 比 161.8 差 38.2(>4%),比 261.8 差 61.8(>4%)
        let classified = vec![
            cmw(100.0, 110.0),
            cmw(110.0, 105.0),
            cmw(105.0, 120.0),
            cmw(120.0, 115.0),
            cmw(115.0, 135.0),
        ];
        let r = rule_w2(&cand_5wave(), &classified);
        assert!(matches!(r, RuleResult::NotApplicable(_)),
            "150% / 200% 無 Fib match 應 NotApplicable, got {:?}", r);
    }

    #[test]
    fn fib_tolerance_constant() {
        assert_eq!(super::super::helpers::FIB_TOLERANCE_PCT, 4.0);
    }
}
