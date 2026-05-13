// wave_rules.rs — Validator Ch5 Equality + Alternation 規則
//
// 對齊 m3Spec/neely_core_architecture.md §9.3 + m3Spec/neely_rules.md §Conditional Construction Rules。
//
// **Phase 1 PR**:2 條規則 framework 落地(從 r4 自編號 Wave 對映 r5 §9.3 Ch5 規則),
// **body Deferred**。
//
// **r4 → r5 對映**:
//   - Wave(1)(自編)→ Ch5_Equality(W1/W3/W5 中非延伸的兩個傾向等價,W3 Extension 最強)
//   - Wave(2)(自編)→ Ch5_Alternation { Construction }(W2/W4 在 Construction 軸 alternation)
//
// 具體門檻邏輯留 P4 / P5。

use super::RuleResult;
use crate::candidates::WaveCandidate;
use crate::monowave::ClassifiedMonowave;
use crate::output::{AlternationAxis, RuleId};

pub fn run(
    _candidate: &WaveCandidate,
    _classified: &[ClassifiedMonowave],
) -> Vec<RuleResult> {
    vec![
        RuleResult::Deferred(RuleId::Ch5_Equality),
        RuleResult::Deferred(RuleId::Ch5_Alternation {
            axis: AlternationAxis::Construction,
        }),
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
        if let RuleResult::Deferred(rid) = &results[0] {
            assert!(matches!(rid, RuleId::Ch5_Equality));
        }
        if let RuleResult::Deferred(rid) = &results[1] {
            assert!(matches!(
                rid,
                RuleId::Ch5_Alternation {
                    axis: AlternationAxis::Construction
                }
            ));
        }
    }
}
