// patterns/flat.rs — 平台 grouper(3 children A-B-C)。
// HARD:R10(arity);DEFERRED→HARD@deg≥2:R11(3-3-5 — A 須是 3,running-flat trap:B 是 5 → 非 flat)。
// variant(regular/expanded/running)= 顯示分類,不淘汰。

use super::{build_parent, enforce};
use crate::mode::{Mode, PatternKind};
use crate::node::EngineNode;
use crate::output::{Direction, TradRuleId};
use crate::rules::alternates;

pub fn group(w: &[EngineNode]) -> Vec<EngineNode> {
    if w.len() != 3 || !alternates(w) {
        return Vec::new();
    }
    // 3-3-5(slot[1]=Corrective 即 running-flat trap:degree≥2 B 若是 Motive → 淘汰)
    let slot = [Mode::Corrective, Mode::Corrective, Mode::Motive];
    let deferred = match enforce(w, &slot, TradRuleId::R11CorrectiveSubdivision) {
        Some(d) => d,
        None => return Vec::new(),
    };
    // 位置約束:C(motive slot)不可是 Leading Diagonal(C 處只可 Ending)
    if w[2].kind == PatternKind::LeadingDiagonal {
        return Vec::new();
    }
    let mut passed = vec![TradRuleId::R10CorrectionNeverFive];
    if deferred.is_empty() {
        passed.push(TradRuleId::R11CorrectiveSubdivision);
    }
    let variant = flat_variant(w);
    vec![build_parent(
        PatternKind::Flat,
        w.to_vec(),
        passed,
        deferred,
        None,
        Some(variant),
    )]
}

fn flat_variant(w: &[EngineNode]) -> String {
    let b_exceeds_a_start = w[1].amp() > w[0].amp(); // B 超越 A 起點
    let a_up = matches!(w[0].direction, Direction::Up);
    let c_past_a_end = if a_up {
        w[2].end_price > w[0].end_price
    } else {
        w[2].end_price < w[0].end_price
    };
    match (b_exceeds_a_start, c_past_a_end) {
        (true, false) => "running".to_string(),  // B 過 A 起點、C 未達 A 終點(罕,十有九錯)
        (true, true) => "expanded".to_string(),   // 最常見
        _ => "regular".to_string(),
    }
}
