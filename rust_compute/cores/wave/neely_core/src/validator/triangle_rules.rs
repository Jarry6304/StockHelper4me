// triangle_rules.rs — Validator T1-T10(Triangle 子規則,r5 §9.3)
//
// 對齊 m3Spec/neely_rules.md Ch5 + Ch11 p.11-19~30 + architecture.md TriangleVariant(9 種)。
//
// **PR-3c-pre 階段(2026-05-13)**:T1-T10 全部 Deferred。
// PR-3c-2 動工:對映 spec r5 §9.3 line 1016 `Ch11TriangleVariantRules { variant, wave }`,
// 9 個 TriangleVariant × 5 wave(A-E)= 45 種具體規則,本 stub 階段先用 10 條代表性
// rule_id 標記:
//   - T1-T3 = 3 種 Contracting Limiting variant 的 wave-c(收斂主軸)
//   - T4 = Ch5TriangleBRange(b-wave 範圍)
//   - T5 = Ch5TriangleLegContraction(每段更短)
//   - T6 = Ch5TriangleLegEquality5Pct(等邊 5% 容差)
//   - T7-T9 = 3 種 Expanding variant 的 wave-e
//   - T10 = Ch6TriangleExpandingNonConfirmation

use super::RuleResult;
use crate::candidates::WaveCandidate;
use crate::monowave::ClassifiedMonowave;
use crate::output::{RuleId, TriangleVariant, TriangleWave};

pub fn run(
    _candidate: &WaveCandidate,
    _classified: &[ClassifiedMonowave],
) -> Vec<RuleResult> {
    vec![
        // T1-T3: Contracting Limiting 3 種(Horizontal / Irregular / Running)wave-c
        RuleResult::Deferred(RuleId::Ch11TriangleVariantRules {
            variant: TriangleVariant::HorizontalLimiting, wave: TriangleWave::C,
        }),
        RuleResult::Deferred(RuleId::Ch11TriangleVariantRules {
            variant: TriangleVariant::IrregularLimiting, wave: TriangleWave::C,
        }),
        RuleResult::Deferred(RuleId::Ch11TriangleVariantRules {
            variant: TriangleVariant::RunningLimiting, wave: TriangleWave::C,
        }),
        // T4-T6: Ch5 通用 Triangle 規則
        RuleResult::Deferred(RuleId::Ch5TriangleBRange),
        RuleResult::Deferred(RuleId::Ch5TriangleLegContraction),
        RuleResult::Deferred(RuleId::Ch5TriangleLegEquality5Pct),
        // T7-T9: Expanding 3 種 wave-e
        RuleResult::Deferred(RuleId::Ch11TriangleVariantRules {
            variant: TriangleVariant::HorizontalExpanding, wave: TriangleWave::E,
        }),
        RuleResult::Deferred(RuleId::Ch11TriangleVariantRules {
            variant: TriangleVariant::IrregularExpanding, wave: TriangleWave::E,
        }),
        RuleResult::Deferred(RuleId::Ch11TriangleVariantRules {
            variant: TriangleVariant::RunningExpanding, wave: TriangleWave::E,
        }),
        // T10: Ch6 Expanding Non-Confirmation
        RuleResult::Deferred(RuleId::Ch6TriangleExpandingNonConfirmation),
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
        assert_eq!(results.len(), 10);
        for r in &results {
            assert!(r.is_deferred());
        }
    }
}
