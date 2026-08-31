// robustness.rs — E2:噪音穩健度(m3Spec/wave_judgment_loop.md §3)
//
// REVERSAL_ATR_MULTIPLIER ∈ {0.3, 0.5, 0.7} 三組偵測,scenario 的 wave_tree
// 頂層 children 端點日期三組皆存在 → robust = true。只重跑 detect(Stage 1);
// 分類(neutrality / proportion)不移動端點,不需重跑;compaction 端點鍵
// 位置相依,無法跨偵測組共用 — 成本即 Stage 1 ×2 額外組(≈ +5%)。
// multiplier 變體**不進 NeelyEngineConfig**:進 config = params_hash 變 =
// structural_snapshots 另立 row + `fetch_structural_latest` 讀取不確定。

use std::collections::HashSet;

use chrono::NaiveDate;

use super::pure_close::detect_monowaves_with_multiplier;
use crate::output::{OhlcvBar, Scenario};

/// E2 偵測組(中值 = serving 的 REVERSAL_ATR_MULTIPLIER)。
pub(crate) const E2_ROBUST_MULTIPLIERS: [f64; 3] = [0.3, 0.5, 0.7];

/// 單一 multiplier 的 monowave 端點日期集合。start/end 全收,Neutral monowave
/// 端點自然在內 —「Neutral 合成葉端點視同存在」由此成立(合成葉端點 =
/// Neutral 段終點 = 該 Neutral monowave 的 end_date)。
fn endpoint_dates(bars: &[OhlcvBar], atr_period: usize, multiplier: f64) -> HashSet<NaiveDate> {
    detect_monowaves_with_multiplier(bars, atr_period, multiplier)
        .iter()
        .flat_map(|m| [m.start_date, m.end_date])
        .collect()
}

/// 對 forest 全體 scenario 填 `robust`(0.5 組 = serving 偵測本身,恆真;
/// 只需驗 0.3 ∩ 0.7)。凍結預設 true,此處覆寫。
pub fn apply_robustness(forest: &mut [Scenario], bars: &[OhlcvBar], atr_period: usize) {
    if forest.is_empty() || bars.is_empty() {
        return;
    }
    let set_lo = endpoint_dates(bars, atr_period, E2_ROBUST_MULTIPLIERS[0]);
    let set_hi = endpoint_dates(bars, atr_period, E2_ROBUST_MULTIPLIERS[2]);
    for s in forest.iter_mut() {
        s.robust = s
            .wave_tree
            .children
            .iter()
            .flat_map(|c| [c.start, c.end])
            .all(|d| set_lo.contains(&d) && set_hi.contains(&d));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn bar(day: u32, h: f64, l: f64) -> OhlcvBar {
        OhlcvBar {
            date: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap() + chrono::Duration::days(day as i64),
            open: (h + l) / 2.0,
            high: h,
            low: l,
            close: (h + l) / 2.0,
            volume: None,
        }
    }

    /// 大幅走勢 + 一段僅在低門檻(0.3×ATR)成立的邊際反轉:
    /// 0.7 組看不到該端點 → 依附它的 scenario robust = false。
    #[test]
    fn marginal_pivot_vanishing_at_high_threshold_marks_not_robust() {
        // 走勢:強漲 → 微幅回檔(movement 介於 0.3×ATR 與 0.7×ATR 之間)→ 強漲
        let mut bars: Vec<OhlcvBar> = Vec::new();
        let mut price = 100.0;
        for d in 0..10 {
            price += 5.0;
            bars.push(bar(d, price + 1.0, price - 1.0)); // ATR ≈ 5-6
        }
        // 微幅回檔 2 天(每日 -1.5;累計 -3 介於 0.3×ATR(≈1.7)與 0.7×ATR(≈3.9)
        // 之間 → 僅低門檻組視為反轉)
        for d in 10..12 {
            price -= 1.5;
            bars.push(bar(d, price + 1.0, price - 1.0));
        }
        for d in 12..22 {
            price += 5.0;
            bars.push(bar(d, price + 1.0, price - 1.0));
        }

        let lo = endpoint_dates(&bars, 14, 0.3);
        let hi = endpoint_dates(&bars, 14, 0.7);
        // 前提確認:低門檻端點集合是高門檻的嚴格超集(有邊際端點)
        assert!(lo.len() > hi.len(), "0.3 組應多出邊際端點:lo={} hi={}", lo.len(), hi.len());

        // 造兩個 scenario:一個 joints 全在 hi(robust),一個含邊際端點
        let marginal: Vec<NaiveDate> = lo.difference(&hi).cloned().collect();
        assert!(!marginal.is_empty());
        let stable: Vec<NaiveDate> = hi.intersection(&lo).cloned().collect();
        assert!(stable.len() >= 2);

        let mk = |joints: &[NaiveDate]| -> Scenario {
            let mut s = Scenario::test_minimal();
            s.wave_tree.children = joints
                .iter()
                .map(|d| crate::output::WaveNode {
                    label: "x".to_string(),
                    start: *d,
                    end: *d,
                    degree_level: 0,
                    base_label: crate::output::StructureLabel::Three,
                    children: Vec::new(),
                })
                .collect();
            s
        };
        let mut forest = vec![mk(&stable[..2]), mk(&marginal[..1])];
        apply_robustness(&mut forest, &bars, 14);
        assert!(forest[0].robust, "端點全在三組 → robust");
        assert!(!forest[1].robust, "僅低門檻存在的端點 → 非 robust");
    }
}
