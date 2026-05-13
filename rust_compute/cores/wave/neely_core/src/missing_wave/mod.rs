// missing_wave — Stage 9a:Missing Wave 偵測
//
// 對齊 m3Spec/neely_rules.md §Pre-Constructive Logic 細部技術備註(1054-1057 行)
//       + §Ch12 Missing Waves
//       + m3Spec/neely_core_architecture.md §7.1 Stage 9a
//
// **Phase 9 PR**(r5 alignment):
//   從 Phase 2 Pre-Constructive Logic 已標的 `MissingWaveBundle` certainty
//   萃取結構化資訊。Missing wave 標記在 P2 階段已寫入 monowave 的
//   structure_label_candidates(certainty = MissingWaveBundle),本 stage 把它們
//   重新組織成 Vec<MissingWaveSuspect> 供下游使用。
//
// **位置分類**(spec 1055-1057 行):
//   - m1 中心:含 BF3 bundle → M1Center(Flat b-wave Complex 場景)
//   - m0 中心:含 XC3 + S5 bundle → M0Center
//   - m2 終點:只含 XC3 bundle → M2Endpoint
//   - 其他組合:Ambiguous

use crate::monowave::ClassifiedMonowave;
use crate::output::{
    Certainty, MissingWavePosition, MissingWaveSuspect, Scenario, StructureLabel,
};

/// Stage 9a 主入口:從 classified monowaves 提取 MissingWaveBundle 資訊。
///
/// 對齊 spec 1054-1057:missing wave 標記成組捆綁,本 stage 把同一 monowave
/// 上的所有 MissingWaveBundle labels 視為一組 bundle。
pub fn detect(classified: &[ClassifiedMonowave]) -> Vec<MissingWaveSuspect> {
    let mut suspects = Vec::new();
    for (idx, cmw) in classified.iter().enumerate() {
        let bundle_labels: Vec<StructureLabel> = cmw
            .structure_label_candidates
            .iter()
            .filter(|c| matches!(c.certainty, Certainty::MissingWaveBundle))
            .map(|c| c.label)
            .collect();
        if bundle_labels.is_empty() {
            continue;
        }
        let position = classify_position(&bundle_labels);
        let message = format!(
            "Monowave[{}] missing-wave bundle 標記({} labels):位置 = {:?}",
            idx,
            bundle_labels.len(),
            position
        );
        suspects.push(MissingWaveSuspect {
            monowave_idx: idx,
            position,
            bundle_labels,
            message,
        });
    }
    suspects
}

/// 依 bundle labels 組合判位置(spec 1055-1057):
///   - 含 BF3 → M1Center(Flat b-wave Complex 場景)
///   - 含 XC3 + S5(且無 BF3)→ M0Center
///   - 只含 XC3 → M2Endpoint
///   - 其他組合 → Ambiguous
fn classify_position(labels: &[StructureLabel]) -> MissingWavePosition {
    let has_xc3 = labels.contains(&StructureLabel::XC3);
    let has_s5 = labels.contains(&StructureLabel::S5);
    let has_bf3 = labels.contains(&StructureLabel::BF3);

    if has_bf3 {
        MissingWavePosition::M1Center
    } else if has_xc3 && has_s5 {
        MissingWavePosition::M0Center
    } else if has_xc3 && !has_s5 && !has_bf3 {
        MissingWavePosition::M2Endpoint
    } else {
        MissingWavePosition::Ambiguous
    }
}

/// Legacy API(Phase 1 skeleton)— Phase 9 改用 detect(classified)。
/// 保留以避免既有 caller 破壞。
#[deprecated(note = "Phase 9 改用 detect(classified) 萃取整 monowave 序列的 missing waves")]
pub fn detect_missing_wave(_scenario: &Scenario) -> bool {
    false
}

/// Legacy API(Phase 1 skeleton)— Phase 9 改用 detect(classified)。
pub fn apply_to_forest(_forest: &[Scenario]) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::monowave::ProportionMetrics;
    use crate::output::{Monowave, MonowaveDirection, StructureLabelCandidate};
    use chrono::NaiveDate;

    fn cmw_with(labels: Vec<(StructureLabel, Certainty)>) -> ClassifiedMonowave {
        let cands = labels
            .into_iter()
            .map(|(l, c)| StructureLabelCandidate {
                label: l,
                certainty: c,
            })
            .collect();
        ClassifiedMonowave {
            monowave: Monowave {
                start_date: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                end_date: NaiveDate::from_ymd_opt(2026, 1, 5).unwrap(),
                start_price: 100.0,
                end_price: 110.0,
                direction: MonowaveDirection::Up,
            },
            atr_at_start: 1.0,
            metrics: ProportionMetrics {
                magnitude: 10.0,
                duration_bars: 5,
                atr_relative: 1.0,
                slope_vs_45deg: 1.0,
            },
            structure_label_candidates: cands,
        }
    }

    #[test]
    fn detect_empty_yields_no_suspects() {
        assert!(detect(&[]).is_empty());
    }

    #[test]
    fn detect_skips_monowaves_without_bundle_labels() {
        let classified = vec![
            cmw_with(vec![(StructureLabel::Five, Certainty::Primary)]),
            cmw_with(vec![(StructureLabel::C3, Certainty::Possible)]),
        ];
        assert!(detect(&classified).is_empty());
    }

    #[test]
    fn detect_xc3_plus_s5_yields_m0_center() {
        let classified = vec![cmw_with(vec![
            (StructureLabel::XC3, Certainty::MissingWaveBundle),
            (StructureLabel::S5, Certainty::MissingWaveBundle),
        ])];
        let suspects = detect(&classified);
        assert_eq!(suspects.len(), 1);
        assert_eq!(suspects[0].position, MissingWavePosition::M0Center);
    }

    #[test]
    fn detect_xc3_only_yields_m2_endpoint() {
        let classified = vec![cmw_with(vec![(
            StructureLabel::XC3,
            Certainty::MissingWaveBundle,
        )])];
        let suspects = detect(&classified);
        assert_eq!(suspects[0].position, MissingWavePosition::M2Endpoint);
    }

    #[test]
    fn detect_bf3_yields_m1_center() {
        let classified = vec![cmw_with(vec![
            (StructureLabel::BF3, Certainty::MissingWaveBundle),
            (StructureLabel::Five, Certainty::MissingWaveBundle),
        ])];
        let suspects = detect(&classified);
        assert_eq!(suspects[0].position, MissingWavePosition::M1Center);
    }
}
