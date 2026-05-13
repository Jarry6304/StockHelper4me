// zigzag_rules.rs — Validator Z1-Z4(Zigzag 子規則,r5 §9.3)
//
// 對齊 m3Spec/neely_rules.md Ch5 p.5-41~42 + Ch4 p.4-15~20 + Ch11 p.11-17~18。
//
// **PR-3c-pre 階段(2026-05-13)**:Z1-Z4 全部 Deferred。
// PR-3c-1 動工:
//   - Z1 = Ch5ZigzagMaxBRetracement:b-wave ≤ 61.8% × a(v1.9 修正,刪除「下限 1%」)
//   - Z2 = Ch11ZigzagWaveByWave { wave: C }:c-wave 範圍(Normal 61.8-161.8% /
//     Truncated 38.2-61.8% / Elongated > 161.8%)
//   - Z3 = Ch4ZigzagDetour:避免誤判 Impulse 為 Zigzag(DETOUR Test)
//   - Z4 = Ch5ZigzagCTriangleException:Triangle 內 Zigzag c-wave 例外(c 可破範圍上下限)

use super::RuleResult;
use crate::candidates::WaveCandidate;
use crate::monowave::ClassifiedMonowave;
use crate::output::{RuleId, WaveAbc};

pub fn run(
    _candidate: &WaveCandidate,
    _classified: &[ClassifiedMonowave],
) -> Vec<RuleResult> {
    vec![
        RuleResult::Deferred(RuleId::Ch5ZigzagMaxBRetracement),
        RuleResult::Deferred(RuleId::Ch11ZigzagWaveByWave { wave: WaveAbc::C }),
        RuleResult::Deferred(RuleId::Ch4ZigzagDetour),
        RuleResult::Deferred(RuleId::Ch5ZigzagCTriangleException),
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
