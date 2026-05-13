// triangle_rules.rs — Validator Ch5 Triangle 子規則
//
// 對齊 m3Spec/neely_core_architecture.md §9.3 + m3Spec/neely_rules.md §Triangles(3-3-3-3-3)。
//
// **Phase 1 PR**:3 條規則 framework 落地(從 r4 自編號 T1-T10 對映 r5 §9.3 3 條),
// **body Deferred**。具體門檻 + 5-leg sub-wave 結構 + Contracting/Expanding/Limiting
// 子型號識別邏輯留 P4。
//
// **r4 → r5 對映**:
//   - T1-T2(自編)→ Ch5_Triangle_BRange(b 的價格範圍約束)
//   - T3-T5(自編)→ Ch5_Triangle_LegContraction(leg 收斂/擴張)
//   - T6-T8(自編)→ Ch5_Triangle_LegEquality_5Pct(三條同度數腿 ±5% 等價)
//   - T9-T10(自編)→ 無 Ch5 對應(歸 Ch11_Triangle_Variant_Rules 留後續 PR)

use super::RuleResult;
use crate::candidates::WaveCandidate;
use crate::monowave::ClassifiedMonowave;
use crate::output::RuleId;

pub fn run(
    _candidate: &WaveCandidate,
    _classified: &[ClassifiedMonowave],
) -> Vec<RuleResult> {
    vec![
        RuleResult::Deferred(RuleId::Ch5_Triangle_BRange),
        RuleResult::Deferred(RuleId::Ch5_Triangle_LegContraction),
        RuleResult::Deferred(RuleId::Ch5_Triangle_LegEquality_5Pct),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::MonowaveDirection;

    #[test]
    fn all_triangle_rules_deferred() {
        let candidate = WaveCandidate {
            id: "c5-test".to_string(),
            monowave_indices: vec![0, 1, 2, 3, 4],
            wave_count: 5,
            initial_direction: MonowaveDirection::Up,
        };
        let results = run(&candidate, &[]);
        assert_eq!(results.len(), 3);
        for r in &results {
            assert!(r.is_deferred());
        }
        if let RuleResult::Deferred(rid) = &results[0] {
            assert!(matches!(rid, RuleId::Ch5_Triangle_BRange));
        }
        if let RuleResult::Deferred(rid) = &results[2] {
            assert!(matches!(rid, RuleId::Ch5_Triangle_LegEquality_5Pct));
        }
    }
}
