// patterns/zigzag.rs — 鋸齒 grouper(3 children A-B-C)。
// HARD:R10(arity)+ B 不完全回撤 A(幾何);DEFERRED→HARD@deg≥2:R11(5-3-5)。

use super::{build_parent, enforce};
use crate::mode::{Mode, PatternKind};
use crate::node::EngineNode;
use crate::output::TradRuleId;
use crate::rules::alternates;

pub fn group(w: &[EngineNode]) -> Vec<EngineNode> {
    if w.len() != 3 || !alternates(w) {
        return Vec::new();
    }
    // B 不完全回撤 A(B amp ≥ A amp → 非鋸齒,留給 flat/其他)
    if w[1].amp() >= w[0].amp() {
        return Vec::new();
    }
    // 5-3-5
    let slot = [Mode::Motive, Mode::Corrective, Mode::Motive];
    let deferred = match enforce(w, &slot, TradRuleId::R11CorrectiveSubdivision) {
        Some(d) => d,
        None => return Vec::new(),
    };
    let mut passed = vec![TradRuleId::R10CorrectionNeverFive];
    if deferred.is_empty() {
        passed.push(TradRuleId::R11CorrectiveSubdivision);
    }
    vec![build_parent(PatternKind::Zigzag, w.to_vec(), passed, deferred, None, None)]
}
