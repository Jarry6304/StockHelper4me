// patterns/diagonal.rs — 對角三角形 grouper(5 children)。R5 不套(重疊=特徵)。
// HARD:R1/R3/R9;DEFERRED→HARD@deg≥2:R7(Ending 全3,收斂)/ R8(Leading 5-3-5-3-5 或全3,收斂/擴張)。
// 5-leg 三分叉:同 window 可同時出 Impulse(impulse.rs)+ Leading + Ending → 忠實 alternates。

use super::{build_parent, enforce};
use crate::mode::{Mode, PatternKind};
use crate::node::EngineNode;
use crate::output::{DiagonalKind, DiagonalShape, DiagonalSub, TradRuleId};
use crate::rules::{alternates, r1_ok, r3_ok, r9_ok, window_up};

pub fn group(w: &[EngineNode]) -> Vec<EngineNode> {
    if w.len() != 5 || !alternates(w) {
        return Vec::new();
    }
    let up = window_up(w);
    if !r1_ok(w) || !r3_ok(w, up) || !r9_ok(w) {
        return Vec::new();
    }
    let shape = if w[4].amp() < w[0].amp() {
        DiagonalShape::Contracting
    } else {
        DiagonalShape::Expanding
    };

    let mut out = Vec::new();
    let all_three = [Mode::Corrective; 5];
    let five_three = [
        Mode::Motive,
        Mode::Corrective,
        Mode::Motive,
        Mode::Corrective,
        Mode::Motive,
    ];

    // Leading:5-3-5-3-5
    if let Some(d) = enforce(w, &five_three, TradRuleId::R8LeadingDiagonalSub) {
        out.push(diag_node(
            w,
            DiagonalKind::Leading,
            shape,
            DiagonalSub::FiveThreeFiveThreeFive,
            TradRuleId::R8LeadingDiagonalSub,
            d,
        ));
    }
    // Leading:3-3-3-3-3(rev2 較常觀察者)
    if let Some(d) = enforce(w, &all_three, TradRuleId::R8LeadingDiagonalSub) {
        out.push(diag_node(
            w,
            DiagonalKind::Leading,
            shape,
            DiagonalSub::AllThrees,
            TradRuleId::R8LeadingDiagonalSub,
            d,
        ));
    }
    // Ending:全3-3-3-3-3,收斂 only(擴張 ending 不 emit)
    if matches!(shape, DiagonalShape::Contracting) {
        if let Some(d) = enforce(w, &all_three, TradRuleId::R7EndingDiagonalSub) {
            out.push(diag_node(
                w,
                DiagonalKind::Ending,
                DiagonalShape::Contracting,
                DiagonalSub::AllThrees,
                TradRuleId::R7EndingDiagonalSub,
                d,
            ));
        }
    }
    out
}

fn diag_node(
    w: &[EngineNode],
    kind: DiagonalKind,
    shape: DiagonalShape,
    sub: DiagonalSub,
    rule: TradRuleId,
    mut deferred: Vec<TradRuleId>,
) -> EngineNode {
    let mut passed = vec![
        TradRuleId::R1Wave2Retracement,
        TradRuleId::R3Wave3ExceedsWave1,
        TradRuleId::R9DiagonalNoFullRetrace,
    ];
    if deferred.is_empty() {
        passed.push(rule);
    }
    deferred.push(TradRuleId::R2Wave4Retracement);
    deferred.dedup();
    let pk = match kind {
        DiagonalKind::Leading => PatternKind::LeadingDiagonal,
        DiagonalKind::Ending => PatternKind::EndingDiagonal,
    };
    build_parent(pk, w.to_vec(), passed, deferred, Some((kind, shape, sub)), None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::Direction;
    use chrono::NaiveDate;

    fn mw(sp: f64, ep: f64, i: usize) -> EngineNode {
        let dir = if ep >= sp { Direction::Up } else { Direction::Down };
        EngineNode {
            kind: PatternKind::Monowave,
            mode: Mode::Unknown,
            direction: dir,
            degree_level: 0,
            start_bar: i,
            end_bar: i + 1,
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

    // 浪4 重疊浪1(13 < 14):Impulse 被 R5 淘汰、**同幾何 Diagonal 合法**(R8 rev2 擴張引導對角)
    #[test]
    fn overlap_kills_impulse_but_diagonal_legal() {
        let prices = [
            (10.0, 14.0),
            (14.0, 12.5),
            (12.5, 18.0),
            (18.0, 13.0),
            (13.0, 20.0),
        ];
        let w: Vec<EngineNode> = prices
            .iter()
            .enumerate()
            .map(|(i, (s, e))| mw(*s, *e, i))
            .collect();
        assert!(
            crate::patterns::impulse::group(&w).is_empty(),
            "浪4 重疊浪1 → Impulse 被 R5 淘汰"
        );
        assert!(!group(&w).is_empty(), "同幾何 → Diagonal 合法(R5 不套、R9 通過)");
    }
}
