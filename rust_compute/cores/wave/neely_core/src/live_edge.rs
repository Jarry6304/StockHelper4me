// live_edge.rs — E4:live-edge 互斥候選歧義(m3Spec/wave_judgment_loop.md §3)
//
// 取代 Stage 10.5 Reverse Logic 觀察:判讀者要的不是「多解 = 更大形態中段」
// 的字串暗示,而是尾端到底有幾個互斥計數、什麼型。
//
// 候選集:`wave_tree.end` 落在最後 3 bars 內(end bar ≥ last_bar − 3)且
// degree_level = 候選中最大。互斥 = `pattern_type` 不同或 `end` 不同 —
// 同 end 同型僅子結構不同者不重計(distinct (pattern_tag, end))。

use std::collections::BTreeSet;

use crate::compaction::round_engine::pattern_tag;
use crate::output::{LiveEdgeAmbiguity, OhlcvBar, Scenario};

pub fn compute(forest: &[Scenario], bars: &[OhlcvBar]) -> LiveEdgeAmbiguity {
    if forest.is_empty() || bars.is_empty() {
        return LiveEdgeAmbiguity::default();
    }
    // cutoff = 倒數第 4 bar 的日期(end 日期 ≥ cutoff ⇔ end bar ≥ last − 3;
    // 資料不足 4 bars 時全部視為 live edge)
    let cutoff = bars[bars.len().saturating_sub(4)].date;
    let live: Vec<&Scenario> = forest
        .iter()
        .filter(|s| s.wave_tree.end >= cutoff)
        .collect();
    if live.is_empty() {
        return LiveEdgeAmbiguity::default();
    }
    let max_degree = live
        .iter()
        .map(|s| s.wave_tree.degree_level)
        .max()
        .unwrap_or(0);
    let mut pairs: BTreeSet<(String, chrono::NaiveDate)> = BTreeSet::new();
    let mut kinds: BTreeSet<String> = BTreeSet::new();
    for s in live
        .into_iter()
        .filter(|s| s.wave_tree.degree_level == max_degree)
    {
        let tag = pattern_tag(&s.pattern_type);
        pairs.insert((tag.clone(), s.wave_tree.end));
        kinds.insert(tag);
    }
    LiveEdgeAmbiguity {
        count: pairs.len(),
        kinds: kinds.into_iter().collect(),
        degree_level: max_degree,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::{NeelyPatternType, ZigzagKind};
    use chrono::NaiveDate;

    fn bars(n: usize) -> Vec<OhlcvBar> {
        (0..n)
            .map(|i| OhlcvBar {
                date: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap()
                    + chrono::Duration::days(i as i64),
                open: 100.0,
                high: 101.0,
                low: 99.0,
                close: 100.0,
                volume: None,
            })
            .collect()
    }

    fn scenario(pattern: NeelyPatternType, end_day: i64, degree: usize) -> Scenario {
        let mut s = Scenario::test_minimal();
        s.pattern_type = pattern;
        s.wave_tree.end =
            NaiveDate::from_ymd_opt(2026, 1, 1).unwrap() + chrono::Duration::days(end_day);
        s.wave_tree.degree_level = degree;
        s
    }

    #[test]
    fn same_end_same_pattern_substructure_variants_count_once() {
        let b = bars(20); // last bar = day 19;cutoff = day 16
        let forest = vec![
            scenario(NeelyPatternType::Impulse, 19, 2),
            scenario(NeelyPatternType::Impulse, 19, 2), // 子結構異體 → 不重計
            scenario(
                NeelyPatternType::Zigzag { sub_kind: ZigzagKind::Single },
                19,
                2,
            ),
        ];
        let amb = compute(&forest, &b);
        assert_eq!(amb.count, 2, "同 end 同型不重計;異型計 2");
        assert_eq!(amb.kinds.len(), 2);
        assert_eq!(amb.degree_level, 2);
    }

    #[test]
    fn different_end_same_pattern_counts_twice() {
        let b = bars(20);
        let forest = vec![
            scenario(NeelyPatternType::Impulse, 19, 2),
            scenario(NeelyPatternType::Impulse, 17, 2), // end 不同 → 互斥
        ];
        let amb = compute(&forest, &b);
        assert_eq!(amb.count, 2);
        assert_eq!(amb.kinds, vec!["Impulse".to_string()]);
    }

    #[test]
    fn lower_degree_and_historical_excluded() {
        let b = bars(20);
        let forest = vec![
            scenario(NeelyPatternType::Impulse, 19, 3),
            scenario(
                NeelyPatternType::Zigzag { sub_kind: ZigzagKind::Single },
                19,
                1, // 低 degree → 排除
            ),
            scenario(NeelyPatternType::Impulse, 10, 3), // end < last−3 → 排除
        ];
        let amb = compute(&forest, &b);
        assert_eq!(amb.count, 1);
        assert_eq!(amb.degree_level, 3);
    }
}
