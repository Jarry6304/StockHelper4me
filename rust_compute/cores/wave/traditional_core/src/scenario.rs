// scenario.rs — v3:top EngineNode → 凍結的 output::TraditionalScenario。
//
// 透過 candidates::Candidate adapter 重用既有 guidelines::evaluate / fibonacci::project /
// triggers::build(它們只用 pivot.price + direction,pivot.kind 不影響輸出)。

use crate::candidates::{Candidate, HypoKind};
use crate::config::TraditionalEngineConfig;
use crate::degree::degree_for_span;
use crate::mode::PatternKind;
use crate::node::EngineNode;
use crate::output::{Direction, Pivot, PivotKind, TraditionalScenario};
use crate::{fibonacci, guidelines, triggers};

pub fn assemble(top_nodes: &[EngineNode], config: &TraditionalEngineConfig) -> Vec<TraditionalScenario> {
    top_nodes.iter().map(|n| assemble_one(n, config)).collect()
}

fn assemble_one(node: &EngineNode, config: &TraditionalEngineConfig) -> TraditionalScenario {
    let cand = node_to_candidate(node);
    let (guidelines_satisfied, qualifiers_met) = guidelines::evaluate(&cand, config.fib_tolerance);
    let preference_score = guidelines_satisfied.len() + qualifiers_met.len();
    let root = root_label_of(node);
    let expected_fib_zones = fibonacci::project(&cand, &root, config.fib_tolerance);
    let invalidation_triggers = triggers::build(&cand);
    let degree = degree_for_span(node.span_bars());
    let structure_label = structure_label_of(node);

    TraditionalScenario {
        id: node.canonical_key(),
        wave_tree: node.to_wave_node(root),
        pattern_type: node.pattern_type(),
        direction: node.direction,
        structure_label,
        degree,
        passed_rules: node.passed_rules.clone(),
        deferred_rules: node.deferred_rules.clone(),
        guidelines_satisfied,
        qualifiers_met,
        preference_score,
        invalidation_triggers,
        expected_fib_zones,
    }
}

fn hypo_of(kind: PatternKind) -> HypoKind {
    match kind {
        PatternKind::Impulse => HypoKind::Impulse,
        PatternKind::LeadingDiagonal | PatternKind::EndingDiagonal => HypoKind::Diagonal,
        PatternKind::Zigzag => HypoKind::Zigzag,
        PatternKind::Flat | PatternKind::Combination => HypoKind::Flat,
        PatternKind::Triangle => HypoKind::Triangle,
        PatternKind::Monowave => HypoKind::Zigzag,
    }
}

fn node_to_candidate(node: &EngineNode) -> Candidate {
    let mut pivots: Vec<Pivot> = Vec::new();
    for (i, ch) in node.children.iter().enumerate() {
        if i == 0 {
            pivots.push(Pivot {
                bar_index: ch.start_bar,
                date: ch.start_date,
                price: ch.start_price,
                kind: PivotKind::Low, // 下游 helper 不用 kind
            });
        }
        pivots.push(Pivot {
            bar_index: ch.end_bar,
            date: ch.end_date,
            price: ch.end_price,
            kind: PivotKind::Low,
        });
    }
    Candidate {
        id: node.canonical_key(),
        pivots,
        hypo: hypo_of(node.kind),
        direction: node.direction,
    }
}

fn root_label_of(node: &EngineNode) -> String {
    match node.kind {
        PatternKind::Impulse => "Impulse",
        PatternKind::LeadingDiagonal => "Leading Diagonal",
        PatternKind::EndingDiagonal => "Ending Diagonal",
        PatternKind::Zigzag => "Zigzag",
        PatternKind::Flat => "Flat",
        PatternKind::Triangle => "Triangle",
        PatternKind::Combination => "Combination",
        PatternKind::Monowave => "Monowave",
    }
    .to_string()
}

fn structure_label_of(node: &EngineNode) -> String {
    let dir = if matches!(node.direction, Direction::Up) {
        "↑"
    } else {
        "↓"
    };
    let labels = node.kind.child_labels().join("-");
    let sub = node
        .diag
        .map(|(_, shape, s)| format!(", {:?}/{:?}", s, shape))
        .unwrap_or_default();
    let variant = node
        .variant
        .clone()
        .map(|v| format!(", {}", v))
        .unwrap_or_default();
    let deferred = if node.deferred_rules.is_empty() {
        String::new()
    } else {
        " [子浪細分 deferred]".to_string()
    };
    format!(
        "{} ({}{}{}, {} · deg{}){}",
        labels,
        root_label_of(node),
        sub,
        variant,
        dir,
        node.degree_level,
        deferred
    )
}
