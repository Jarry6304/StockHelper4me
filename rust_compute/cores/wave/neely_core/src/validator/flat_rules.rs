// flat_rules.rs — Validator F1-F2(Flat 子規則)
//
// 對齊 m2Spec/oldm2Spec/neely_core.md §十(規則組)。
//
// **M3 PR-3b 階段**:F1-F2 全部 Deferred。具體 Flat 規則(Regular / Expanded /
// Running 子型號的識別 + B-wave / C-wave 比例約束)等 user 在 m3Spec/ 寫最新
// neely_core spec 後 batch 補。

use super::RuleResult;
use crate::candidates::WaveCandidate;
use crate::monowave::ClassifiedMonowave;
use crate::output::RuleId;

pub fn run(
    _candidate: &WaveCandidate,
    _classified: &[ClassifiedMonowave],
) -> Vec<RuleResult> {
    vec![
        RuleResult::Deferred(RuleId::Flat(1)),
        RuleResult::Deferred(RuleId::Flat(2)),
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
    }
}
