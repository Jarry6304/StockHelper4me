// fifth_of_fifth_detector.rs — Appendix A.3 共通函式
//
// 對齊 m3Spec/neely_rules.md Appendix A.3 「5th-of-5th Extension 共通條件」+ spec line 526
// (`m2_breaches_2_4_line_within_m1_time` predicate)。
//
// **v4.1 落地**(2026-05-19):
// 從 `rule_3.rs::check_fifth_of_fifth_and_add` + `rule_4.rs::add_l5_if_fifth_of_fifth`
// 兩處 byte-for-byte 重複實作抽出共通 fn。對齊 plan v4.0 P1.1 #5 +
// m3Spec/neely_core_architecture.md §四「禁止抽象」原則(此處屬「真重複」可抽,
// 非 over-engineering)。
//
// **判定**:
//   IF m1 為 (m(-3), m1, m(-1)) magnitude 中最長
//   AND m2 在 ≤ m1 時間內突破 m(-2)/m0 連線(2-4 trendline breach)
//   → 該 cands 額外 add `:L5`(Certainty::Rare)

use super::context::MonowaveContext;
use super::predicates::{
    add_or_promote, is_longest_of_three, m2_breaches_2_4_line_within_m1_time,
};
use crate::output::{Certainty, StructureLabel, StructureLabelCandidate};

/// 5th-of-5th Extension 共通條件:符合 → add `:L5`(Certainty::Rare)。
///
/// 對齊 `rule_3.rs` Cond 3a 每子規則 + `rule_4.rs` 多 branch 共用判定。
pub fn add_l5_if_fifth_of_fifth(
    ctx: &MonowaveContext,
    cands: &mut Vec<StructureLabelCandidate>,
) {
    if let (Some(m_minus_3), Some(m_minus_2), Some(m_minus_1), Some(m0), Some(m2)) = (
        ctx.m_minus_3,
        ctx.m_minus_2,
        ctx.m_minus_1,
        ctx.m0,
        ctx.m2,
    ) {
        let m1 = ctx.m1;
        let m1_longest = is_longest_of_three(m1, Some(m_minus_1), Some(m_minus_3));
        let breaches = m2_breaches_2_4_line_within_m1_time(m_minus_2, m0, m1, m2);
        if m1_longest && breaches {
            add_or_promote(cands, StructureLabel::L5, Certainty::Rare);
        }
        // 抑制 unused warning 對 m0(僅 breaches 內用)
        let _ = m0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::monowave::{ClassifiedMonowave, ProportionMetrics};
    use crate::output::{Monowave, MonowaveDirection};
    use chrono::NaiveDate;

    fn cmw(start_p: f64, end_p: f64, dir: MonowaveDirection, dur: usize, day: u32) -> ClassifiedMonowave {
        ClassifiedMonowave {
            monowave: Monowave {
                start_date: NaiveDate::from_ymd_opt(2026, 1, day).unwrap(),
                end_date: NaiveDate::from_ymd_opt(2026, 1, day + dur as u32).unwrap(),
                start_price: start_p,
                end_price: end_p,
                direction: dir,
                bar_indices: (0, 0),
            },
            atr_at_start: 1.0,
            metrics: ProportionMetrics {
                magnitude: (end_p - start_p).abs(),
                duration_bars: dur,
                atr_relative: 1.0,
                slope_vs_45deg: 1.0,
            },
            structure_label_candidates: Vec::new(),
            polywave_size: 0,
        }
    }

    #[test]
    fn detector_no_op_when_m1_not_longest() {
        // m_minus_3 mag = 30,m1 mag = 5,m_minus_1 mag = 10 → m1 非最長
        // 用 MonowaveContext::build 從 slice 建立而非手動構造(避免 i / classified 漏填)
        let classified = vec![
            cmw(0.0, 30.0, MonowaveDirection::Up, 5, 1),    // index 0 = m_minus_4
            cmw(0.0, 30.0, MonowaveDirection::Up, 5, 1),    // index 1 = m_minus_3
            cmw(30.0, 20.0, MonowaveDirection::Down, 3, 7), // index 2 = m_minus_2
            cmw(20.0, 30.0, MonowaveDirection::Up, 4, 11),  // index 3 = m_minus_1 (mag 10)
            cmw(30.0, 25.0, MonowaveDirection::Down, 3, 16),// index 4 = m0
            cmw(25.0, 30.0, MonowaveDirection::Up, 5, 20),  // index 5 = m1 (mag 5)
            cmw(30.0, 28.0, MonowaveDirection::Down, 2, 26),// index 6 = m2
        ];
        let ctx = MonowaveContext::build(&classified, &[], 5).expect("應建立");
        let mut cands: Vec<StructureLabelCandidate> = Vec::new();
        add_l5_if_fifth_of_fifth(&ctx, &mut cands);
        assert!(cands.is_empty(), "m1 非最長,不應加 L5");
    }

    #[test]
    fn detector_no_op_when_required_context_missing() {
        // Slice 太短 → m_minus_3/m_minus_2/m_minus_1 都會是 None
        let classified = vec![
            cmw(100.0, 120.0, MonowaveDirection::Up, 5, 1),
            cmw(120.0, 115.0, MonowaveDirection::Down, 3, 7),
        ];
        let ctx = MonowaveContext::build(&classified, &[], 0).expect("應建立");
        let mut cands: Vec<StructureLabelCandidate> = Vec::new();
        add_l5_if_fifth_of_fifth(&ctx, &mut cands);
        assert!(cands.is_empty(), "context 缺 m_minus_*,不應加 L5");
    }
}
