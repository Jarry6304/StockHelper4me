// zigzag_rules.rs — Validator Z1-Z4(Zigzag 子規則)
//
// 對齊 m2Spec/oldm2Spec/neely_core.md §十(規則組)。
//
// **M3 PR-3b 階段**:Z1-Z4 全部 Deferred。具體 Zigzag 規則(Single / Double /
// Triple 子型號 + Fibonacci 比率約束)等 user 在 m3Spec/ 寫最新 neely_core spec
// 後 batch 補。

use super::RuleResult;
use crate::candidates::WaveCandidate;
use crate::monowave::ClassifiedMonowave;
use crate::output::RuleId;

pub fn run(
    _candidate: &WaveCandidate,
    _classified: &[ClassifiedMonowave],
) -> Vec<RuleResult> {
    vec![
        RuleResult::Deferred(RuleId::Zigzag(1)),
        RuleResult::Deferred(RuleId::Zigzag(2)),
        RuleResult::Deferred(RuleId::Zigzag(3)),
        RuleResult::Deferred(RuleId::Zigzag(4)),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::MonowaveDirection;

    #[test]
    fn all_zigzag_rules_deferred() {
        let candidate = WaveCandidate {
            id: "c3-test".to_string(),
            monowave_indices: vec![0, 1, 2],
            wave_count: 3,
            initial_direction: MonowaveDirection::Up,
        };
        let results = run(&candidate, &[]);
        assert_eq!(results.len(), 4);
        for r in &results {
            assert!(r.is_deferred());
        }
    }
}
