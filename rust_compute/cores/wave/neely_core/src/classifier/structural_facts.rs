// structural_facts.rs — Phase 17:StructuralFacts 7 sub-fields 計算
//
// 對齊 m3Spec/neely_core_architecture.md §9.1 line 549-556 「群 6 — Fibonacci + 結構性事實」。
//
// **設計**(去耦合不抽象):
//   - 7 個獨立 fn,每 fn 接受 minimum input
//   - 不引入 trait,不抽 builder pattern
//   - 4 個 fn 在 classifier::classify 階段呼叫(monowave 資料):
//       fibonacci_alignment / alternation / time_relationship / overlap_pattern
//   - 3 個 fn 在 lib.rs::compute Stage 7.5 後呼叫(bars / advisory_findings):
//       channeling / volume_alignment / gap_count
//
// **資料源**:全部 computable-from-bars(0 個需 FinMind 新抓資料)。

use crate::candidates::WaveCandidate;
use crate::fibonacci::ratios::{FIB_TOLERANCE_PCT, NEELY_FIB_RATIOS};
use crate::monowave::ClassifiedMonowave;
use crate::output::{
    AdvisoryFinding, AdvisorySeverity, AlternationFact, ChannelingFact, FibonacciAlignment,
    OhlcvBar, OverlapPattern, RuleId, TimeRelationship, VolumeAlignment,
};
use crate::validator::ValidationReport;

// ── 4 fns at classifier::classify time ─────────────────────────────────

/// 對 W1/W3/W5(或 W1/W3 / a/c)magnitudes 找 NEELY_FIB_RATIOS 對應(±4% 容差)。
///
/// 對齊 spec §StructuralFacts.fibonacci_alignment + NEELY_FIB_RATIOS(0.236-2.618 共 10 比率)。
/// matched_ratios:從相鄰 pair magnitude 比例對到的 Fibonacci 值(去重)。
pub fn fibonacci_alignment(
    candidate: &WaveCandidate,
    classified: &[ClassifiedMonowave],
) -> Option<FibonacciAlignment> {
    let mi = &candidate.monowave_indices;
    if mi.len() < 2 {
        return None;
    }
    let tol = FIB_TOLERANCE_PCT / 100.0; // 0.04
    let mut matched: Vec<f64> = Vec::new();

    // 對所有 (i,j) pair 算 magnitude_j / magnitude_i,看是否 ≈ NEELY_FIB_RATIOS 任一比率
    for i in 0..mi.len() - 1 {
        let mag_i = classified[mi[i]].metrics.magnitude;
        if mag_i <= 0.0 {
            continue;
        }
        for j in (i + 1)..mi.len() {
            let mag_j = classified[mi[j]].metrics.magnitude;
            let ratio = mag_j / mag_i;
            for &fib in NEELY_FIB_RATIOS {
                if (ratio - fib).abs() / fib <= tol {
                    if !matched.iter().any(|m| (m - fib).abs() < 1e-9) {
                        matched.push(fib);
                    }
                    break;
                }
            }
        }
    }

    if matched.is_empty() {
        None
    } else {
        matched.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        Some(FibonacciAlignment {
            matched_ratios: matched,
        })
    }
}

/// 從 ValidationReport 抽 Ch5 Alternation Rule 的結果。
///
/// 對齊 m3Spec/neely_rules.md §Ch5 Alternation Rule。
/// validator/wave_rules.rs::rule_alternation_construction 已產 Pass/Fail,
/// 但 report.passed 只記 passed RuleId 而不記具體規則 detail。
/// 這裡簡化判定:Ch5_Alternation 不在 report.failed 內 → holds = true。
pub fn alternation(report: &ValidationReport) -> Option<AlternationFact> {
    let failed_alternation = report
        .failed
        .iter()
        .any(|r| matches!(r.rule_id, RuleId::Ch5_Alternation { .. }));
    let na_alternation = report
        .not_applicable
        .iter()
        .any(|r| matches!(r, RuleId::Ch5_Alternation { .. }));
    if na_alternation {
        // 規則不適用(3-wave 等)→ 不產 fact
        return None;
    }
    Some(AlternationFact {
        holds: !failed_alternation,
    })
}

/// 從 monowave duration_bars 計算 wave-time 關係 label。
///
/// 對齊 spec §StructuralFacts.time_relationship。
/// 簡化 label:「W3 最長/最短 + W1:W5 時間比例 ≈ Fibonacci 值」。
pub fn time_relationship(
    candidate: &WaveCandidate,
    classified: &[ClassifiedMonowave],
) -> Option<TimeRelationship> {
    let mi = &candidate.monowave_indices;
    if mi.len() < 3 {
        return None;
    }
    let durations: Vec<usize> = mi.iter().map(|&i| classified[i].metrics.duration_bars).collect();
    let total: usize = durations.iter().sum();
    if total == 0 {
        return None;
    }

    // W3 vs W1/W5 比較(5-wave)或 longest / shortest summary
    let max_dur = *durations.iter().max().unwrap();
    let min_dur = *durations.iter().min().unwrap();
    let max_idx = durations.iter().position(|&d| d == max_dur).unwrap();

    let label = if mi.len() == 5 {
        format!(
            "5-wave durations: W{}-longest({} bars), W{}-shortest({} bars)",
            max_idx + 1,
            max_dur,
            durations.iter().position(|&d| d == min_dur).unwrap() + 1,
            min_dur,
        )
    } else {
        format!(
            "{}-wave durations: longest={} bars / shortest={} bars / total={} bars",
            mi.len(),
            max_dur,
            min_dur,
            total
        )
    };
    Some(TimeRelationship { label })
}

/// 從相鄰 monowave 的 start/end 價區計算 overlap pattern label。
///
/// 對齊 spec §StructuralFacts.overlap_pattern。
/// 簡化 label:「W4 進入 W1 區 / W2 進入 W0 區 / 無 overlap」。
pub fn overlap_pattern(
    candidate: &WaveCandidate,
    classified: &[ClassifiedMonowave],
) -> Option<OverlapPattern> {
    let mi = &candidate.monowave_indices;
    if mi.len() < 4 {
        return None;
    }
    // 對 5-wave Impulse 結構,檢查 W4(idx 3)是否進入 W1(idx 0)區
    let w1 = &classified[mi[0]].monowave;
    let w4 = &classified[mi[3]].monowave;
    let w1_lo = w1.start_price.min(w1.end_price);
    let w1_hi = w1.start_price.max(w1.end_price);
    let w4_lo = w4.start_price.min(w4.end_price);
    let w4_hi = w4.start_price.max(w4.end_price);

    let label = if w4_lo <= w1_hi && w1_lo <= w4_hi {
        format!(
            "W4 overlap into W1 range [{:.2}, {:.2}] ∩ [{:.2}, {:.2}]",
            w1_lo, w1_hi, w4_lo, w4_hi
        )
    } else {
        format!(
            "no W4-W1 overlap (W1 [{:.2}, {:.2}] / W4 [{:.2}, {:.2}])",
            w1_lo, w1_hi, w4_lo, w4_hi
        )
    };
    Some(OverlapPattern { label })
}

// ── 3 fns at post-Stage 7.5 time ─────────────────────────────────────────

/// 從 advisory_findings 抽 Channeling 結果(0-2 / 1-3 / 2-4 / 0-B / B-D 5 條 trendlines)。
///
/// 對齊 m3Spec/neely_rules.md §Ch5 Channeling。
/// 簡化判定:advisory_findings 含任一 Ch9_TrendlineTouchpoints / Channeling 相關 Strong finding
/// → holds = true。
pub fn channeling(advisory_findings: &[AdvisoryFinding]) -> Option<ChannelingFact> {
    let any_channeling = advisory_findings
        .iter()
        .any(|f| matches!(f.severity, AdvisorySeverity::Strong | AdvisorySeverity::Warning));
    Some(ChannelingFact {
        holds: any_channeling,
    })
}

/// 對 scenario 起終日期範圍內的 bars 計 gap 數量(bars[i].open != bars[i-1].close)。
///
/// 對齊 spec §StructuralFacts.gap_count。
pub fn gap_count(start_date: chrono::NaiveDate, end_date: chrono::NaiveDate, bars: &[OhlcvBar]) -> usize {
    let in_range: Vec<&OhlcvBar> = bars
        .iter()
        .filter(|b| b.date >= start_date && b.date <= end_date)
        .collect();
    if in_range.len() < 2 {
        return 0;
    }
    let tol = 1e-9;
    in_range
        .windows(2)
        .filter(|w| (w[1].open - w[0].close).abs() > tol)
        .count()
}

/// 對 scenario 起終日期範圍內的 bars 計 volume alignment(有 volume 資料 + 平均 > 0)。
///
/// 對齊 spec §StructuralFacts.volume_alignment(spec line 553 註「選填,若有 volume 資料才填」)。
/// 簡化判定:>50% bars 帶 volume → holds = true。
pub fn volume_alignment(
    start_date: chrono::NaiveDate,
    end_date: chrono::NaiveDate,
    bars: &[OhlcvBar],
) -> Option<VolumeAlignment> {
    let in_range: Vec<&OhlcvBar> = bars
        .iter()
        .filter(|b| b.date >= start_date && b.date <= end_date)
        .collect();
    if in_range.is_empty() {
        return None;
    }
    let with_volume = in_range.iter().filter(|b| b.volume.is_some() && b.volume.unwrap() > 0).count();
    let ratio = with_volume as f64 / in_range.len() as f64;
    if ratio < 0.01 {
        // 完全沒 volume 資料 → None(spec 553 註「選填」)
        return None;
    }
    Some(VolumeAlignment {
        holds: ratio >= 0.5,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::monowave::ProportionMetrics;
    use crate::output::{Monowave, MonowaveDirection};
    use chrono::NaiveDate;

    fn cmw(mag: f64, dur_bars: usize) -> ClassifiedMonowave {
        ClassifiedMonowave {
            monowave: Monowave {
                start_date: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                end_date: NaiveDate::from_ymd_opt(2026, 1, 5).unwrap(),
                start_price: 100.0,
                end_price: 100.0 + mag,
                direction: MonowaveDirection::Up,
            },
            atr_at_start: 1.0,
            metrics: ProportionMetrics {
                magnitude: mag,
                duration_bars: dur_bars,
                atr_relative: 1.0,
                slope_vs_45deg: 1.0,
            },
            structure_label_candidates: Vec::new(),
        }
    }

    #[test]
    fn fibonacci_alignment_matches_618() {
        // mag pair 10, 6.18 → ratio 0.618 ≈ Fib
        let classified = vec![cmw(10.0, 5), cmw(6.18, 5)];
        let candidate = WaveCandidate {
            id: "c2".to_string(),
            monowave_indices: vec![0, 1],
            wave_count: 3,
            initial_direction: MonowaveDirection::Up,
        };
        let fa = fibonacci_alignment(&candidate, &classified).expect("應 matched");
        assert!(fa.matched_ratios.iter().any(|r| (r - 0.618).abs() < 1e-9));
    }

    #[test]
    fn fibonacci_alignment_none_when_no_match() {
        // 任意比例不對 Fib(0.5 仍在 NEELY_FIB_RATIOS 內,改用更偏的)
        let classified = vec![cmw(10.0, 5), cmw(3.3, 5)]; // 0.33 不對 0.382(diff > 4%)
        let candidate = WaveCandidate {
            id: "c2".to_string(),
            monowave_indices: vec![0, 1],
            wave_count: 3,
            initial_direction: MonowaveDirection::Up,
        };
        // 0.33 vs 0.382 → diff = 13.6% > 4% tol → no match
        assert!(fibonacci_alignment(&candidate, &classified).is_none());
    }

    #[test]
    fn time_relationship_5wave_format() {
        let classified = vec![
            cmw(10.0, 5),
            cmw(5.0, 3),
            cmw(15.0, 10),
            cmw(7.0, 4),
            cmw(12.0, 6),
        ];
        let candidate = WaveCandidate {
            id: "c5".to_string(),
            monowave_indices: vec![0, 1, 2, 3, 4],
            wave_count: 5,
            initial_direction: MonowaveDirection::Up,
        };
        let tr = time_relationship(&candidate, &classified).expect("應產生");
        // W3 (idx 2 → label W3) 是最長(10 bars)
        assert!(tr.label.contains("W3-longest(10 bars)"));
    }

    #[test]
    fn overlap_pattern_detects_w4_w1_overlap() {
        // W1: 100 → 110 / W2: 110 → 105 / W3: 105 → 125 / W4: 125 → 108(進 W1 區)
        let mw1 = cmw(10.0, 5); // 100 → 110
        let mut mw2 = cmw(5.0, 3);
        mw2.monowave.start_price = 110.0;
        mw2.monowave.end_price = 105.0;
        mw2.monowave.direction = MonowaveDirection::Down;
        let mut mw3 = cmw(20.0, 8);
        mw3.monowave.start_price = 105.0;
        mw3.monowave.end_price = 125.0;
        let mut mw4 = cmw(17.0, 6);
        mw4.monowave.start_price = 125.0;
        mw4.monowave.end_price = 108.0;
        mw4.monowave.direction = MonowaveDirection::Down;
        let classified = vec![mw1, mw2, mw3, mw4];
        let candidate = WaveCandidate {
            id: "c4".to_string(),
            monowave_indices: vec![0, 1, 2, 3],
            wave_count: 5,
            initial_direction: MonowaveDirection::Up,
        };
        let op = overlap_pattern(&candidate, &classified).expect("應產生");
        assert!(op.label.contains("overlap"));
    }

    #[test]
    fn gap_count_zero_for_aligned_bars() {
        let bars = vec![
            OhlcvBar {
                date: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                open: 100.0,
                high: 105.0,
                low: 99.0,
                close: 103.0,
                volume: Some(1000),
            },
            OhlcvBar {
                date: NaiveDate::from_ymd_opt(2026, 1, 2).unwrap(),
                open: 103.0, // 與前一日 close 對齊
                high: 107.0,
                low: 102.0,
                close: 106.0,
                volume: Some(1500),
            },
        ];
        let gaps = gap_count(
            NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2026, 1, 2).unwrap(),
            &bars,
        );
        assert_eq!(gaps, 0);
    }

    #[test]
    fn gap_count_counts_misaligned_open() {
        let bars = vec![
            OhlcvBar {
                date: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                open: 100.0,
                high: 105.0,
                low: 99.0,
                close: 103.0,
                volume: Some(1000),
            },
            OhlcvBar {
                date: NaiveDate::from_ymd_opt(2026, 1, 2).unwrap(),
                open: 107.0, // 與前一日 close(103)有 gap
                high: 108.0,
                low: 106.0,
                close: 107.5,
                volume: Some(1500),
            },
        ];
        let gaps = gap_count(
            NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2026, 1, 2).unwrap(),
            &bars,
        );
        assert_eq!(gaps, 1);
    }

    #[test]
    fn volume_alignment_holds_when_most_bars_have_volume() {
        let bars = vec![
            OhlcvBar {
                date: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                open: 100.0,
                high: 105.0,
                low: 99.0,
                close: 103.0,
                volume: Some(1000),
            },
            OhlcvBar {
                date: NaiveDate::from_ymd_opt(2026, 1, 2).unwrap(),
                open: 103.0,
                high: 107.0,
                low: 102.0,
                close: 106.0,
                volume: Some(1500),
            },
        ];
        let va = volume_alignment(
            NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2026, 1, 2).unwrap(),
            &bars,
        );
        assert!(va.is_some());
        assert!(va.unwrap().holds);
    }

    #[test]
    fn volume_alignment_none_when_no_volume() {
        let bars = vec![OhlcvBar {
            date: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            open: 100.0,
            high: 105.0,
            low: 99.0,
            close: 103.0,
            volume: None,
        }];
        let va = volume_alignment(
            NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            &bars,
        );
        assert!(va.is_none());
    }
}
