// patterns/mod.rs — v3 per-pattern groupers + 共用 build/enforce helper。
//
// 每 grouper:`group(window: &[EngineNode]) -> Vec<EngineNode>`,對一個「連續同層子浪視窗」
// 試組成下一度數的 parent;子浪細分(R6/R7/R8/R11)以 children 的 mode 強制
// (degree-0 child 無 mode → defer;degree≥1 child mode 不符 → 硬淘汰)。

pub mod diagonal;
pub mod flat;
pub mod impulse;
pub mod triangle;
pub mod zigzag;

use crate::mode::{Mode, PatternKind};
use crate::node::EngineNode;
use crate::output::{DiagonalKind, DiagonalShape, DiagonalSub, Direction, TradRuleId};

/// 對一個 window 試所有適用 grouper,回所有合法 parent 候選。
pub fn try_all(w: &[EngineNode]) -> Vec<EngineNode> {
    let mut out = Vec::new();
    match w.len() {
        3 => {
            out.extend(zigzag::group(w));
            out.extend(flat::group(w));
        }
        5 => {
            out.extend(impulse::group(w));
            out.extend(diagonal::group(w));
            out.extend(triangle::group(w));
        }
        _ => {}
    }
    out
}

/// 依每 slot 必需 mode 強制子浪細分。
/// - `Some(deferred)`:合法。deferred 含 rule(若有 degree-0 child 無法檢查)。
/// - `None`:degree≥1 child mode 與要求不符 → 硬違反。
pub(crate) fn enforce(
    children: &[EngineNode],
    slot_modes: &[Mode],
    rule: TradRuleId,
) -> Option<Vec<TradRuleId>> {
    let mut deferred: Vec<TradRuleId> = Vec::new();
    for (c, &req) in children.iter().zip(slot_modes.iter()) {
        if req == Mode::Unknown {
            continue;
        }
        match c.mode {
            Mode::Unknown => deferred.push(rule), // degree-0:線內不可見 → defer
            m if m != req => return None,          // 硬違反
            _ => {}
        }
    }
    deferred.dedup();
    Some(deferred)
}

/// 以 children 建 parent 節點(degree_level = max(child)+1,span/方向自動推)。
pub(crate) fn build_parent(
    kind: PatternKind,
    children: Vec<EngineNode>,
    passed: Vec<TradRuleId>,
    deferred: Vec<TradRuleId>,
    diag: Option<(DiagonalKind, DiagonalShape, DiagonalSub)>,
    variant: Option<String>,
) -> EngineNode {
    let degree_level = children.iter().map(|c| c.degree_level).max().unwrap_or(0) + 1;
    let first = &children[0];
    let last = children.last().unwrap();
    let direction = if last.end_price >= first.start_price {
        Direction::Up
    } else {
        Direction::Down
    };
    EngineNode {
        kind,
        mode: kind.mode(),
        direction,
        degree_level,
        start_bar: first.start_bar,
        end_bar: last.end_bar,
        start_date: first.start_date,
        end_date: last.end_date,
        start_price: first.start_price,
        end_price: last.end_price,
        diag,
        variant,
        children,
        passed_rules: passed,
        deferred_rules: deferred,
    }
}
