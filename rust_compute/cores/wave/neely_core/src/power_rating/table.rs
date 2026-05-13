// power_rating/table.rs — Neely 書 Power Rating 完整查表(r5,寫死)
//
// 對齊 m3Spec/neely_rules.md §Pattern Implications & Power Ratings(2004-2022 行)
//       + m3Spec/neely_core_architecture.md §11(Power Rating 截斷哲學)
//
// **r5 Power Rating 完整表**(spec 2006-2014 行):
//   | Power | 形態 |
//   |---:|---|
//   | +3 / -3 | Triple Zigzag, Triple Combination, Triple Flat |
//   | +2 / -2 | Double Zigzag, Double Combination, Double Flat |
//   | +1 / -1 | Elongated Zigzag, Elongated Flat(三角內 = 0) |
//   | 0       | Zigzag, B-Failure, Common Flat |
//   | -1 / +1 | C-Failure(三角內 = 0), Irregular(三角內 = 0) |
//   | -2 / +2 | Irregular Failure(三角內 = 0), Double Three, Triple Three |
//   | -3 / +3 | Running Correction, Double Three Running, Triple Three Running |
//
// **方向**:正號 = 延續趨勢方向;負號 = 反轉傾向。實際 Bullish/Bearish 由 initial_direction 決定:
//   - Up direction + 正號 → Bullish 系列
//   - Up direction + 負號 → Bearish 系列
//   - Down direction → 符號全反
//
// **不可外部化**(architecture §4.5 / §6.6):查表值寫死,不可從 toml 設定。

use crate::output::{
    CombinationKind, DiagonalKind, FlatKind, MonowaveDirection, NeelyPatternType,
    PowerRating, ZigzagKind,
};

/// 「Signed Power」= -3..=+3,代表 spec 表內的數字
type SignedPower = i8;

/// 由 spec power table 查找 signed power(±3..0..∓3 範圍)。
///
/// `in_triangle` = scenario 是否為較大 Triangle 內部 leg(spec 2021 行例外:Triangle 內任一規則例外 = 0)。
/// Phase 5 暫無 parent context → 傳 false;留 P6/P8 Compaction 補完整 nested 偵測。
fn signed_power_lookup(pattern: &NeelyPatternType, in_triangle: bool) -> SignedPower {
    if in_triangle {
        // Spec 2021:三角內所有規則例外 → power = 0
        return 0;
    }
    match pattern {
        // Impulse(Trending)→ 強勁延續趨勢,Power = +3
        NeelyPatternType::Impulse => 3,

        // Diagonal(Terminal Impulse)→ Leading 開新段(+1),Ending 收尾(-1)
        NeelyPatternType::Diagonal {
            sub_kind: DiagonalKind::Leading,
        } => 1,
        NeelyPatternType::Diagonal {
            sub_kind: DiagonalKind::Ending,
        } => -1,

        // Zigzag sub_kinds(spec 1487-1498 + 2008-2010)
        NeelyPatternType::Zigzag {
            sub_kind: ZigzagKind::Single,
        } => 0, // Common Zigzag
        NeelyPatternType::Zigzag {
            sub_kind: ZigzagKind::Double,
        } => 2, // Double Zigzag
        NeelyPatternType::Zigzag {
            sub_kind: ZigzagKind::Triple,
        } => 3, // Triple Zigzag

        // Flat sub_kinds(spec 1425-1475 + 2010-2014)
        NeelyPatternType::Flat {
            sub_kind: FlatKind::Regular,
        } => 0, // Common Flat
        NeelyPatternType::Flat {
            sub_kind: FlatKind::Expanded,
        } => 1, // Elongated Flat(三角內 = 0,由 in_triangle handled)
        NeelyPatternType::Flat {
            sub_kind: FlatKind::Running,
        } => -3, // Running Correction

        // Triangle:無強勁方向暗示,Power = 0
        NeelyPatternType::Triangle { .. } => 0,

        // Combination(Non-Standard with x-wave,spec 2008-2014)
        NeelyPatternType::Combination { sub_kinds } => {
            // 計 sub_kinds 中 DoubleThree / TripleThree 數量,取最 dominant
            let has_triple = sub_kinds
                .iter()
                .any(|k| matches!(k, CombinationKind::TripleThree));
            if has_triple {
                -3 // Triple Three / Triple Three Running
            } else {
                -2 // Double Three
            }
        }
    }
}

/// 將 signed power 轉成方向感知的 PowerRating enum(architecture §9.4)。
///
/// Up direction:正號 → Bullish 系列;負號 → Bearish 系列
/// Down direction:全反
pub fn lookup_power_rating(
    pattern: &NeelyPatternType,
    direction: MonowaveDirection,
    in_triangle: bool,
) -> PowerRating {
    let raw = signed_power_lookup(pattern, in_triangle);
    let signed = match direction {
        MonowaveDirection::Up => raw,
        MonowaveDirection::Down => -raw,
        MonowaveDirection::Neutral => 0,
    };
    match signed {
        3 => PowerRating::StrongBullish,
        2 => PowerRating::Bullish,
        1 => PowerRating::SlightBullish,
        0 => PowerRating::Neutral,
        -1 => PowerRating::SlightBearish,
        -2 => PowerRating::Bearish,
        -3 => PowerRating::StrongBearish,
        _ => PowerRating::Neutral, // 截斷哲學(architecture §11)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::*;

    #[test]
    fn impulse_up_strong_bullish() {
        let p = lookup_power_rating(
            &NeelyPatternType::Impulse,
            MonowaveDirection::Up,
            false,
        );
        assert!(matches!(p, PowerRating::StrongBullish));
    }

    #[test]
    fn impulse_down_strong_bearish() {
        let p = lookup_power_rating(
            &NeelyPatternType::Impulse,
            MonowaveDirection::Down,
            false,
        );
        assert!(matches!(p, PowerRating::StrongBearish));
    }

    #[test]
    fn diagonal_leading_up_slight_bullish() {
        let p = lookup_power_rating(
            &NeelyPatternType::Diagonal {
                sub_kind: DiagonalKind::Leading,
            },
            MonowaveDirection::Up,
            false,
        );
        assert!(matches!(p, PowerRating::SlightBullish));
    }

    #[test]
    fn diagonal_ending_up_slight_bearish() {
        let p = lookup_power_rating(
            &NeelyPatternType::Diagonal {
                sub_kind: DiagonalKind::Ending,
            },
            MonowaveDirection::Up,
            false,
        );
        assert!(matches!(p, PowerRating::SlightBearish));
    }

    #[test]
    fn double_zigzag_up_bullish() {
        let p = lookup_power_rating(
            &NeelyPatternType::Zigzag {
                sub_kind: ZigzagKind::Double,
            },
            MonowaveDirection::Up,
            false,
        );
        assert!(matches!(p, PowerRating::Bullish));
    }

    #[test]
    fn triple_zigzag_up_strong_bullish() {
        let p = lookup_power_rating(
            &NeelyPatternType::Zigzag {
                sub_kind: ZigzagKind::Triple,
            },
            MonowaveDirection::Up,
            false,
        );
        assert!(matches!(p, PowerRating::StrongBullish));
    }

    #[test]
    fn running_flat_up_strong_bearish() {
        // Running Correction = -3 spec → Up direction → StrongBearish
        let p = lookup_power_rating(
            &NeelyPatternType::Flat {
                sub_kind: FlatKind::Running,
            },
            MonowaveDirection::Up,
            false,
        );
        assert!(matches!(p, PowerRating::StrongBearish));
    }

    #[test]
    fn double_three_combination_up_bearish() {
        let p = lookup_power_rating(
            &NeelyPatternType::Combination {
                sub_kinds: vec![CombinationKind::DoubleThree],
            },
            MonowaveDirection::Up,
            false,
        );
        assert!(matches!(p, PowerRating::Bearish));
    }

    #[test]
    fn elongated_flat_in_triangle_neutral() {
        // 三角內 = 0 例外(spec 2021)
        let p = lookup_power_rating(
            &NeelyPatternType::Flat {
                sub_kind: FlatKind::Expanded,
            },
            MonowaveDirection::Up,
            true, // in_triangle
        );
        assert!(matches!(p, PowerRating::Neutral));
    }

    #[test]
    fn triangle_neutral() {
        let p = lookup_power_rating(
            &NeelyPatternType::Triangle {
                sub_kind: TriangleKind::Contracting,
            },
            MonowaveDirection::Up,
            false,
        );
        assert!(matches!(p, PowerRating::Neutral));
    }

    #[test]
    fn neutral_direction_yields_neutral_rating() {
        let p = lookup_power_rating(
            &NeelyPatternType::Impulse,
            MonowaveDirection::Neutral,
            false,
        );
        assert!(matches!(p, PowerRating::Neutral));
    }
}
