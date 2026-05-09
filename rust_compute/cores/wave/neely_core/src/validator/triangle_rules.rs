// triangle_rules.rs — Validator T1-T10(Triangle 子規則)
//
// 對齊 m2Spec/oldm2Spec/neely_core.md §十(規則組)。
//
// **M3 PR-3b 階段**:T1-T10 全部 Deferred。具體 Triangle 規則(Contracting /
// Expanding / Limiting 子型號 + 5-3-3-3-3 sub-wave 結構 + 收斂 / 擴散約束)
// 等 user 在 m3Spec/ 寫最新 neely_core spec 後 batch 補。

use super::RuleResult;
use crate::candidates::WaveCandidate;
use crate::monowave::ClassifiedMonowave;
use crate::output::RuleId;

pub fn run(
    _candidate: &WaveCandidate,
    _classified: &[ClassifiedMonowave],
) -> Vec<RuleResult> {
    (1u8..=10).map(|n| RuleResult::Deferred(RuleId::Triangle(n))).collect()
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
        assert_eq!(results.len(), 10);
        for r in &results {
            assert!(r.is_deferred());
        }
    }
}
