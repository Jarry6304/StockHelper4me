// patterns/triangle.rs — 三角形 grouper(5 children a-b-c-d-e,全 corrective)。
// HARD:R10(arity)+ 幅度單調趨勢生成閘(收斂或擴張,避免任意 5-window 都成三角);
// DEFERRED→HARD@deg≥2:R11(3-3-3-3-3)。shape/running = guideline。R13 = parent-time qualifier(scenario 處理)。

use super::{build_parent, enforce};
use crate::mode::{Mode, PatternKind};
use crate::node::EngineNode;
use crate::output::TradRuleId;
use crate::rules::alternates;

pub fn group(w: &[EngineNode]) -> Vec<EngineNode> {
    if w.len() != 5 || !alternates(w) {
        return Vec::new();
    }
    // 生成閘:行動腿幅度單調(收斂 a>c>e 或 擴張 a<c<e)。running triangle(b 超 a 起點)不淘汰。
    let contracting = w[0].amp() > w[2].amp() && w[2].amp() > w[4].amp();
    let expanding = w[0].amp() < w[2].amp() && w[2].amp() < w[4].amp();
    if !contracting && !expanding {
        return Vec::new();
    }
    let slot = [Mode::Corrective; 5];
    let deferred = match enforce(w, &slot, TradRuleId::R11CorrectiveSubdivision) {
        Some(d) => d,
        None => return Vec::new(),
    };
    let mut passed = vec![TradRuleId::R10CorrectionNeverFive];
    if deferred.is_empty() {
        passed.push(TradRuleId::R11CorrectiveSubdivision);
    }
    let variant = if contracting { "contracting" } else { "expanding" }.to_string();
    vec![build_parent(
        PatternKind::Triangle,
        w.to_vec(),
        passed,
        deferred,
        None,
        Some(variant),
    )]
}
