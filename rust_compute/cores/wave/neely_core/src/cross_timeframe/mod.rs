// cross_timeframe — Stage 12:cross_timeframe_hints 計算
//
// 對齊 m3Spec/neely_core_architecture.md §8.6 CrossTimeframeHints + §3.4 Gap 3.4
//       + §7.1 Stage 12
//
// **Phase 12 PR(r5 alignment)**:
//   - 為每個 classified_monowave 產出一條 MonowaveSummary:
//     monowave_index / date_range / structure_label_candidates / price_range
//   - 供 Aggregation Layer 跨 Timeframe 比對(避免 Aggregation 重新解析 structural_snapshots)

use crate::monowave::ClassifiedMonowave;
use crate::output::{CrossTimeframeHints, MonowaveSummary, StructureLabel};
use fact_schema::Timeframe;

/// Stage 12 主入口:從 classified_monowaves 產出 CrossTimeframeHints。
pub fn compute_hints(
    classified: &[ClassifiedMonowave],
    timeframe: Timeframe,
) -> CrossTimeframeHints {
    let monowave_summaries = classified
        .iter()
        .enumerate()
        .map(|(idx, cmw)| MonowaveSummary {
            monowave_index: idx,
            date_range: (cmw.monowave.start_date, cmw.monowave.end_date),
            structure_label_candidates: cmw
                .structure_label_candidates
                .iter()
                .map(|c| label_to_string(c.label))
                .collect(),
            price_range: (cmw.monowave.start_price, cmw.monowave.end_price),
        })
        .collect();

    CrossTimeframeHints {
        timeframe,
        monowave_summaries,
    }
}

/// StructureLabel → spec 標準字串(對齊 architecture §8.6 範例「:L5 / :F3」)。
fn label_to_string(label: StructureLabel) -> String {
    match label {
        StructureLabel::Five => ":5".to_string(),
        StructureLabel::Three => ":3".to_string(),
        StructureLabel::F3 => ":F3".to_string(),
        StructureLabel::C3 => ":c3".to_string(),
        StructureLabel::L3 => ":L3".to_string(),
        StructureLabel::UnknownThree => ":?3".to_string(),
        StructureLabel::F5 => ":F5".to_string(),
        StructureLabel::L5 => ":L5".to_string(),
        StructureLabel::UnknownFive => ":?5".to_string(),
        StructureLabel::S5 => ":s5".to_string(),
        StructureLabel::SL3 => ":sL3".to_string(),
        StructureLabel::SL5 => ":sL5".to_string(),
        StructureLabel::XC3 => "x:c3".to_string(),
        StructureLabel::BC3 => "b:c3".to_string(),
        StructureLabel::BF3 => "b:F3".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::monowave::ProportionMetrics;
    use crate::output::{Certainty, Monowave, MonowaveDirection, StructureLabelCandidate};
    use chrono::NaiveDate;

    fn cmw(labels: Vec<StructureLabel>) -> ClassifiedMonowave {
        let candidates = labels
            .into_iter()
            .map(|l| StructureLabelCandidate {
                label: l,
                certainty: Certainty::Primary,
            })
            .collect();
        ClassifiedMonowave {
            monowave: Monowave {
                start_date: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                end_date: NaiveDate::from_ymd_opt(2026, 1, 5).unwrap(),
                start_price: 100.0,
                end_price: 110.0,
                direction: MonowaveDirection::Up,
            },
            atr_at_start: 1.0,
            metrics: ProportionMetrics {
                magnitude: 10.0,
                duration_bars: 5,
                atr_relative: 10.0,
                slope_vs_45deg: 1.0,
            },
            structure_label_candidates: candidates,
        }
    }

    #[test]
    fn empty_classified_yields_empty_summaries() {
        let hints = compute_hints(&[], Timeframe::Daily);
        assert_eq!(hints.timeframe, Timeframe::Daily);
        assert!(hints.monowave_summaries.is_empty());
    }

    #[test]
    fn each_monowave_produces_one_summary() {
        let classified = vec![
            cmw(vec![StructureLabel::F5]),
            cmw(vec![StructureLabel::L5, StructureLabel::F3]),
        ];
        let hints = compute_hints(&classified, Timeframe::Weekly);
        assert_eq!(hints.monowave_summaries.len(), 2);
        assert_eq!(hints.monowave_summaries[0].monowave_index, 0);
        assert_eq!(
            hints.monowave_summaries[0].structure_label_candidates,
            vec![":F5".to_string()]
        );
        assert_eq!(
            hints.monowave_summaries[1].structure_label_candidates,
            vec![":L5".to_string(), ":F3".to_string()]
        );
    }

    #[test]
    fn summary_carries_price_range_and_dates() {
        let classified = vec![cmw(vec![StructureLabel::Five])];
        let hints = compute_hints(&classified, Timeframe::Daily);
        let s = &hints.monowave_summaries[0];
        assert_eq!(s.price_range, (100.0, 110.0));
        assert_eq!(s.date_range.0, NaiveDate::from_ymd_opt(2026, 1, 1).unwrap());
        assert_eq!(s.date_range.1, NaiveDate::from_ymd_opt(2026, 1, 5).unwrap());
    }
}
