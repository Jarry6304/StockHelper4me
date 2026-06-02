// patterns/combination.rs — 組合 grouper(Double/Triple Three + Double/Triple Zigzag)。
//
// 結構:W-X-Y(double)/ W-X-Y-X-Z(triple)。組件(actionary,偶數 index)= 簡單修正形態
// (zigzag/flat/triangle);X(reactionary,奇數 index)= 連接修正。**組合由已分類的修正形態組成**
// → 要求所有 children degree≥1 且 mode=Corrective(裸 monowave 不成組合)。
//
// 分流(對齊原書 Glossary 區分 Double Zigzag vs Double Three):
//   - 組件全是 Zigzag → Double/Triple **Zigzag**(陡峭多重鋸齒),R12 不適用。
//   - 否則 → Double/Triple **Three**:HARD R12(≤1 zigzag、≤1 triangle、triangle 僅最終組件)。

use super::build_parent;
use crate::mode::{Mode, PatternKind};
use crate::node::EngineNode;
use crate::output::TradRuleId;
use crate::rules::alternates;

pub fn group(w: &[EngineNode]) -> Vec<EngineNode> {
    let n = w.len();
    if (n != 3 && n != 5) || !alternates(w) {
        return Vec::new();
    }
    // 組合由「已分類的修正形態」組成:所有 child 須 degree≥1 且 Corrective
    if w.iter().any(|c| c.degree_level < 1 || c.mode != Mode::Corrective) {
        return Vec::new();
    }

    // 組件 = actionary slots(偶數 index):double=[0,2]、triple=[0,2,4]
    let comp_idx: Vec<usize> = (0..n).step_by(2).collect();
    let all_zigzag = comp_idx.iter().all(|&i| w[i].kind == PatternKind::Zigzag);

    let (passed, variant) = if all_zigzag {
        // Double/Triple Zigzag(多重鋸齒,非 three-combination)→ R12 不適用
        let v = if n == 3 { "double_zigzag" } else { "triple_zigzag" };
        (vec![TradRuleId::R10CorrectionNeverFive], v.to_string())
    } else {
        // Double/Triple Three:HARD R12
        let zz = comp_idx
            .iter()
            .filter(|&&i| w[i].kind == PatternKind::Zigzag)
            .count();
        let tri = comp_idx
            .iter()
            .filter(|&&i| w[i].kind == PatternKind::Triangle)
            .count();
        if zz > 1 || tri > 1 {
            return Vec::new(); // R12:至多一鋸齒、至多一三角
        }
        if tri == 1 {
            let last = *comp_idx.last().unwrap();
            if w[last].kind != PatternKind::Triangle {
                return Vec::new(); // R12:三角僅作最終組件
            }
        }
        let v = if n == 3 { "double_three" } else { "triple_three" };
        (
            vec![
                TradRuleId::R10CorrectionNeverFive,
                TradRuleId::R12CombinationConstraint,
            ],
            v.to_string(),
        )
    };

    vec![build_parent(
        PatternKind::Combination,
        w.to_vec(),
        passed,
        Vec::new(),
        None,
        Some(variant),
    )]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::Direction;
    use chrono::NaiveDate;

    fn comp(kind: PatternKind, sp: f64, ep: f64, sb: usize, eb: usize) -> EngineNode {
        let dir = if ep >= sp { Direction::Up } else { Direction::Down };
        EngineNode {
            kind,
            mode: Mode::Corrective,
            direction: dir,
            degree_level: 1,
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

    // W=flat(↓) X=zigzag(↑) Y=flat(↓) → Double Three,R12 passed
    fn wxy(k0: PatternKind, k1: PatternKind, k2: PatternKind) -> Vec<EngineNode> {
        vec![
            comp(k0, 20.0, 17.0, 0, 3),
            comp(k1, 17.0, 19.0, 3, 6),
            comp(k2, 19.0, 16.0, 6, 9),
        ]
    }

    #[test]
    fn double_three_forms_and_passes_r12() {
        let out = group(&wxy(PatternKind::Flat, PatternKind::Zigzag, PatternKind::Flat));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].kind, PatternKind::Combination);
        assert!(out[0].passed_rules.contains(&TradRuleId::R12CombinationConstraint));
        assert_eq!(out[0].variant.as_deref(), Some("double_three"));
    }

    #[test]
    fn double_zigzag_allowed_without_r12() {
        let out = group(&wxy(PatternKind::Zigzag, PatternKind::Zigzag, PatternKind::Zigzag));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].variant.as_deref(), Some("double_zigzag"));
        assert!(!out[0].passed_rules.contains(&TradRuleId::R12CombinationConstraint));
    }

    #[test]
    fn triangle_not_final_rejected_by_r12() {
        // W=triangle(非最終)→ R12 淘汰
        assert!(group(&wxy(PatternKind::Triangle, PatternKind::Zigzag, PatternKind::Flat)).is_empty());
    }

    #[test]
    fn triangle_as_final_component_ok() {
        let out = group(&wxy(PatternKind::Flat, PatternKind::Zigzag, PatternKind::Triangle));
        assert_eq!(out.len(), 1);
        assert!(out[0].passed_rules.contains(&TradRuleId::R12CombinationConstraint));
    }

    #[test]
    fn monowave_children_rejected() {
        // degree-0 / Unknown → 不成組合
        let mut w = wxy(PatternKind::Flat, PatternKind::Zigzag, PatternKind::Flat);
        for c in &mut w {
            c.degree_level = 0;
            c.mode = Mode::Unknown;
            c.kind = PatternKind::Monowave;
        }
        assert!(group(&w).is_empty());
    }
}
