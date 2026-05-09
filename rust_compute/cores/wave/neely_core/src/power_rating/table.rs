// power_rating/table.rs — Neely 書 Power Rating 查表(寫死)
//
// 對齊 m2Spec/oldm2Spec/neely_core.md §十三 / 附錄 B(待 P0 開發補完)。
// 不可外部化(§4.4 / §6.6):查表值寫死 Rust 常數。
//
// **M3 PR-6 階段**(先實踐以後再改):
//   - 提供查表接口(NeelyPatternType + sub_kind → PowerRating)
//   - 對齊 §十三 截斷哲學:邊界外的 case 截斷不外推
//   - 完整書頁查表內容留 PR-6b 對齊 Neely 書附錄 B

#![allow(dead_code)]

use crate::output::{NeelyPatternType, PowerRating};

/// Neely 書 Power Rating 查表(查表值寫死,**不可外部化**)。
///
/// 邊界處理(§十三 截斷哲學):
///   - pattern_type 不在表內 → PowerRating::Neutral(截斷不外推)
pub fn lookup_power_rating(pattern: &NeelyPatternType) -> PowerRating {
    match pattern {
        NeelyPatternType::Impulse => PowerRating::Bullish,
        NeelyPatternType::Diagonal { .. } => PowerRating::SlightBullish,
        NeelyPatternType::Zigzag { .. } => PowerRating::Neutral,
        NeelyPatternType::Flat { .. } => PowerRating::Neutral,
        NeelyPatternType::Triangle { .. } => PowerRating::Neutral,
        NeelyPatternType::Combination { .. } => PowerRating::Neutral,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::*;

    #[test]
    fn impulse_lookup() {
        assert!(matches!(
            lookup_power_rating(&NeelyPatternType::Impulse),
            PowerRating::Bullish
        ));
    }

    #[test]
    fn diagonal_lookup() {
        assert!(matches!(
            lookup_power_rating(&NeelyPatternType::Diagonal {
                sub_kind: DiagonalKind::Leading
            }),
            PowerRating::SlightBullish
        ));
    }

    #[test]
    fn correction_patterns_neutral() {
        for pattern in &[
            NeelyPatternType::Zigzag {
                sub_kind: ZigzagKind::Single,
            },
            NeelyPatternType::Flat {
                sub_kind: FlatKind::Regular,
            },
            NeelyPatternType::Triangle {
                sub_kind: TriangleKind::Contracting,
            },
        ] {
            assert!(matches!(lookup_power_rating(pattern), PowerRating::Neutral));
        }
    }
}
