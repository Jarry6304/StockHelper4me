// wave_rules.rs — Validator W1-W2(通用波浪規則,r5 §9.3)
//
// 對齊 m3Spec/neely_rules.md Ch5 p.5-34~47 + Ch11 p.11-4~18 + Ch12 Fibonacci。
//
// **PR-3c-pre 階段(2026-05-13)**:W1-W2 全部 Deferred。
// PR-3c-1 動工:
//   - W1 = Ch11ImpulseWaveByWave { ext: ThirdExt, wave: Three }:Impulse Extension
//     6 情境(1st/3rd/5th Ext × Trending/Terminal),預設 3rd Ext Trending(最常見)
//   - W2 = Ch12FibonacciInternal:Essential Construction Rules + Fibonacci 內部
//     比例(wave-c Zigzag 常 = a;Triangle 常有 61.8% 比例)

use super::RuleResult;
use crate::candidates::WaveCandidate;
use crate::monowave::ClassifiedMonowave;
use crate::output::{ImpulseExtension, RuleId, WaveNumber};

pub fn run(
    _candidate: &WaveCandidate,
    _classified: &[ClassifiedMonowave],
) -> Vec<RuleResult> {
    vec![
        RuleResult::Deferred(RuleId::Ch11ImpulseWaveByWave {
            ext: ImpulseExtension::ThirdExt,
            wave: WaveNumber::Three,
        }),
        RuleResult::Deferred(RuleId::Ch12FibonacciInternal),
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
