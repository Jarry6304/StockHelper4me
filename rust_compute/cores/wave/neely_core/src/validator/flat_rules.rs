// flat_rules.rs — Validator Ch5 Flat 子規則(Ch5_Flat_Min_BRatio / Ch5_Flat_Min_CRatio)
//
// 對齊 m3Spec/neely_core_architecture.md §9.3 + m3Spec/neely_rules.md §Flats(3-3-5)。
//
// **Phase 1 PR**:2 條規則 framework 落地,**body Deferred**。
// 具體門檻 + B-wave / C-wave 比例約束邏輯留 P4(Stage 4 Flat/Zigzag/Triangle 變體 PR)。

use super::RuleResult;
use crate::candidates::WaveCandidate;
use crate::monowave::ClassifiedMonowave;
use crate::output::RuleId;

pub fn run(
    _candidate: &WaveCandidate,
    _classified: &[ClassifiedMonowave],
) -> Vec<RuleResult> {
    vec![
        RuleResult::Deferred(RuleId::Ch5_Flat_Min_BRatio),
        RuleResult::Deferred(RuleId::Ch5_Flat_Min_CRatio),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::MonowaveDirection;

    #[test]
    fn all_flat_rules_deferred() {
        let candidate = WaveCandidate {
            id: "c3-test".to_string(),
            monowave_indices: vec![0, 1, 2],
            wave_count: 3,
            initial_direction: MonowaveDirection::Up,
        };
        let results = run(&candidate, &[]);
        assert_eq!(results.len(), 2);
        for r in &results {
            assert!(r.is_deferred());
        }
        // 確認 RuleId 編碼對齊 r5 §9.3
        if let RuleResult::Deferred(rid) = &results[0] {
            assert!(matches!(rid, RuleId::Ch5_Flat_Min_BRatio));
        }
        if let RuleResult::Deferred(rid) = &results[1] {
            assert!(matches!(rid, RuleId::Ch5_Flat_Min_CRatio));
        }
    }
}
