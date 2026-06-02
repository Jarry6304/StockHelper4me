// patterns/impulse.rs — 衝擊浪 grouper(5 children)。
// HARD:R1/R3/R4/R5;DEFERRED→HARD@deg≥2:R6(5-3-5-3-5 + 浪3 須 Impulse);R2 永 deferred。

use super::{build_parent, enforce};
use crate::mode::{Mode, PatternKind};
use crate::node::EngineNode;
use crate::output::TradRuleId;
use crate::rules::{alternates, r1_ok, r3_ok, r4_ok, r5_no_overlap, window_up};

pub fn group(w: &[EngineNode]) -> Vec<EngineNode> {
    if w.len() != 5 || !alternates(w) {
        return Vec::new();
    }
    let up = window_up(w);
    // 幾何硬規則(浪4 重疊浪1 → 此處淘汰,但同幾何可能成立為對角)
    if !r1_ok(w) || !r3_ok(w, up) || !r4_ok(w) || !r5_no_overlap(w, up) {
        return Vec::new();
    }
    // 子浪 5-3-5-3-5
    let slot = [
        Mode::Motive,
        Mode::Corrective,
        Mode::Motive,
        Mode::Corrective,
        Mode::Motive,
    ];
    let mut deferred = match enforce(w, &slot, TradRuleId::R6ImpulseSubdivision) {
        Some(d) => d,
        None => return Vec::new(),
    };
    // 浪3 須是 Impulse(degree≥1 才可查;degree-0 已被 R6 deferred 涵蓋)
    if w[2].mode != Mode::Unknown && w[2].kind != PatternKind::Impulse {
        return Vec::new();
    }
    let mut passed = vec![
        TradRuleId::R1Wave2Retracement,
        TradRuleId::R3Wave3ExceedsWave1,
        TradRuleId::R4Wave3NotShortest,
        TradRuleId::R5NoOverlap,
    ];
    if deferred.is_empty() {
        passed.push(TradRuleId::R6ImpulseSubdivision);
    }
    deferred.push(TradRuleId::R2Wave4Retracement); // [待查證] 永 deferred
    deferred.dedup();
    vec![build_parent(PatternKind::Impulse, w.to_vec(), passed, deferred, None, None)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::Direction;
    use chrono::NaiveDate;

    // 乾淨上行衝擊浪幾何:10→15→12→22→18→28
    fn child(kind: PatternKind, mode: Mode, deg: usize, sp: f64, ep: f64, sb: usize, eb: usize) -> EngineNode {
        let dir = if ep >= sp { Direction::Up } else { Direction::Down };
        EngineNode {
            kind,
            mode,
            direction: dir,
            degree_level: deg,
            start_bar: sb,
            end_bar: eb,
            start_date: NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
            end_date: NaiveDate::from_ymd_opt(2024, 1, 2).unwrap(),
            start_price: sp,
            end_price: ep,
            diag: None,
            variant: None,
            children: Vec::new(),
            passed_rules: Vec::new(),
            deferred_rules: Vec::new(),
        }
    }

    fn impulse_geometry(kinds_modes: [(PatternKind, Mode); 5], deg: usize) -> Vec<EngineNode> {
        let prices = [(10.0, 15.0), (15.0, 12.0), (12.0, 22.0), (22.0, 18.0), (18.0, 28.0)];
        kinds_modes
            .iter()
            .zip(prices.iter())
            .enumerate()
            .map(|(i, ((k, m), (s, e)))| child(*k, *m, deg, *s, *e, i, i + 1))
            .collect()
    }

    // degree-0 monowave children(mode Unknown)→ R6 進 deferred(線內不可見,忠於原書)
    #[test]
    fn deg1_from_monowaves_defers_r6() {
        let w = impulse_geometry(
            [(PatternKind::Monowave, Mode::Unknown); 5],
            0,
        );
        let out = group(&w);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].degree_level, 1);
        assert!(out[0].deferred_rules.contains(&TradRuleId::R6ImpulseSubdivision));
        assert!(!out[0].passed_rules.contains(&TradRuleId::R6ImpulseSubdivision));
    }

    // degree-1 children 模式 5-3-5-3-5 正確 → R6 HARD passed(headline:子浪細分真執行)
    #[test]
    fn deg2_correct_modes_pass_r6_hard() {
        let w = impulse_geometry(
            [
                (PatternKind::Impulse, Mode::Motive),
                (PatternKind::Zigzag, Mode::Corrective),
                (PatternKind::Impulse, Mode::Motive),
                (PatternKind::Flat, Mode::Corrective),
                (PatternKind::Impulse, Mode::Motive),
            ],
            1,
        );
        let out = group(&w);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].degree_level, 2);
        assert!(out[0].passed_rules.contains(&TradRuleId::R6ImpulseSubdivision));
        assert!(!out[0].deferred_rules.contains(&TradRuleId::R6ImpulseSubdivision));
    }

    // degree-1 child 模式錯(slot-1 應 Corrective 卻 Motive)→ 硬淘汰
    #[test]
    fn deg2_wrong_child_mode_hard_rejected() {
        let w = impulse_geometry(
            [
                (PatternKind::Impulse, Mode::Motive),
                (PatternKind::Impulse, Mode::Motive), // ✗ 浪2 應 Corrective
                (PatternKind::Impulse, Mode::Motive),
                (PatternKind::Flat, Mode::Corrective),
                (PatternKind::Impulse, Mode::Motive),
            ],
            1,
        );
        assert!(group(&w).is_empty(), "錯誤子浪模式須硬淘汰");
    }

    // 浪3 不是 Impulse(是 Diagonal)→ 硬淘汰(R6:浪3 須是衝擊浪)
    #[test]
    fn deg2_wave3_not_impulse_hard_rejected() {
        let w = impulse_geometry(
            [
                (PatternKind::Impulse, Mode::Motive),
                (PatternKind::Zigzag, Mode::Corrective),
                (PatternKind::LeadingDiagonal, Mode::Motive), // ✗ 浪3 是對角
                (PatternKind::Flat, Mode::Corrective),
                (PatternKind::Impulse, Mode::Motive),
            ],
            1,
        );
        assert!(group(&w).is_empty(), "浪3 須是 Impulse");
    }
}
