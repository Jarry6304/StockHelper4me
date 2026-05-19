// structural_facts.rs — Phase 17 / v4.1:StructuralFacts 8 sub-fields 計算
//
// 對齊 m3Spec/neely_core_architecture.md §9.1 line 549-556 「群 6 — Fibonacci + 結構性事實」
//      + m3Spec/neely_rules.md §Rule of Alternation 五軸 + §Ch8 Independent Rule。
//
// **設計**(去耦合不抽象):
//   - 8 個獨立 fn,每 fn 接受 minimum input
//   - 不引入 trait,不抽 builder pattern
//   - 5 個 fn 在 classifier::classify 階段呼叫(monowave + report):
//       fibonacci_alignment / alternation / time_relationship / overlap_pattern /
//       extension_subdivision_pair(v4.1 新)
//   - 3 個 fn 在 lib.rs::compute Stage 7.5 後呼叫(bars / advisory_findings):
//       channeling / volume_alignment / gap_count
//
// **資料源**:全部 computable-from-bars(0 個需 FinMind 新抓資料)。
//
// **v4.1 變動**(2026-05-19):
//   - alternation():從單軸 `holds: bool` 升 5-axis(Price/Time/Severity/Intricacy/Construction)
//   - overlap_pattern():從 `{ label: String }` 升 enum `Trending/Terminal/None` + evidence
//   - time_relationship():加 durations_bars + fibonacci_ratios_matched evidence
//   - channeling():加 evidence Vec<String>
//   - extension_subdivision_pair():新 fn,對 5-wave Impulse 判延伸段獨立性

use crate::candidates::WaveCandidate;
use crate::fibonacci::ratios::{FIB_TOLERANCE_PCT, NEELY_FIB_RATIOS};
use crate::monowave::ClassifiedMonowave;
use crate::output::{
    AdvisoryFinding, AdvisorySeverity, AlternationCheck, AlternationFact, ChannelingFact,
    ExtensionSubdivisionPair, FibonacciAlignment, MonowaveDirection, OhlcvBar, OverlapPattern,
    RuleId, SubdivisionStatus, TimeRelationship, VolumeAlignment, WaveNumber,
};
use crate::validator::ValidationReport;

// ── 5 fns at classifier::classify time ─────────────────────────────────

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

/// Alternation 5-axis 檢查(v4.1 — 對齊 spec §Rule of Alternation Price/Time/Severity/
/// Intricacy/Construction 五軸)。
///
/// 適用條件:5-wave candidate(wave_count == 5,有 W2/W4)。
/// 5 軸計算(各自 Confirmed / NotApplicable / Failed):
/// - **Construction**:從 ValidationReport 抽 `Ch5_Alternation { Construction }` 結果
///   (validator/wave_rules.rs 既有邏輯,Phase 1 唯一 dispatched 軸)
/// - **Price**:W2/W4 retracement % 差異 ≥ 25% → Confirmed
/// - **Time**:W2/W4 duration ratio ≥ 1.5x 或 ≤ 0.67x → Confirmed
/// - **Severity**:一深(≥61.8%)一淺(<38.2%)→ Confirmed
/// - **Intricacy**:W2/W4 的 structure_label_candidates 個數差異 ≥ 2 → Confirmed
///   (簡化代理「子結構複雜度」— V4.x 用 sub-monowave 數量精確化)
///
/// overall_holds:任一軸 Failed → false;任一軸 Confirmed + 其他 NotApplicable → true;
/// 全部 NotApplicable → false(設計上 5-wave 至少 Construction 軸應有結果)。
pub fn alternation(
    candidate: &WaveCandidate,
    classified: &[ClassifiedMonowave],
    report: &ValidationReport,
) -> Option<AlternationFact> {
    if candidate.wave_count != 5 || candidate.monowave_indices.len() < 5 {
        return None;
    }
    let mi = &candidate.monowave_indices;
    let w1 = &classified[mi[0]];
    let w2 = &classified[mi[1]];
    let w3 = &classified[mi[2]];
    let w4 = &classified[mi[3]];

    let construction = classify_construction_axis(report);
    let price = classify_price_axis(w1, w2, w3, w4);
    let time = classify_time_axis(w2, w4);
    let severity = classify_severity_axis(w1, w2, w3, w4);
    let intricacy = classify_intricacy_axis(w2, w4);

    // NEoWave 原則:Alternation 任一軸 Confirmed 即視為 holds(對齊 §Rule of Alternation
    // 「If wave-2 is one kind, wave-4 should be the other kind」— 任一軸差異即 alternation 存在)。
    // 各軸的 Failed / NotApplicable 仍保留供 LLM advisory 用,但聚合用「any_confirmed」即可。
    let axes = [&price, &time, &severity, &intricacy, &construction];
    let overall_holds = axes
        .iter()
        .any(|c| matches!(c, AlternationCheck::Confirmed));

    Some(AlternationFact {
        price,
        time,
        severity,
        intricacy,
        construction,
        overall_holds,
    })
}

fn classify_construction_axis(report: &ValidationReport) -> AlternationCheck {
    let failed = report
        .failed
        .iter()
        .any(|r| matches!(r.rule_id, RuleId::Ch5_Alternation { .. }));
    if failed {
        return AlternationCheck::Failed;
    }
    let passed = report
        .passed
        .iter()
        .any(|r| matches!(r, RuleId::Ch5_Alternation { .. }));
    if passed {
        return AlternationCheck::Confirmed;
    }
    // 在 not_applicable 內或完全不在 report → NotApplicable
    // (rule 未跑 = N/A,避免 empty report 誤判 Confirmed)
    AlternationCheck::NotApplicable
}

/// Price 軸:W2 retracement(W1)% vs W4 retracement(W3)% 差異 ≥ 25%。
fn classify_price_axis(
    w1: &ClassifiedMonowave,
    w2: &ClassifiedMonowave,
    w3: &ClassifiedMonowave,
    w4: &ClassifiedMonowave,
) -> AlternationCheck {
    let w1_mag = w1.metrics.magnitude;
    let w3_mag = w3.metrics.magnitude;
    if w1_mag <= 1e-12 || w3_mag <= 1e-12 {
        return AlternationCheck::NotApplicable;
    }
    let w2_retrace_pct = w2.metrics.magnitude / w1_mag;
    let w4_retrace_pct = w4.metrics.magnitude / w3_mag;
    let diff = (w2_retrace_pct - w4_retrace_pct).abs();
    if diff >= 0.25 {
        AlternationCheck::Confirmed
    } else {
        AlternationCheck::Failed
    }
}

/// Time 軸:W2 duration vs W4 duration ratio ≥ 1.5x 或 ≤ 0.67x。
fn classify_time_axis(w2: &ClassifiedMonowave, w4: &ClassifiedMonowave) -> AlternationCheck {
    let d2 = w2.metrics.duration_bars as f64;
    let d4 = w4.metrics.duration_bars as f64;
    if d2 < 1.0 || d4 < 1.0 {
        return AlternationCheck::NotApplicable;
    }
    let ratio = d2 / d4;
    if ratio >= 1.5 || ratio <= 0.67 {
        AlternationCheck::Confirmed
    } else {
        AlternationCheck::Failed
    }
}

/// Severity 軸:一深(≥61.8%)一淺(<38.2%)retracement → Confirmed。
fn classify_severity_axis(
    w1: &ClassifiedMonowave,
    w2: &ClassifiedMonowave,
    w3: &ClassifiedMonowave,
    w4: &ClassifiedMonowave,
) -> AlternationCheck {
    let w1_mag = w1.metrics.magnitude;
    let w3_mag = w3.metrics.magnitude;
    if w1_mag <= 1e-12 || w3_mag <= 1e-12 {
        return AlternationCheck::NotApplicable;
    }
    let w2_retrace = w2.metrics.magnitude / w1_mag;
    let w4_retrace = w4.metrics.magnitude / w3_mag;
    // 一深 ≥ 0.618 一淺 < 0.382 → Confirmed(對齊 NEoWave 典型 sharp vs flat 對立)
    let one_deep_one_shallow = (w2_retrace >= 0.618 && w4_retrace < 0.382)
        || (w4_retrace >= 0.618 && w2_retrace < 0.382);
    if one_deep_one_shallow {
        AlternationCheck::Confirmed
    } else {
        AlternationCheck::Failed
    }
}

/// Intricacy 軸:W2/W4 的 structure_label_candidates 個數差異 ≥ 2 → Confirmed
///(簡化代理「子結構複雜度」— V4.x 可改用 sub-monowave 數量精確化)。
fn classify_intricacy_axis(w2: &ClassifiedMonowave, w4: &ClassifiedMonowave) -> AlternationCheck {
    let n2 = w2.structure_label_candidates.len();
    let n4 = w4.structure_label_candidates.len();
    if n2 == 0 && n4 == 0 {
        return AlternationCheck::NotApplicable;
    }
    let diff = (n2 as isize - n4 as isize).abs();
    if diff >= 2 {
        AlternationCheck::Confirmed
    } else {
        AlternationCheck::Failed
    }
}

/// 從 monowave duration_bars 計算 wave-time 關係 label + evidence(durations + Fib ratios)。
///
/// 對齊 spec §StructuralFacts.time_relationship。
/// v4.1:label 沿用 + 新增 durations_bars(per-wave)+ fibonacci_ratios_matched
/// (W1/W3/W5 任意 pair 的 duration 比例命中 NEELY_FIB_RATIOS,±10% 容差)。
pub fn time_relationship(
    candidate: &WaveCandidate,
    classified: &[ClassifiedMonowave],
) -> Option<TimeRelationship> {
    let mi = &candidate.monowave_indices;
    if mi.len() < 3 {
        return None;
    }
    let durations: Vec<usize> = mi
        .iter()
        .map(|&i| classified[i].metrics.duration_bars)
        .collect();
    let total: usize = durations.iter().sum();
    if total == 0 {
        return None;
    }

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

    // Fibonacci ratios matched(time axis,±10% 容差;對齊 architecture §4.2 第一檔)
    let time_tol = 0.10;
    let mut fib_matched: Vec<f64> = Vec::new();
    for i in 0..durations.len() - 1 {
        let d_i = durations[i] as f64;
        if d_i < 1.0 {
            continue;
        }
        for j in (i + 1)..durations.len() {
            let d_j = durations[j] as f64;
            if d_j < 1.0 {
                continue;
            }
            let ratio = d_j / d_i;
            for &fib in NEELY_FIB_RATIOS {
                if (ratio - fib).abs() / fib <= time_tol {
                    if !fib_matched.iter().any(|m| (m - fib).abs() < 1e-9) {
                        fib_matched.push(fib);
                    }
                    break;
                }
            }
        }
    }
    fib_matched.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    Some(TimeRelationship {
        label,
        durations_bars: durations,
        fibonacci_ratios_matched: fib_matched,
    })
}

/// 從相鄰 monowave 的 start/end 價區計算 overlap pattern enum 變體 + evidence。
///
/// 對齊 spec §StructuralFacts.overlap_pattern + §Ch5 Overlap Rule 1326-1329 行。
/// v4.1:從 `{ label: String }` 升 enum:
///   - W4 不進 W2 區 → Trending(Trending Impulse 候選)
///   - W4 部分進 W2 區 → Terminal(Terminal Impulse 候選)
///   - 非 5-wave 或資料不足 → None
pub fn overlap_pattern(
    candidate: &WaveCandidate,
    classified: &[ClassifiedMonowave],
) -> Option<OverlapPattern> {
    let mi = &candidate.monowave_indices;
    if mi.len() < 4 {
        return Some(OverlapPattern::None);
    }
    // 對 5-wave Impulse 結構,檢查 W4(idx 3)是否進入 W2(idx 1)區
    let w2 = &classified[mi[1]].monowave;
    let w4 = &classified[mi[3]].monowave;
    let w2_lo = w2.start_price.min(w2.end_price);
    let w2_hi = w2.start_price.max(w2.end_price);
    let w4_lo = w4.start_price.min(w4.end_price);
    let w4_hi = w4.start_price.max(w4.end_price);

    let overlap = w4_lo <= w2_hi && w2_lo <= w4_hi;
    let evidence = format!(
        "W2 [{:.2}, {:.2}] / W4 [{:.2}, {:.2}]",
        w2_lo, w2_hi, w4_lo, w4_hi
    );

    Some(if overlap {
        OverlapPattern::Terminal { evidence }
    } else {
        OverlapPattern::Trending { evidence }
    })
}

/// Extension Subdivision Pair(v4.1 新):對 5-wave Impulse 判定延伸段位置與獨立性。
///
/// 對齊 spec §Ch8 Independent Rule(`neely_rules.md` §Ch8 Complex Polywaves)。
///
/// 判定:
/// - 5-wave 中找 W1/W3/W5 magnitude 最長者為 Extension(extension_ratio = ext_mag /
///   max(non-ext_mag))
/// - status 判定(簡化版,V4.x 加 sub-monowave 統計):
///   - extension_ratio ≥ 1.618 → Independent(典型延伸,結構獨立)
///   - extension_ratio < 1.236 → Indeterminate(三段比例接近,非明顯延伸)
///   - 1.236 ≤ ratio < 1.618 → SubordinateToLarger(weak extension,可能屬更大級 subdivision)
/// - 非 5-wave / 資料不足 → None
pub fn extension_subdivision_pair(
    candidate: &WaveCandidate,
    classified: &[ClassifiedMonowave],
) -> Option<ExtensionSubdivisionPair> {
    if candidate.wave_count != 5 || candidate.monowave_indices.len() < 5 {
        return None;
    }
    let mi = &candidate.monowave_indices;
    let w1_mag = classified[mi[0]].metrics.magnitude;
    let w3_mag = classified[mi[2]].metrics.magnitude;
    let w5_mag = classified[mi[4]].metrics.magnitude;
    if w1_mag <= 1e-12 && w3_mag <= 1e-12 && w5_mag <= 1e-12 {
        return None;
    }

    let (ext_wave, ext_mag, non_ext_max) = if w3_mag >= w1_mag && w3_mag >= w5_mag {
        (WaveNumber::W3, w3_mag, w1_mag.max(w5_mag))
    } else if w1_mag >= w3_mag && w1_mag >= w5_mag {
        (WaveNumber::W1, w1_mag, w3_mag.max(w5_mag))
    } else {
        (WaveNumber::W5, w5_mag, w1_mag.max(w3_mag))
    };

    if non_ext_max <= 1e-12 {
        return None;
    }
    let ratio = ext_mag / non_ext_max;
    let status = if ratio >= 1.618 {
        SubdivisionStatus::Independent
    } else if ratio >= 1.236 {
        SubdivisionStatus::SubordinateToLarger
    } else {
        SubdivisionStatus::Indeterminate
    };

    Some(ExtensionSubdivisionPair {
        extended_wave: ext_wave,
        status,
        extension_ratio: ratio,
    })
}

// ── 3 fns at post-Stage 7.5 time ─────────────────────────────────────────

/// 從 advisory_findings 抽 Channeling 結果 + evidence。
///
/// 對齊 m3Spec/neely_rules.md §Ch5 Channeling(5 條 trendlines:0-2 / 1-3 / 2-4 / 0-B / B-D)。
/// v4.1:回傳含 evidence Vec<String>(各 trendline 偵測 / fail 描述)。
pub fn channeling(advisory_findings: &[AdvisoryFinding]) -> Option<ChannelingFact> {
    let evidence: Vec<String> = advisory_findings
        .iter()
        .filter(|f| matches!(f.severity, AdvisorySeverity::Strong | AdvisorySeverity::Warning))
        .map(|f| f.message.clone())
        .collect();
    let holds = !evidence.is_empty();
    Some(ChannelingFact { holds, evidence })
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

// 抑制 MonowaveDirection unused warning(v4.1 重構後沒 direct 用,留供未來擴充)
#[allow(dead_code)]
fn _unused_direction_anchor(_: MonowaveDirection) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::monowave::ProportionMetrics;
    use crate::output::{Monowave, MonowaveDirection, StructureLabel, StructureLabelCandidate, Certainty};
    use crate::validator::ValidationReport;
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

    fn cmw_with_labels(mag: f64, dur_bars: usize, n_labels: usize) -> ClassifiedMonowave {
        let mut c = cmw(mag, dur_bars);
        c.structure_label_candidates = (0..n_labels)
            .map(|_| StructureLabelCandidate {
                label: StructureLabel::F3,
                certainty: Certainty::Primary,
            })
            .collect();
        c
    }

    fn empty_report() -> ValidationReport {
        ValidationReport {
            candidate_id: "test".to_string(),
            passed: Vec::new(),
            failed: Vec::new(),
            not_applicable: Vec::new(),
            deferred: Vec::new(),
            overall_pass: true,
        }
    }

    #[test]
    fn fibonacci_alignment_matches_618() {
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
        let classified = vec![cmw(10.0, 5), cmw(3.3, 5)];
        let candidate = WaveCandidate {
            id: "c2".to_string(),
            monowave_indices: vec![0, 1],
            wave_count: 3,
            initial_direction: MonowaveDirection::Up,
        };
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
        assert!(tr.label.contains("W3-longest(10 bars)"));
        assert_eq!(tr.durations_bars, vec![5, 3, 10, 4, 6]);
    }

    #[test]
    fn time_relationship_fib_ratios_matched_when_durations_align() {
        // durations [10, 6.18 ish] → ratio 0.618 命中
        let classified = vec![cmw(10.0, 10), cmw(5.0, 6), cmw(15.0, 16)];
        let candidate = WaveCandidate {
            id: "c3".to_string(),
            monowave_indices: vec![0, 1, 2],
            wave_count: 3,
            initial_direction: MonowaveDirection::Up,
        };
        let tr = time_relationship(&candidate, &classified).expect("應產生");
        // 6/10 = 0.6 在 0.618 ±10% 容差內 → 命中 0.618
        assert!(tr.fibonacci_ratios_matched.iter().any(|r| (r - 0.618).abs() < 1e-9));
    }

    #[test]
    fn overlap_pattern_detects_terminal_when_w4_overlaps_w2() {
        // W1: 100 → 110 / W2: 110 → 105 / W3: 105 → 125 / W4: 125 → 108(進 W2 區)
        let mw1 = cmw(10.0, 5);
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
        assert!(matches!(op, OverlapPattern::Terminal { .. }));
    }

    #[test]
    fn overlap_pattern_returns_trending_when_w4_outside_w2() {
        // W1: 100 → 110 / W2: 110 → 108 / W3: 108 → 130 / W4: 130 → 115(未進 W2 [108,110] 區)
        let mw1 = cmw(10.0, 5);
        let mut mw2 = cmw(2.0, 3);
        mw2.monowave.start_price = 110.0;
        mw2.monowave.end_price = 108.0;
        mw2.monowave.direction = MonowaveDirection::Down;
        let mut mw3 = cmw(22.0, 8);
        mw3.monowave.start_price = 108.0;
        mw3.monowave.end_price = 130.0;
        let mut mw4 = cmw(15.0, 6);
        mw4.monowave.start_price = 130.0;
        mw4.monowave.end_price = 115.0;
        mw4.monowave.direction = MonowaveDirection::Down;
        let classified = vec![mw1, mw2, mw3, mw4];
        let candidate = WaveCandidate {
            id: "c4".to_string(),
            monowave_indices: vec![0, 1, 2, 3],
            wave_count: 5,
            initial_direction: MonowaveDirection::Up,
        };
        let op = overlap_pattern(&candidate, &classified).expect("應產生");
        assert!(matches!(op, OverlapPattern::Trending { .. }));
    }

    #[test]
    fn alternation_5_axis_construction_only_path_when_other_axes_fail() {
        // W2/W4 同款 duration + 同款 retrace → 4 軸 Failed,Construction 缺資料 → NotApplicable
        let classified = vec![
            cmw(10.0, 5), // W1
            cmw(5.0, 4),  // W2 retrace 50%
            cmw(15.0, 8), // W3
            cmw(7.5, 4),  // W4 retrace 50% — 同 W2
            cmw(12.0, 6), // W5
        ];
        let candidate = WaveCandidate {
            id: "c5".to_string(),
            monowave_indices: vec![0, 1, 2, 3, 4],
            wave_count: 5,
            initial_direction: MonowaveDirection::Up,
        };
        let report = empty_report();
        let alt = alternation(&candidate, &classified, &report).expect("5-wave 應產生");
        // Price: 50% vs 50% diff = 0 < 25% → Failed
        assert!(matches!(alt.price, AlternationCheck::Failed));
        // Time: 4/4 = 1.0 → Failed
        assert!(matches!(alt.time, AlternationCheck::Failed));
        // overall_holds = false(有 Failed 軸)
        assert!(!alt.overall_holds);
    }

    #[test]
    fn alternation_severity_axis_confirmed_when_one_deep_one_shallow() {
        // W2 retrace 70%(deep,>= 0.618), W4 retrace 30%(shallow,< 0.382)
        let classified = vec![
            cmw(10.0, 5), // W1
            cmw(7.0, 5),  // W2 deep
            cmw(15.0, 5), // W3
            cmw(4.5, 5),  // W4 shallow (4.5 / 15 = 0.30)
            cmw(12.0, 5), // W5
        ];
        let candidate = WaveCandidate {
            id: "c5".to_string(),
            monowave_indices: vec![0, 1, 2, 3, 4],
            wave_count: 5,
            initial_direction: MonowaveDirection::Up,
        };
        let report = empty_report();
        let alt = alternation(&candidate, &classified, &report).expect("應產生");
        assert!(matches!(alt.severity, AlternationCheck::Confirmed));
        // overall_holds = true(severity Confirmed,其他 Failed/NotApplicable 中至少 1 Confirmed)
        assert!(alt.overall_holds);
    }

    #[test]
    fn alternation_intricacy_axis_confirmed_when_labels_differ() {
        // W2 has 1 label, W4 has 4 labels → diff = 3 ≥ 2 → Confirmed
        let classified = vec![
            cmw(10.0, 5),
            cmw_with_labels(5.0, 5, 1),  // W2 sparse
            cmw(15.0, 5),
            cmw_with_labels(7.0, 5, 4),  // W4 intricate
            cmw(12.0, 5),
        ];
        let candidate = WaveCandidate {
            id: "c5".to_string(),
            monowave_indices: vec![0, 1, 2, 3, 4],
            wave_count: 5,
            initial_direction: MonowaveDirection::Up,
        };
        let report = empty_report();
        let alt = alternation(&candidate, &classified, &report).expect("應產生");
        assert!(matches!(alt.intricacy, AlternationCheck::Confirmed));
    }

    #[test]
    fn extension_subdivision_independent_when_w3_strong_extension() {
        // W3 = 30 vs W1 = 10 vs W5 = 12 → ratio 30/12 = 2.5 >= 1.618 → Independent
        let classified = vec![
            cmw(10.0, 5),
            cmw(5.0, 3),
            cmw(30.0, 10),
            cmw(7.0, 4),
            cmw(12.0, 6),
        ];
        let candidate = WaveCandidate {
            id: "c5".to_string(),
            monowave_indices: vec![0, 1, 2, 3, 4],
            wave_count: 5,
            initial_direction: MonowaveDirection::Up,
        };
        let esp = extension_subdivision_pair(&candidate, &classified).expect("應產生");
        assert!(matches!(esp.extended_wave, WaveNumber::W3));
        assert!(matches!(esp.status, SubdivisionStatus::Independent));
        assert!(esp.extension_ratio >= 1.618);
    }

    #[test]
    fn extension_subdivision_indeterminate_when_three_segments_similar() {
        // W1=10, W3=11, W5=10 → ratio 11/10 = 1.1 < 1.236 → Indeterminate
        let classified = vec![
            cmw(10.0, 5),
            cmw(5.0, 3),
            cmw(11.0, 6),
            cmw(7.0, 4),
            cmw(10.0, 6),
        ];
        let candidate = WaveCandidate {
            id: "c5".to_string(),
            monowave_indices: vec![0, 1, 2, 3, 4],
            wave_count: 5,
            initial_direction: MonowaveDirection::Up,
        };
        let esp = extension_subdivision_pair(&candidate, &classified).expect("應產生");
        assert!(matches!(esp.status, SubdivisionStatus::Indeterminate));
    }

    #[test]
    fn extension_subdivision_none_for_3wave() {
        let classified = vec![cmw(10.0, 5), cmw(5.0, 3), cmw(7.0, 4)];
        let candidate = WaveCandidate {
            id: "c3".to_string(),
            monowave_indices: vec![0, 1, 2],
            wave_count: 3,
            initial_direction: MonowaveDirection::Up,
        };
        assert!(extension_subdivision_pair(&candidate, &classified).is_none());
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
                open: 103.0,
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
                open: 107.0,
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
