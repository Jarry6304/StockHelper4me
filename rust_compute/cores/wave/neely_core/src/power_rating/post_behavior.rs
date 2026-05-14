// post_behavior.rs — PostBehavior lookup table
//
// 對齊 m3Spec/neely_rules.md §第 10 章 line 2024–2037「各修正暗示重點」
//       + m3Spec/neely_core_architecture.md §9.2 PostBehavior 8-variant enum
//       + §11.4 Triangle/Terminal 內部覆蓋規則(line 2021)
//
// **設計**:
//   - 純查表(pattern_type × in_triangle → PostBehavior)
//   - 不引入機率語意 / 不加權(對齊 architecture §2.2)
//   - Triangle 內部覆蓋為 Unconstrained(對齊 max_retracement 同款覆蓋規則)
//   - PowerRating 從 pattern_type 推導,本 fn 不直接吃 PowerRating
//     (PowerRating 是 lookup_power_rating 的結果,PostBehavior 與其 1:1 對齊)

use crate::output::{
    CombinationKind, DiagonalKind, FlatKind, NeelyPatternType, PostBehavior, ZigzagKind,
};

/// 由 pattern_type + in_triangle_context 查 PostBehavior。
///
/// 對齊 spec line 2024–2037 列的 14 種「各修正暗示重點」+ line 2017–2022
/// Power Rating × 回測限制聯動表。
///
/// Triangle 內部覆蓋規則(spec 2021):in_triangle = true → Unconstrained
/// (與 max_retracement::lookup 同款覆蓋設計)。
pub fn lookup(pattern: &NeelyPatternType, in_triangle_context: bool) -> PostBehavior {
    if in_triangle_context {
        // Triangle 內部 Power 暗示不傳遞 → 後續任意
        return PostBehavior::Unconstrained;
    }
    match pattern {
        // Impulse(±3 Strong):不會被完全回測,除非為更大級的 5/c
        NeelyPatternType::Impulse => PostBehavior::NotFullyRetracedUnless {
            exception: "更大級的 5/c".to_string(),
        },

        // Diagonal:Leading(±1 Slight)→ MinRetracement 90%
        //          Ending(Terminal 等價,±1 Slight Against)→ 必完全回測
        NeelyPatternType::Diagonal { sub_kind } => match sub_kind {
            DiagonalKind::Leading => PostBehavior::MinRetracement { ratio: 0.90 },
            DiagonalKind::Ending => PostBehavior::FullRetracementRequired,
        },

        // Zigzag:Single(0 Neutral)→ Unconstrained
        //        Double / Triple(±2 / ±3)→ 不會被完全回測,除非 5th Terminal Extended 最後段
        NeelyPatternType::Zigzag { sub_kind } => match sub_kind {
            ZigzagKind::Single => PostBehavior::Unconstrained,
            ZigzagKind::Double | ZigzagKind::Triple => PostBehavior::NotFullyRetracedUnless {
                exception: "5th Terminal Extended 最後段".to_string(),
            },
        },

        // Flat 7 variants(Phase 16 r5 落地,對齊 spec line 2030-2033):
        //   - Common / BFailure(0 Neutral 「B-Failure 最中性」 spec 2030)→ Unconstrained
        //   - CFailure(-1 SlightAgainst)→ 必完全回測 + 後續 Impulse 大於前一(spec 2032)
        //   - DoubleFailure(-2 ModerateAgainst)→ Composite:必完全回測 + 後續 ≥ 161.8%
        //   - Irregular(-1 SlightAgainst,三角內 = 0)→ MinRetracement 90%
        //     (Irregular 罕見,常為自我矛盾,後續多為 Triangle/Terminal — 給 MinRetracement)
        //   - IrregularFailure(-2 ModerateAgainst)→ Composite:必完全回測 + 後續 ≥ 161.8%(spec 2033)
        //   - Elongated(±1, 三角內 = 0)→ MinRetracement 90%
        NeelyPatternType::Flat { sub_kind } => match sub_kind {
            FlatKind::Common | FlatKind::BFailure => PostBehavior::Unconstrained,
            FlatKind::CFailure => PostBehavior::FullRetracementRequired,
            FlatKind::DoubleFailure | FlatKind::IrregularFailure => PostBehavior::Composite {
                behaviors: vec![
                    PostBehavior::FullRetracementRequired,
                    PostBehavior::NextImpulseExceeds { ratio: 1.618 },
                ],
            },
            FlatKind::Irregular | FlatKind::Elongated => PostBehavior::MinRetracement { ratio: 0.90 },
        },

        // Triangle:作為「整體形態」(不在內部段)時,後續 Thrust 必達 wave-D 區
        //          (spec 2041 Contracting Thrust 必超過三角最高/最低價;
        //           spec 2047 Expanding Thrust 小於最寬段;此處不細分,用 ReachesWaveZone 共通)
        NeelyPatternType::Triangle { .. } => PostBehavior::ReachesWaveZone {
            wave: crate::output::WaveNumber::WD,
        },

        // Combination:多以 Triangle 結尾 / 不可被完全回測(spec 2027)
        //   - Triple* variants(±3)→ NextImpulseExceeds 2.618(spec 2037 Triple Three Running 後續 ≥ 261.8%)
        //   - Double* variants(±2)→ NextImpulseExceeds 1.618(spec 2034 Double Three 後續 ≥ 161.8%)
        NeelyPatternType::Combination { sub_kinds } => {
            if sub_kinds.iter().any(|k| matches!(
                k,
                CombinationKind::TripleZigzag
                    | CombinationKind::TripleCombination
                    | CombinationKind::TripleThree
                    | CombinationKind::TripleThreeCombination
                    | CombinationKind::TripleThreeRunning
            )) {
                PostBehavior::NextImpulseExceeds { ratio: 2.618 }
            } else {
                PostBehavior::NextImpulseExceeds { ratio: 1.618 }
            }
        }

        // RunningCorrection(Phase 16 r5 上提頂層,spec 2035):
        //   後續必為延伸 Impulse 或 Flat/Zigzag 延伸 c-wave;後續 Impulse 多 > 161.8%(常達 261.8%)
        NeelyPatternType::RunningCorrection => {
            PostBehavior::NextImpulseExceeds { ratio: 1.618 }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::{CombinationKind, DiagonalKind, FlatKind, TriangleKind, ZigzagKind};

    #[test]
    fn impulse_not_fully_retraced_unless() {
        let pb = lookup(&NeelyPatternType::Impulse, false);
        matches!(pb, PostBehavior::NotFullyRetracedUnless { .. });
    }

    #[test]
    fn diagonal_leading_min_retracement_90() {
        let pb = lookup(
            &NeelyPatternType::Diagonal {
                sub_kind: DiagonalKind::Leading,
            },
            false,
        );
        match pb {
            PostBehavior::MinRetracement { ratio } => assert_eq!(ratio, 0.90),
            other => panic!("expected MinRetracement(0.90), got {:?}", other),
        }
    }

    #[test]
    fn diagonal_ending_full_retracement() {
        let pb = lookup(
            &NeelyPatternType::Diagonal {
                sub_kind: DiagonalKind::Ending,
            },
            false,
        );
        assert!(matches!(pb, PostBehavior::FullRetracementRequired));
    }

    #[test]
    fn zigzag_single_unconstrained() {
        let pb = lookup(
            &NeelyPatternType::Zigzag {
                sub_kind: ZigzagKind::Single,
            },
            false,
        );
        assert!(matches!(pb, PostBehavior::Unconstrained));
    }

    #[test]
    fn zigzag_double_not_fully_retraced() {
        let pb = lookup(
            &NeelyPatternType::Zigzag {
                sub_kind: ZigzagKind::Double,
            },
            false,
        );
        assert!(matches!(pb, PostBehavior::NotFullyRetracedUnless { .. }));
    }

    #[test]
    fn flat_common_unconstrained() {
        let pb = lookup(
            &NeelyPatternType::Flat {
                sub_kind: FlatKind::Common,
            },
            false,
        );
        assert!(matches!(pb, PostBehavior::Unconstrained));
    }

    #[test]
    fn flat_b_failure_unconstrained() {
        // B-Failure 最中性(spec 2030)→ Unconstrained
        let pb = lookup(
            &NeelyPatternType::Flat {
                sub_kind: FlatKind::BFailure,
            },
            false,
        );
        assert!(matches!(pb, PostBehavior::Unconstrained));
    }

    #[test]
    fn flat_c_failure_full_retracement() {
        let pb = lookup(
            &NeelyPatternType::Flat {
                sub_kind: FlatKind::CFailure,
            },
            false,
        );
        assert!(matches!(pb, PostBehavior::FullRetracementRequired));
    }

    #[test]
    fn flat_irregular_failure_composite() {
        let pb = lookup(
            &NeelyPatternType::Flat {
                sub_kind: FlatKind::IrregularFailure,
            },
            false,
        );
        match pb {
            PostBehavior::Composite { behaviors } => {
                assert_eq!(behaviors.len(), 2);
                assert!(matches!(behaviors[0], PostBehavior::FullRetracementRequired));
                assert!(matches!(
                    behaviors[1],
                    PostBehavior::NextImpulseExceeds { ratio } if (ratio - 1.618).abs() < 1e-9
                ));
            }
            other => panic!("expected Composite, got {:?}", other),
        }
    }

    #[test]
    fn flat_irregular_min_retracement_90() {
        let pb = lookup(
            &NeelyPatternType::Flat {
                sub_kind: FlatKind::Irregular,
            },
            false,
        );
        match pb {
            PostBehavior::MinRetracement { ratio } => assert_eq!(ratio, 0.90),
            other => panic!("expected MinRetracement(0.90), got {:?}", other),
        }
    }

    #[test]
    fn running_correction_next_impulse_1618() {
        // Phase 16 r5:RunningCorrection 上提頂層,後續 ≥ 161.8%(spec 2035)
        let pb = lookup(&NeelyPatternType::RunningCorrection, false);
        match pb {
            PostBehavior::NextImpulseExceeds { ratio } => assert!((ratio - 1.618).abs() < 1e-9),
            other => panic!("expected NextImpulseExceeds(1.618), got {:?}", other),
        }
    }

    #[test]
    fn combination_double_next_impulse_1618() {
        let pb = lookup(
            &NeelyPatternType::Combination {
                sub_kinds: vec![CombinationKind::DoubleZigzag],
            },
            false,
        );
        match pb {
            PostBehavior::NextImpulseExceeds { ratio } => assert!((ratio - 1.618).abs() < 1e-9),
            other => panic!("expected NextImpulseExceeds(1.618), got {:?}", other),
        }
    }

    #[test]
    fn combination_triple_next_impulse_2618() {
        let pb = lookup(
            &NeelyPatternType::Combination {
                sub_kinds: vec![CombinationKind::TripleThreeRunning],
            },
            false,
        );
        match pb {
            PostBehavior::NextImpulseExceeds { ratio } => assert!((ratio - 2.618).abs() < 1e-9),
            other => panic!("expected NextImpulseExceeds(2.618), got {:?}", other),
        }
    }

    #[test]
    fn triangle_reaches_wave_d_zone() {
        let pb = lookup(
            &NeelyPatternType::Triangle {
                sub_kind: TriangleKind::Contracting,
            },
            false,
        );
        match pb {
            PostBehavior::ReachesWaveZone { wave } => {
                assert_eq!(wave, crate::output::WaveNumber::WD)
            }
            other => panic!("expected ReachesWaveZone(WD), got {:?}", other),
        }
    }

    #[test]
    fn in_triangle_context_overrides_to_unconstrained() {
        // Triangle 內部覆蓋:任何 pattern 都變 Unconstrained
        for pattern in [
            NeelyPatternType::Impulse,
            NeelyPatternType::Flat {
                sub_kind: FlatKind::CFailure,
            },
            NeelyPatternType::Flat {
                sub_kind: FlatKind::IrregularFailure,
            },
            NeelyPatternType::RunningCorrection,
            NeelyPatternType::Combination {
                sub_kinds: vec![CombinationKind::TripleThreeRunning],
            },
        ] {
            let pb = lookup(&pattern, true);
            assert!(
                matches!(pb, PostBehavior::Unconstrained),
                "in_triangle override failed for {:?}",
                pattern
            );
        }
    }
}
