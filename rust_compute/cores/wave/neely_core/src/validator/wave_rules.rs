// wave_rules.rs — Validator W1-W2(通用波浪規則)
//
// 對齊 m2Spec/oldm2Spec/neely_core.md §十(規則組)。
//
// **M3 PR-3b 階段**:W1-W2 全部 Deferred。具體規則(Channeling / Volume
// alignment / Time relationship 等通用波浪約束)等 user 在 m3Spec/ 寫最新
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
        RuleResult::Deferred(RuleId::Wave(1)),
        RuleResult::Deferred(RuleId::Wave(2)),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::MonowaveDirection;

    #[test]
    fn all_wave_rules_deferred() {
        let candidate = WaveCandidate {
            id: "c5-test".to_string(),
            monowave_indices: vec![0, 1, 2, 3, 4],
            wave_count: 5,
            initial_direction: MonowaveDirection::Up,
        };
        let results = run(&candidate, &[]);
        assert_eq!(results.len(), 2);
        for r in &results {
            assert!(r.is_deferred());
        }
    }
}
