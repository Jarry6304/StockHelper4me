// power_rating/table.rs — Neely 書 Power Rating 查表(寫死)
//
// 對齊 m3Spec/neely_rules.md Ch10 p.10-1~5(7 級完整查表)。
// 不可外部化(§4.5 / §6.6):查表值寫死 Rust 常數。
//
// **PR-3c-pre 階段(2026-05-13)**:
//   - r2 Bullish/Bearish 改 FavorContinuation/AgainstContinuation(§9.2 方向中性)
//   - r2 Diagonal → TerminalImpulse(§9.6 取代 Prechter 派術語)
//   - 完整 Ch10 查表內容留 PR-6b-1 對齊 neely_rules.md Ch10 行 2006-2022

#![allow(dead_code)]

use crate::output::{NeelyPatternType, PowerRating};

/// Neely 書 Power Rating 查表(查表值寫死,**不可外部化**)。
///
/// 邊界處理(§11 截斷哲學):
///   - pattern_type 不在表內 → PowerRating::Neutral(截斷不外推)
pub fn lookup_power_rating(pattern: &NeelyPatternType) -> PowerRating {
    match pattern {
        NeelyPatternType::Impulse => PowerRating::ModeratelyFavorContinuation,
        NeelyPatternType::TerminalImpulse => PowerRating::SlightlyFavorContinuation,
        NeelyPatternType::RunningCorrection => PowerRating::StronglyAgainstContinuation,
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
            PowerRating::ModeratelyFavorContinuation
        ));
    }

    #[test]
    fn terminal_impulse_lookup() {
        assert!(matches!(
            lookup_power_rating(&NeelyPatternType::TerminalImpulse),
            PowerRating::SlightlyFavorContinuation
        ));
    }

    #[test]
    fn correction_patterns_neutral() {
        for pattern in &[
            NeelyPatternType::Zigzag { sub_kind: ZigzagVariant::Normal },
            NeelyPatternType::Flat { sub_kind: FlatVariant::Common },
            NeelyPatternType::Triangle { sub_kind: TriangleVariant::HorizontalLimiting },
        ] {
            assert!(matches!(lookup_power_rating(pattern), PowerRating::Neutral));
        }
    }

    #[test]
    fn running_correction_against() {
        assert!(matches!(
            lookup_power_rating(&NeelyPatternType::RunningCorrection),
            PowerRating::StronglyAgainstContinuation
        ));
    }
}
