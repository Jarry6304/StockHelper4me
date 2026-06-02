// classifier.rs — Stage 4:形態確認 + 波浪樹
//
// 對齊 traditional_rules.md L4/L5/L6–9 + references/engine.md(對角分類 v2:kind/shape/sub)。
//
// v1 限制(產品閘需知):對角的 `kind`(Leading vs Ending)與 `sub`(5-3-5-3-5 vs 3-3-3-3-3)
// 嚴格需「上層 degree 位置」與「遞迴子浪細分」context;v1 pivot-level 以幾何啟發給 best-guess
// (shape 由 leg 幅度趨勢判;kind 預設 Leading;sub 預設 AllThrees = rev2 較常觀察者),
// 並於 structure_label 標記。真正定 kind/sub 屬 v2 深度(遞迴分解 + multi-timeframe context)。

use crate::candidates::{Candidate, HypoKind};
use crate::output::{
    Direction, DiagonalKind, DiagonalShape, DiagonalSub, Pivot, TraditionalPatternType, WaveNode,
};

pub struct Classified {
    pub pattern_type: TraditionalPatternType,
    pub structure_label: String,
    pub wave_tree: WaveNode,
}

fn amp(p: &[Pivot], a: usize, b: usize) -> f64 {
    (p[b].price - p[a].price).abs()
}

fn dir_sym(d: Direction) -> &'static str {
    match d {
        Direction::Up => "↑",
        Direction::Down => "↓",
    }
}

fn build_tree(pivots: &[Pivot], labels: &[&str], root_label: &str) -> WaveNode {
    let children = labels
        .iter()
        .enumerate()
        .map(|(i, lab)| WaveNode {
            label: lab.to_string(),
            start: pivots[i].date,
            end: pivots[i + 1].date,
            start_price: pivots[i].price,
            end_price: pivots[i + 1].price,
            children: Vec::new(),
        })
        .collect();
    let last = pivots.len() - 1;
    WaveNode {
        label: root_label.to_string(),
        start: pivots[0].date,
        end: pivots[last].date,
        start_price: pivots[0].price,
        end_price: pivots[last].price,
        children,
    }
}

pub fn classify(c: &Candidate) -> Classified {
    let p = &c.pivots;
    let d = c.direction;
    match c.hypo {
        HypoKind::Impulse => Classified {
            pattern_type: TraditionalPatternType::Impulse,
            structure_label: format!("1-2-3-4-5 (Impulse, {})", dir_sym(d)),
            wave_tree: build_tree(p, &["1", "2", "3", "4", "5"], "Impulse"),
        },
        HypoKind::Diagonal => {
            // shape:行動浪 W1 vs W5 幅度趨勢(收斂 / 擴張)
            let shape = if amp(p, 4, 5) < amp(p, 0, 1) {
                DiagonalShape::Contracting
            } else {
                DiagonalShape::Expanding
            };
            // v1 best-guess:kind=Leading、sub=AllThrees(rev2 較常觀察者),需 v2 遞迴/context 才能定論
            let kind = DiagonalKind::Leading;
            let sub = DiagonalSub::AllThrees;
            let shape_s = match shape {
                DiagonalShape::Contracting => "contracting",
                DiagonalShape::Expanding => "expanding",
            };
            Classified {
                pattern_type: TraditionalPatternType::Diagonal { kind, shape, sub },
                structure_label: format!(
                    "1-2-3-4-5 (Leading Diagonal, {}/3-3-3-3-3 [v1 best-guess], {})",
                    shape_s,
                    dir_sym(d)
                ),
                wave_tree: build_tree(p, &["1", "2", "3", "4", "5"], "Diagonal"),
            }
        }
        HypoKind::Zigzag => Classified {
            pattern_type: TraditionalPatternType::Zigzag,
            structure_label: format!("A-B-C (Zigzag, {})", dir_sym(d)),
            wave_tree: build_tree(p, &["A", "B", "C"], "Zigzag"),
        },
        HypoKind::Flat => Classified {
            pattern_type: TraditionalPatternType::Flat,
            structure_label: format!("A-B-C (Flat, {})", dir_sym(d)),
            wave_tree: build_tree(p, &["A", "B", "C"], "Flat"),
        },
        HypoKind::Triangle => {
            let shape_s = if amp(p, 4, 5) < amp(p, 0, 1) {
                "contracting"
            } else {
                "expanding"
            };
            Classified {
                pattern_type: TraditionalPatternType::Triangle,
                structure_label: format!("a-b-c-d-e (Triangle, {})", shape_s),
                wave_tree: build_tree(p, &["a", "b", "c", "d", "e"], "Triangle"),
            }
        }
    }
}
