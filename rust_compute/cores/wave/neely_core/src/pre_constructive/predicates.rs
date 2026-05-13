// pre_constructive/predicates.rs — Ch3 Pre-Constructive Logic 共用 predicate
//
// 對齊 m3Spec/neely_rules.md §Pre-Constructive Logic 細部技術備註(1040-1062 行)
//       + m3Spec/neely_core_architecture.md §4.2 三檔容差表
//
// **容差規範**(architecture §4.2):
//   - 一般近似(approximately equal / about / close to):±10%
//   - Fibonacci 比率(38.2% / 61.8% / 100% / 161.8% / 261.8%):±4%
//   - Triangle 三條同度數腿價格相等性:±5%(本檔不用 — 三角規則層用)

use crate::monowave::ClassifiedMonowave;
use crate::output::{Monowave, MonowaveDirection, StructureLabel, StructureLabelCandidate};

// ---------------------------------------------------------------------------
// 容差常數
// ---------------------------------------------------------------------------

/// 一般近似(approximately equal)= ±10%
pub const APPROX_TOL: f64 = 0.10;
/// Fibonacci 比率 = ±4%
pub const FIB_TOL: f64 = 0.04;

// 常用 Fib 比率
pub const FIB_382: f64 = 0.382;
pub const FIB_618: f64 = 0.618;
pub const FIB_1000: f64 = 1.0;
pub const FIB_1382: f64 = 1.382;
pub const FIB_1618: f64 = 1.618;
pub const FIB_2618: f64 = 2.618;

// ---------------------------------------------------------------------------
// Magnitude / Ratio helpers
// ---------------------------------------------------------------------------

#[inline]
pub fn magnitude(mw: &ClassifiedMonowave) -> f64 {
    mw.metrics.magnitude
}

#[inline]
pub fn duration(mw: &ClassifiedMonowave) -> usize {
    mw.metrics.duration_bars
}

/// `a` 與 `b` magnitude 比(a/b)。
#[inline]
pub fn mag_ratio(a: &ClassifiedMonowave, b: &ClassifiedMonowave) -> f64 {
    let m_b = magnitude(b);
    if m_b > 1e-12 { magnitude(a) / m_b } else { 0.0 }
}

/// `a` 與 `b` duration 比(a/b)。
#[inline]
pub fn dur_ratio(a: &ClassifiedMonowave, b: &ClassifiedMonowave) -> f64 {
    let d_b = duration(b);
    if d_b > 0 { duration(a) as f64 / d_b as f64 } else { 0.0 }
}

/// 「approximately equal」(±10%)— spec 1050 行
#[inline]
pub fn approx_equal(a: f64, b: f64) -> bool {
    let avg = (a.abs() + b.abs()) / 2.0;
    if avg < 1e-12 {
        (a - b).abs() < 1e-12
    } else {
        (a - b).abs() / avg <= APPROX_TOL
    }
}

/// Fibonacci 容差(±4%)— spec 1051 行
#[inline]
pub fn fib_approx(value: f64, target: f64) -> bool {
    if target.abs() < 1e-12 {
        return value.abs() < 1e-12;
    }
    (value / target - 1.0).abs() <= FIB_TOL
}

// ---------------------------------------------------------------------------
// Direction-aware 價格與 retracement
// ---------------------------------------------------------------------------

/// `a` 與 `b` 是否共享部分價區(neely_rules.md 524 行 share_price_range)。
///
/// 價區 = [min(start, end), max(start, end)]。
pub fn share_price_range(a: &Monowave, b: &Monowave) -> bool {
    let a_low = a.start_price.min(a.end_price);
    let a_high = a.start_price.max(a.end_price);
    let b_low = b.start_price.min(b.end_price);
    let b_high = b.start_price.max(b.end_price);
    a_low <= b_high && b_low <= a_high
}

/// `retracer` 對 `target` 的回測百分比(0..=1+,可超過 1 表完全回測並更深)。
///
/// retracer 從 target.end 起點,target 的方向逆向移動的程度;
/// 比 = (overshoot amount) / target.magnitude
pub fn retracement_pct(target: &Monowave, retracer: &Monowave) -> f64 {
    let mag = (target.end_price - target.start_price).abs();
    if mag < 1e-12 {
        return 0.0;
    }
    let retraced = match target.direction {
        MonowaveDirection::Up => (target.end_price - retracer.end_price).max(0.0),
        MonowaveDirection::Down => (retracer.end_price - target.end_price).max(0.0),
        MonowaveDirection::Neutral => 0.0,
    };
    retraced / mag
}

/// `retracer` 是否完全回測 `target`(回測量 ≥ target magnitude)。
#[inline]
pub fn completely_retraced(target: &Monowave, retracer: &Monowave) -> bool {
    retracement_pct(target, retracer) >= 1.0
}

/// `retracer` 是否在「target 同時間或更短」內完全回測 target。
///
/// 對齊 spec 「m1 在 m0 同時間(或更短)完全回測 m0」style 描述。
pub fn completely_retraced_within_time(
    target: &ClassifiedMonowave,
    retracer: &ClassifiedMonowave,
) -> bool {
    completely_retraced(&target.monowave, &retracer.monowave)
        && duration(retracer) <= duration(target)
}

/// Plus-one-time-unit 規則(spec 1042-1045 行):
///   「target(plus one time unit)在 target 同時間或更短被 retracer 完全回測」
/// = duration(retracer) ≤ duration(target) + 1 且 completely retraced
pub fn completely_retraced_plus_one_time_unit(
    target: &ClassifiedMonowave,
    retracer: &ClassifiedMonowave,
) -> bool {
    completely_retraced(&target.monowave, &retracer.monowave)
        && duration(retracer) <= duration(target) + 1
}

// ---------------------------------------------------------------------------
// 「More vertical」(斜率比較)
// ---------------------------------------------------------------------------

/// 「a 比 b 更長更垂直」= a magnitude > b AND a slope(price/time) > b slope。
///
/// 對齊 spec 「m3 比 m1 更長更垂直」style 描述。
pub fn more_vertical_and_longer(a: &ClassifiedMonowave, b: &ClassifiedMonowave) -> bool {
    if magnitude(a) <= magnitude(b) {
        return false;
    }
    let d_a = duration(a);
    let d_b = duration(b);
    if d_a == 0 || d_b == 0 {
        return false;
    }
    let slope_a = magnitude(a) / d_a as f64;
    let slope_b = magnitude(b) / d_b as f64;
    slope_a > slope_b
}

// ---------------------------------------------------------------------------
// 「m1 為 m(-1)/m1/m3 中最長 / 最短」(neely_rules.md 共用判斷)
// ---------------------------------------------------------------------------

/// 在 (a, b, c) 中,a 是否為最長(magnitude)。tie-break 視為非最長。
pub fn is_longest_of_three(
    a: &ClassifiedMonowave,
    b: Option<&ClassifiedMonowave>,
    c: Option<&ClassifiedMonowave>,
) -> bool {
    let mag_a = magnitude(a);
    let mag_b = b.map(magnitude).unwrap_or(0.0);
    let mag_c = c.map(magnitude).unwrap_or(0.0);
    mag_a > mag_b && mag_a > mag_c
}

/// 在 (a, b, c) 中,a 是否為最短(magnitude)。
pub fn is_shortest_of_three(
    a: &ClassifiedMonowave,
    b: Option<&ClassifiedMonowave>,
    c: Option<&ClassifiedMonowave>,
) -> bool {
    let mag_a = magnitude(a);
    let mag_b = b.map(magnitude).unwrap_or(f64::INFINITY);
    let mag_c = c.map(magnitude).unwrap_or(f64::INFINITY);
    mag_a < mag_b && mag_a < mag_c
}

// ---------------------------------------------------------------------------
// 2-4 Breach Line(早期突破檢測,neely_rules.md 526 行)
// ---------------------------------------------------------------------------

/// `m2` 是否在 `m1` 時間或更短內突破 m(-2)/m0 連線。
///
/// 對齊 spec 「m1 為 m(-1)/m1/m(-3) 中最長 AND m2 在 ≤ m1 時間內突破 m(-2)/m0 連線」。
///
/// 幾何:
///   1. 線從 (m(-2).end_date, m(-2).end_price) 到 (m0.end_date, m0.end_price)
///   2. 線性外推到 m2.end_date 對應的 y 值
///   3. 上漲 impulse:line 應該上升;m2 「突破」即 m2.end_price 在 line 之下(穿破支撐)
///   4. 下跌 impulse:對稱
pub fn m2_breaches_2_4_line_within_m1_time(
    m_minus_2: &ClassifiedMonowave,
    m0: &ClassifiedMonowave,
    m1: &ClassifiedMonowave,
    m2: &ClassifiedMonowave,
) -> bool {
    // 條件 1:m2 耗時 ≤ m1 耗時
    if duration(m2) > duration(m1) {
        return false;
    }

    // 條件 2:線性外推 y(at m2.end_date)
    let t1 = m_minus_2.monowave.end_date;
    let t2 = m0.monowave.end_date;
    let y1 = m_minus_2.monowave.end_price;
    let y2 = m0.monowave.end_price;

    let dt = (t2 - t1).num_days() as f64;
    if dt.abs() < 1e-12 {
        return false; // 時間退化,無法外推
    }
    let dt_m2 = (m2.monowave.end_date - t1).num_days() as f64;
    let line_y_at_m2_end = y1 + (y2 - y1) * (dt_m2 / dt);

    // 條件 3:依 m1 方向判斷 breach
    let m2_end = m2.monowave.end_price;
    match m1.monowave.direction {
        MonowaveDirection::Up => m2_end < line_y_at_m2_end,
        MonowaveDirection::Down => m2_end > line_y_at_m2_end,
        MonowaveDirection::Neutral => false,
    }
}

/// m1 端點是否被 m2 突破(spec 4b/4c/4d/4e 共用 predicate)。
///
/// m1 端點 = m1.end_price。
/// 「突破」= m2 在 m1 方向延伸後,end_price 進一步超越 m1.end_price。
/// 但 spec 用法是「m1 端點被 m2 突破」當 m1 為 5th Ext 的 5th wave 場景,
/// 即 m2 進一步突破 m1 終點(同 m1 方向繼續),這是不尋常的;
/// 解讀為:m2 與 m1 同方向,且 m2 終點超過 m1 終點。
///
/// 但 m2 必然是 m1 的逆向(否則不會是新 monowave)。所以實際語意應為:
/// 在 m2 的 retracement 過程中,某時刻價格觸到或超過 m1 終點之外。
///
/// 簡化詮釋(本 PR best-guess):**m2 的「extreme」價超過 m1 終點**(以 m1 方向延伸看)
/// 由於我們的 Monowave 只記 start/end 而非中間 extreme,以 end_price 近似判斷。
/// 完整語意需 Phase 3 完整 OHLC reference 串接(留 P5/P10 改善)。
///
/// 本 PR best-guess:m2.end 在 m1 同方向上是否超過 m1.end 的對角(即 m2 比 m1 走更遠)
/// = m2 end_price 表現得像繼續 m1 方向走。實際 m2 是逆向 monowave,
/// 因此這個 predicate 在 spec 中通常為 false(很罕見場景)。
/// 留 false 為 default,後續 OHLC reference 串接時補完。
pub fn m1_endpoint_broken_by_m2(_m1: &ClassifiedMonowave, _m2: &ClassifiedMonowave) -> bool {
    // [缺資料欄位]:需要 m2 期間的 intraday extreme(non-trivial,需 OHLC bars 對應日期區間)
    // 公式:m1.direction 是 Up 時,m2 期間任一 bar.high > m1.end_price → true
    //       m1.direction 是 Down 時,m2 期間任一 bar.low < m1.end_price → true
    // 本 PR 留 false placeholder;P5 加 OHLC reference 後實作。
    false
}

// ---------------------------------------------------------------------------
// Structure Label 操作 helpers
// ---------------------------------------------------------------------------

/// 將 (label, certainty) 加入 candidates;若 label 已存在,升級 certainty(取較強者)。
///
/// Certainty 強度:Primary > Possible > Rare > MissingWaveBundle。
pub fn add_or_promote(
    candidates: &mut Vec<StructureLabelCandidate>,
    label: StructureLabel,
    certainty: super::super::output::Certainty,
) {
    use super::super::output::Certainty;
    let strength = |c: Certainty| match c {
        Certainty::Primary => 3,
        Certainty::Possible => 2,
        Certainty::Rare => 1,
        Certainty::MissingWaveBundle => 0,
    };
    if let Some(existing) = candidates.iter_mut().find(|c| c.label == label) {
        if strength(certainty) > strength(existing.certainty) {
            existing.certainty = certainty;
        }
    } else {
        candidates.push(StructureLabelCandidate { label, certainty });
    }
}

/// 從 candidates 移除 label(無論 certainty)。
pub fn drop_label(candidates: &mut Vec<StructureLabelCandidate>, label: StructureLabel) {
    candidates.retain(|c| c.label != label);
}

/// 將 candidate certainty 強制改為 new_certainty(label 存在才作用)。
pub fn change_certainty(
    candidates: &mut [StructureLabelCandidate],
    label: StructureLabel,
    new_certainty: super::super::output::Certainty,
) {
    if let Some(c) = candidates.iter_mut().find(|c| c.label == label) {
        c.certainty = new_certainty;
    }
}

/// 「:c3 前加 x」= 把 C3 移除並加 XC3(spec 共用慣例,Rule 4 多處)。
///
/// 若 C3 不在 list 則 no-op(不主動加 XC3)。
pub fn prefix_c3_with_x(candidates: &mut Vec<StructureLabelCandidate>) {
    let mut found_cert = None;
    candidates.retain(|c| {
        if c.label == StructureLabel::C3 {
            found_cert = Some(c.certainty);
            false
        } else {
            true
        }
    });
    if let Some(cert) = found_cert {
        candidates.push(StructureLabelCandidate {
            label: StructureLabel::XC3,
            certainty: cert,
        });
    }
}

/// 查詢 m_prev 的 structure_label_candidates 是否包含 label。
///
/// 對齊 spec 「m0 Structure 包含 :F3」style query。
pub fn structure_includes(prev: &ClassifiedMonowave, label: StructureLabel) -> bool {
    prev.structure_label_candidates
        .iter()
        .any(|c| c.label == label)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::monowave::ProportionMetrics;
    use crate::output::Certainty;
    use chrono::NaiveDate;

    fn cmw(start_p: f64, end_p: f64, dir: MonowaveDirection, dur: usize) -> ClassifiedMonowave {
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
                duration_bars: dur,
                atr_relative: 1.0,
                slope_vs_45deg: 1.0,
            },
            structure_label_candidates: Vec::new(),
        }
    }

    #[test]
    fn approx_equal_within_10pct() {
        assert!(approx_equal(100.0, 105.0));
        assert!(approx_equal(100.0, 95.0));
        assert!(!approx_equal(100.0, 120.0));
    }

    #[test]
    fn fib_approx_4pct_tolerance() {
        assert!(fib_approx(0.62, 0.618));
        assert!(fib_approx(0.60, 0.618));
        assert!(!fib_approx(0.5, 0.618));
    }

    #[test]
    fn share_price_range_overlapping() {
        let a = cmw(100.0, 110.0, MonowaveDirection::Up, 5).monowave;
        let b = cmw(105.0, 115.0, MonowaveDirection::Up, 5).monowave;
        assert!(share_price_range(&a, &b));
    }

    #[test]
    fn share_price_range_disjoint() {
        let a = cmw(100.0, 110.0, MonowaveDirection::Up, 5).monowave;
        let b = cmw(120.0, 130.0, MonowaveDirection::Up, 5).monowave;
        assert!(!share_price_range(&a, &b));
    }

    #[test]
    fn retracement_pct_up_target() {
        // target 100→110, retracer 110→105 → retraced 5 / 10 = 50%
        let target = cmw(100.0, 110.0, MonowaveDirection::Up, 5).monowave;
        let retracer = cmw(110.0, 105.0, MonowaveDirection::Down, 3).monowave;
        let pct = retracement_pct(&target, &retracer);
        assert!((pct - 0.5).abs() < 1e-9);
    }

    #[test]
    fn completely_retraced_when_retracer_reaches_target_start() {
        let target = cmw(100.0, 110.0, MonowaveDirection::Up, 5).monowave;
        let retracer = cmw(110.0, 100.0, MonowaveDirection::Down, 3).monowave;
        assert!(completely_retraced(&target, &retracer));
    }

    #[test]
    fn add_or_promote_upgrades_certainty() {
        let mut cands = vec![StructureLabelCandidate {
            label: StructureLabel::Five,
            certainty: Certainty::Possible,
        }];
        add_or_promote(&mut cands, StructureLabel::Five, Certainty::Primary);
        assert_eq!(cands.len(), 1);
        assert_eq!(cands[0].certainty, Certainty::Primary);
    }

    #[test]
    fn prefix_c3_with_x_replaces_c3() {
        let mut cands = vec![
            StructureLabelCandidate {
                label: StructureLabel::C3,
                certainty: Certainty::Primary,
            },
            StructureLabelCandidate {
                label: StructureLabel::Five,
                certainty: Certainty::Possible,
            },
        ];
        prefix_c3_with_x(&mut cands);
        assert!(cands.iter().any(|c| c.label == StructureLabel::XC3));
        assert!(!cands.iter().any(|c| c.label == StructureLabel::C3));
    }

    #[test]
    fn m2_breaches_2_4_line_up_direction() {
        // m(-2): end at day=4 price=90 ; m0: end at day=8 price=100 ; line slopes up
        // m1: 100→120 lasts day 8-12 (dur=4)
        // m2: 120→105 lasts day 12-14 (dur=2, ≤ m1 dur=4)
        // line at day=14: 90 + (100-90)*(14-4)/(8-4) = 90 + 25 = 115
        // m2 end 105 < 115 → breach
        let m_neg_2 = ClassifiedMonowave {
            monowave: Monowave {
                start_date: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                end_date: NaiveDate::from_ymd_opt(2026, 1, 5).unwrap(), // day 4 since 1/1
                start_price: 85.0,
                end_price: 90.0,
                direction: MonowaveDirection::Up,
            },
            atr_at_start: 1.0,
            metrics: ProportionMetrics {
                magnitude: 5.0,
                duration_bars: 4,
                atr_relative: 1.0,
                slope_vs_45deg: 1.0,
            },
            structure_label_candidates: Vec::new(),
        };
        let m_0 = ClassifiedMonowave {
            monowave: Monowave {
                start_date: NaiveDate::from_ymd_opt(2026, 1, 6).unwrap(),
                end_date: NaiveDate::from_ymd_opt(2026, 1, 9).unwrap(), // day 8
                start_price: 95.0,
                end_price: 100.0,
                direction: MonowaveDirection::Up,
            },
            atr_at_start: 1.0,
            metrics: ProportionMetrics {
                magnitude: 5.0,
                duration_bars: 4,
                atr_relative: 1.0,
                slope_vs_45deg: 1.0,
            },
            structure_label_candidates: Vec::new(),
        };
        let m_1 = ClassifiedMonowave {
            monowave: Monowave {
                start_date: NaiveDate::from_ymd_opt(2026, 1, 9).unwrap(),
                end_date: NaiveDate::from_ymd_opt(2026, 1, 13).unwrap(), // day 12
                start_price: 100.0,
                end_price: 120.0,
                direction: MonowaveDirection::Up,
            },
            atr_at_start: 1.0,
            metrics: ProportionMetrics {
                magnitude: 20.0,
                duration_bars: 4,
                atr_relative: 1.0,
                slope_vs_45deg: 1.0,
            },
            structure_label_candidates: Vec::new(),
        };
        let m_2 = ClassifiedMonowave {
            monowave: Monowave {
                start_date: NaiveDate::from_ymd_opt(2026, 1, 13).unwrap(),
                end_date: NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(), // day 14
                start_price: 120.0,
                end_price: 105.0,
                direction: MonowaveDirection::Down,
            },
            atr_at_start: 1.0,
            metrics: ProportionMetrics {
                magnitude: 15.0,
                duration_bars: 2,
                atr_relative: 1.0,
                slope_vs_45deg: 1.0,
            },
            structure_label_candidates: Vec::new(),
        };
        assert!(m2_breaches_2_4_line_within_m1_time(&m_neg_2, &m_0, &m_1, &m_2));
    }
}
