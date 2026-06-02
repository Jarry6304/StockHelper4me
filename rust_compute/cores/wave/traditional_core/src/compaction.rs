// compaction.rs — v3 round 引擎(由下而上逐度數建構)。
//
// 每 round 對「當前 tiling」的連續視窗(3/5)試 patterns::try_all 組成下一度數節點;
// 多種合法分組 = 分支(alternates),per-round dedup + beam-cap(top-N by 聚合度)控爆炸;
// degree ceiling = max_degree_levels。forest = 最終各 tiling 的 top-level degree≥1 節點(去重)。
//
// Round-3-pause 類比:某 round 完全無法再聚合 → break。

use crate::config::TraditionalEngineConfig;
use crate::node::EngineNode;
use crate::patterns::try_all;
use std::collections::HashSet;

pub fn compact(base: Vec<EngineNode>, config: &TraditionalEngineConfig) -> Vec<EngineNode> {
    if base.len() < 3 {
        return Vec::new();
    }
    let mut scenarios: Vec<Vec<EngineNode>> = vec![base];

    for _ in 0..config.max_degree_levels {
        let mut next: Vec<Vec<EngineNode>> = Vec::new();
        let mut any_aggregated = false;
        for sc in &scenarios {
            let aggs = aggregate_one(sc);
            if aggs.is_empty() {
                next.push(sc.clone()); // terminal tiling 保留
            } else {
                any_aggregated = true;
                next.extend(aggs);
                next.push(sc.clone()); // 也留 un-aggregated 當「停在此度數」alternate
            }
        }
        dedup_scenarios(&mut next);
        beam_cap(&mut next, config.round_beam_size);
        scenarios = next;
        if !any_aggregated {
            break;
        }
    }

    collect_top_nodes(&scenarios)
}

/// 對一個 tiling,枚舉所有「替換一個連續視窗為其 parent」的新 tiling。
fn aggregate_one(sc: &[EngineNode]) -> Vec<Vec<EngineNode>> {
    let mut out = Vec::new();
    let n = sc.len();
    for &wlen in &[3usize, 5usize] {
        if n < wlen {
            continue;
        }
        for start in 0..=(n - wlen) {
            let window = &sc[start..start + wlen];
            for parent in try_all(window) {
                let mut ns: Vec<EngineNode> = Vec::with_capacity(n - wlen + 1);
                ns.extend_from_slice(&sc[..start]);
                ns.push(parent);
                ns.extend_from_slice(&sc[start + wlen..]);
                out.push(ns);
            }
        }
    }
    out
}

fn scenario_key(sc: &[EngineNode]) -> String {
    sc.iter()
        .map(|n| n.canonical_key())
        .collect::<Vec<_>>()
        .join(";")
}

fn dedup_scenarios(scs: &mut Vec<Vec<EngineNode>>) {
    let mut seen = HashSet::new();
    scs.retain(|sc| seen.insert(scenario_key(sc)));
}

/// 聚合度評分(top 節點 degree_level 總和;愈高 = 愈多度數結構)。beam 偏好深樹。
fn scenario_score(sc: &[EngineNode]) -> usize {
    sc.iter().map(|n| n.degree_level).sum()
}

fn beam_cap(scs: &mut Vec<Vec<EngineNode>>, beam: usize) {
    if scs.len() <= beam {
        return;
    }
    scs.sort_by(|a, b| scenario_score(b).cmp(&scenario_score(a)));
    scs.truncate(beam);
}

/// 收 forest:各最終 tiling 的 top-level degree≥1 節點(pattern,非裸 monowave),去重。
fn collect_top_nodes(scenarios: &[Vec<EngineNode>]) -> Vec<EngineNode> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for sc in scenarios {
        for node in sc {
            if node.degree_level >= 1 && seen.insert(node.canonical_key()) {
                out.push(node.clone());
            }
        }
    }
    out
}
