// flat_rules.rs — Validator F1-F2(Flat 子規則,r5 §9.3 + Ch5)
//
// 對齊 m3Spec/neely_rules.md Ch5 p.5-34~36 + Ch11 p.11 Flat 8 變體。
//
// **PR-3c-pre 階段(2026-05-13)**:F1-F2 全部 Deferred。
// PR-3c-1 動工:
//   - F1 = Ch5FlatMinBRatio:b-wave 回測比規則(Normal 81-100% / Weak 61.8-80% /
//     Strong 101-138.2% / Very Strong > 138.2% → Running Correction)
//   - F2 = Ch5FlatMinCRatio:c-wave 相對於 b-wave 比例(Common c≈a / C-Failure
//     c<100%×b / Elongated c>138.2%×b / Double Failure c<100%×b AND b<81%×a)

use super::RuleResult;
use crate::candidates::WaveCandidate;
use crate::monowave::ClassifiedMonowave;
use crate::output::RuleId;

pub fn run(
    _candidate: &WaveCandidate,
    _classified: &[ClassifiedMonowave],
) -> Vec<RuleResult> {
    vec![
        RuleResult::Deferred(RuleId::Ch5FlatMinBRatio),
        RuleResult::Deferred(RuleId::Ch5FlatMinCRatio),
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
