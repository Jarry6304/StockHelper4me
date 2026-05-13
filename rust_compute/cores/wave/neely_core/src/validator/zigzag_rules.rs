// zigzag_rules.rs — Validator Ch5 Zigzag 子規則
//
// 對齊 m3Spec/neely_core_architecture.md §9.3 + m3Spec/neely_rules.md §Zigzags(5-3-5)。
//
// **Phase 1 PR**:2 條規則 framework 落地(從 r4 自編號 Z1-Z4 對映 r5 §9.3 2 條),
// **body Deferred**。具體門檻留 P4。
//
// **r4 → r5 對映**:
//   - Z1(自編)→ Ch5_Zigzag_Max_BRetracement(b ≤ 61.8% × a)
//   - Z2(自編)→ Ch5_Zigzag_C_TriangleException(c-wave Triangle 例外)
//   - Z3, Z4(自編)→ r5 沒對應 Ch5 zigzag 規則(細項邏輯歸 Ch11_Zigzag_WaveByWave)
//     P4 動工時若需要可用 Ch11_* variants

use super::RuleResult;
use crate::candidates::WaveCandidate;
use crate::monowave::ClassifiedMonowave;
use crate::output::RuleId;

pub fn run(
    _candidate: &WaveCandidate,
    _classified: &[ClassifiedMonowave],
) -> Vec<RuleResult> {
    vec![
        RuleResult::Deferred(RuleId::Ch5_Zigzag_Max_BRetracement),
        RuleResult::Deferred(RuleId::Ch5_Zigzag_C_TriangleException),
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
        assert_eq!(results.len(), 2);
        for r in &results {
            assert!(r.is_deferred());
        }
        if let RuleResult::Deferred(rid) = &results[0] {
            assert!(matches!(rid, RuleId::Ch5_Zigzag_Max_BRetracement));
        }
    }
}
