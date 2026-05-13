// power_rating/table.rs — Neely Ch10 Power Rating 7 級完整查表(PR-6b-1)
//
// 對齊 m3Spec/neely_rules.md Ch10 line 2006-2022(7 級表格 + 回測限制 + Triangle/Terminal override)。
// 不可外部化(§4.5 / §6.6):查表值寫死 Rust 常數。
//
// **PR-6b-1 階段(2026-05-13)**:7 級完整實作 + Max Retracement 對映。
//
// 神奇映射(spec §9.2 方向中性語意:相對於前一段趨勢方向解釋):
//   - 完成向上 +/ 向下 -(符號由 initial_direction 決定)
//   - 「Favor Continuation」= 順趨勢 / 「Against Continuation」= 逆趨勢
//
// neely_rules.md Ch10 表(line 2006-2014):
//   ±3 → Triple Zigzag / Triple Combination / Triple Flat / Running 系列
//   ±2 → Double Zigzag / Double Flat / Irregular Failure / Triple Three
//   ±1 → Elongated Zigzag / Elongated Flat / C-Failure / Irregular(三角內 = 0)
//    0 → Normal Zigzag / Flat Common / B-Failure / Impulse / TerminalImpulse
//
// 回測限制:0 任意 / ±1 ~90% / ±2 ~80% / ±3 ~60-70%

#![allow(dead_code)]

use crate::output::{
    CombinationKind, FlatVariant, MonowaveDirection, NeelyPatternType, PowerRating,
    ZigzagVariant,
};
#[cfg(test)]
use crate::output::TriangleVariant;

/// Max retracement(相對 scenario 整段)
/// 0 → 任意(1.0)/ ±1 → 0.90 / ±2 → 0.80 / ±3 → 0.65(60-70% 中位)
pub const MAX_RETRACE_LEVEL_0: f64 = 1.0;
pub const MAX_RETRACE_LEVEL_1: f64 = 0.90;
pub const MAX_RETRACE_LEVEL_2: f64 = 0.80;
pub const MAX_RETRACE_LEVEL_3: f64 = 0.65;

/// 計算 PowerRating 對應的 max retracement 比例(0.0-1.0)
pub fn max_retracement_for(rating: PowerRating) -> f64 {
    match rating {
        PowerRating::Neutral => MAX_RETRACE_LEVEL_0,
        PowerRating::SlightlyFavorContinuation | PowerRating::SlightlyAgainstContinuation => {
            MAX_RETRACE_LEVEL_1
        }
        PowerRating::ModeratelyFavorContinuation | PowerRating::ModeratelyAgainstContinuation => {
            MAX_RETRACE_LEVEL_2
        }
        PowerRating::StronglyFavorContinuation | PowerRating::StronglyAgainstContinuation => {
            MAX_RETRACE_LEVEL_3
        }
    }
}

/// Neely Ch10 Power Rating 查表(查表值寫死)。
///
/// 邊界處理(§11 截斷哲學):
///   - 不在表內 → Neutral(截斷不外推)
///
/// 方向(direction)決定符號:
///   - Up(順趨勢方向)→ FavorContinuation 側
///   - Down(逆趨勢方向)→ AgainstContinuation 側
pub fn lookup_power_rating(pattern: &NeelyPatternType, direction: MonowaveDirection) -> PowerRating {
    // base_strength 是表內 0-3 級的 magnitude;sign 由 direction 決定
    let (base_strength, is_against): (u8, bool) = match pattern {
        // Impulse / TerminalImpulse: 0 (隨後續趨勢延伸)
        NeelyPatternType::Impulse => (0, false),
        // Terminal 內部段不傳遞 Power(Ch10 line 2021),但 Terminal 自己是 0(暫定)
        NeelyPatternType::TerminalImpulse => (0, false),

        // RunningCorrection top-level variant: ±3 against(Ch10 line 2014)
        NeelyPatternType::RunningCorrection => (3, true),

        // Zigzag 子變體
        NeelyPatternType::Zigzag { sub_kind } => match sub_kind {
            ZigzagVariant::Normal => (0, false),
            ZigzagVariant::Truncated => (0, false),
            ZigzagVariant::Elongated => (1, false), // line 2010
        },

        // Flat 子變體
        NeelyPatternType::Flat { sub_kind } => match sub_kind {
            FlatVariant::Common => (0, false),
            FlatVariant::BFailure => (0, false),
            // C-Failure / Irregular:±1 against
            FlatVariant::CFailure => (1, true),
            FlatVariant::Irregular => (1, true),
            // Irregular Failure:±2 against
            FlatVariant::IrregularFailure => (2, true),
            // Elongated Flat:±1 favor(罕見,接 trend continuation)
            FlatVariant::Elongated => (1, false),
            // Double Failure:±1 against(罕見,best-guess)
            FlatVariant::DoubleFailure => (1, true),
        },

        // Triangle:內部 override = 0(Ch10 line 2021)
        NeelyPatternType::Triangle { .. } => (0, false),

        // Combination 子變體
        NeelyPatternType::Combination { sub_kinds } => {
            // DoubleThree → ±2;TripleThree → ±3(line 2008-2013)
            let any_triple = sub_kinds.iter().any(|k| matches!(k, CombinationKind::TripleThree));
            if any_triple {
                (3, false)
            } else {
                (2, false)
            }
        }
    };

    rating_from_strength_and_direction(base_strength, is_against, direction)
}

/// 由(base_strength, is_against, direction)組合決定 PowerRating enum:
///   - direction Up + !is_against → FavorContinuation 側
///   - direction Up + is_against → AgainstContinuation 側
///   - direction Down → 翻轉(順逆 spec rule 對齊 line 2006「向下則反號」)
///   - Neutral direction → 視為 Up(對 0-strength 結果一樣)
fn rating_from_strength_and_direction(
    base_strength: u8,
    is_against: bool,
    direction: MonowaveDirection,
) -> PowerRating {
    if base_strength == 0 {
        return PowerRating::Neutral;
    }
    // Down 趨勢翻轉 is_against(對齊 line 2006「向下則反號」)
    let final_against = match direction {
        MonowaveDirection::Down => !is_against,
        _ => is_against,
    };
    match (base_strength, final_against) {
        (1, false) => PowerRating::SlightlyFavorContinuation,
        (1, true) => PowerRating::SlightlyAgainstContinuation,
        (2, false) => PowerRating::ModeratelyFavorContinuation,
        (2, true) => PowerRating::ModeratelyAgainstContinuation,
        (3, false) => PowerRating::StronglyFavorContinuation,
        (3, true) => PowerRating::StronglyAgainstContinuation,
        _ => PowerRating::Neutral,
    }
}

/// 三角形/Terminal 內部 override(line 2021):若 scenario 是 nested 在 Triangle/Terminal
/// 內部段 → power_rating 強制 = 0。
///
/// PR-6b-1 階段:此函式提供 API,但實際 nested context tracking 留 PR-5b
/// (Three Rounds Compaction 提供 parent context)。
pub fn apply_triangle_terminal_override(
    rating: PowerRating,
    inside_triangle_or_terminal: bool,
) -> PowerRating {
    if inside_triangle_or_terminal {
        PowerRating::Neutral
    } else {
        rating
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn impulse_zero_rating() {
        assert!(matches!(
            lookup_power_rating(&NeelyPatternType::Impulse, MonowaveDirection::Up),
            PowerRating::Neutral
        ));
    }

    #[test]
    fn terminal_zero_rating() {
        assert!(matches!(
            lookup_power_rating(&NeelyPatternType::TerminalImpulse, MonowaveDirection::Up),
            PowerRating::Neutral
        ));
    }

    #[test]
    fn running_correction_strongly_against() {
        // RunningCorrection completing Up → StronglyAgainstContinuation(逆趨勢)
        assert!(matches!(
            lookup_power_rating(&NeelyPatternType::RunningCorrection, MonowaveDirection::Up),
            PowerRating::StronglyAgainstContinuation
        ));
    }

    #[test]
    fn running_correction_down_flips_to_favor() {
        // Down 翻轉(line 2006 「向下則反號」)
        assert!(matches!(
            lookup_power_rating(&NeelyPatternType::RunningCorrection, MonowaveDirection::Down),
            PowerRating::StronglyFavorContinuation
        ));
    }

    #[test]
    fn zigzag_normal_neutral() {
        assert!(matches!(
            lookup_power_rating(
                &NeelyPatternType::Zigzag { sub_kind: ZigzagVariant::Normal },
                MonowaveDirection::Up,
            ),
            PowerRating::Neutral
        ));
    }

    #[test]
    fn zigzag_elongated_slightly_favor() {
        assert!(matches!(
            lookup_power_rating(
                &NeelyPatternType::Zigzag { sub_kind: ZigzagVariant::Elongated },
                MonowaveDirection::Up,
            ),
            PowerRating::SlightlyFavorContinuation
        ));
    }

    #[test]
    fn flat_common_neutral() {
        assert!(matches!(
            lookup_power_rating(
                &NeelyPatternType::Flat { sub_kind: FlatVariant::Common },
                MonowaveDirection::Up,
            ),
            PowerRating::Neutral
        ));
    }

    #[test]
    fn flat_cfailure_slightly_against() {
        assert!(matches!(
            lookup_power_rating(
                &NeelyPatternType::Flat { sub_kind: FlatVariant::CFailure },
                MonowaveDirection::Up,
            ),
            PowerRating::SlightlyAgainstContinuation
        ));
    }

    #[test]
    fn flat_irregular_failure_moderately_against() {
        assert!(matches!(
            lookup_power_rating(
                &NeelyPatternType::Flat { sub_kind: FlatVariant::IrregularFailure },
                MonowaveDirection::Up,
            ),
            PowerRating::ModeratelyAgainstContinuation
        ));
    }

    #[test]
    fn triangle_all_neutral_internal_override() {
        // Triangle 內部 override = 0(Ch10 line 2021)
        for variant in [
            TriangleVariant::HorizontalLimiting,
            TriangleVariant::IrregularLimiting,
            TriangleVariant::RunningLimiting,
            TriangleVariant::HorizontalExpanding,
        ] {
            assert!(matches!(
                lookup_power_rating(
                    &NeelyPatternType::Triangle { sub_kind: variant },
                    MonowaveDirection::Up,
                ),
                PowerRating::Neutral
            ));
        }
    }

    #[test]
    fn combination_double_three_moderately_favor() {
        assert!(matches!(
            lookup_power_rating(
                &NeelyPatternType::Combination {
                    sub_kinds: vec![CombinationKind::DoubleThree],
                },
                MonowaveDirection::Up,
            ),
            PowerRating::ModeratelyFavorContinuation
        ));
    }

    #[test]
    fn combination_triple_three_strongly_favor() {
        assert!(matches!(
            lookup_power_rating(
                &NeelyPatternType::Combination {
                    sub_kinds: vec![CombinationKind::TripleThree],
                },
                MonowaveDirection::Up,
            ),
            PowerRating::StronglyFavorContinuation
        ));
    }

    #[test]
    fn max_retracement_level_mapping() {
        assert_eq!(max_retracement_for(PowerRating::Neutral), 1.0);
        assert_eq!(max_retracement_for(PowerRating::SlightlyFavorContinuation), 0.90);
        assert_eq!(max_retracement_for(PowerRating::ModeratelyAgainstContinuation), 0.80);
        assert_eq!(max_retracement_for(PowerRating::StronglyAgainstContinuation), 0.65);
    }

    #[test]
    fn triangle_terminal_override_function() {
        // Inside Triangle/Terminal → 強制 Neutral
        assert!(matches!(
            apply_triangle_terminal_override(PowerRating::StronglyFavorContinuation, true),
            PowerRating::Neutral
        ));
        // Outside → 不動
        assert!(matches!(
            apply_triangle_terminal_override(PowerRating::StronglyFavorContinuation, false),
            PowerRating::StronglyFavorContinuation
        ));
    }
}
