// max_retracement.rs — Ch10 Power Rating × 最大回測限制查表
//
// 對齊 m3Spec/neely_rules.md §第 10 章 Pattern Implications & Power Ratings(2016-2022 行)
//       + m3Spec/neely_core_architecture.md §9.1 / §11.4
//
// **回測限制表**(Neely 精華版 Ch10):
//   - Power 0          → 任意回測(`None`)
//   - ±1               → ≤ 約 90%
//   - ±2               → ≤ 約 80%
//   - ±3               → ≤ 約 60–70%(取中值 65%)
//
// **覆蓋規則**(spec line 2021 + §11.4 line 1286):
//   - 形態在 Contracting Triangle 內部 → `None`(Power 暗示不傳遞)
//   - 形態在 Terminal Impulse 內部 → `None`(同上)
//
// **設計理由**(architecture §11):
//   - Power Rating 是「精華版 Ch10 寫死的查表值」,non-subjective
//   - max_retracement 由 Power Rating 直接決定,不引入額外主觀因子

use crate::output::PowerRating;

/// 依 PowerRating + in_triangle_context 查最大回測比例。
///
/// 回傳:
/// - `None`:任意回測(Neutral 或 Triangle/Terminal 內覆蓋)
/// - `Some(ratio)`:回測比例上限(0.0..1.0,例:0.90 = 90%)
pub fn lookup(rating: PowerRating, in_triangle_context: bool) -> Option<f64> {
    // Triangle/Terminal 內部覆蓋規則(spec §11.4 / Ch10 line 2021)
    if in_triangle_context {
        return None;
    }
    match rating {
        PowerRating::Neutral => None,
        // ±1 → ≤ 約 90%
        PowerRating::SlightBullish | PowerRating::SlightBearish => Some(0.90),
        // ±2 → ≤ 約 80%
        PowerRating::Bullish | PowerRating::Bearish => Some(0.80),
        // ±3 → ≤ 約 60–70%(取中值)
        PowerRating::StrongBullish | PowerRating::StrongBearish => Some(0.65),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neutral_returns_none() {
        assert_eq!(lookup(PowerRating::Neutral, false), None);
    }

    #[test]
    fn slight_returns_90_pct() {
        assert_eq!(lookup(PowerRating::SlightBullish, false), Some(0.90));
        assert_eq!(lookup(PowerRating::SlightBearish, false), Some(0.90));
    }

    #[test]
    fn moderate_returns_80_pct() {
        assert_eq!(lookup(PowerRating::Bullish, false), Some(0.80));
        assert_eq!(lookup(PowerRating::Bearish, false), Some(0.80));
    }

    #[test]
    fn strong_returns_65_pct() {
        assert_eq!(lookup(PowerRating::StrongBullish, false), Some(0.65));
        assert_eq!(lookup(PowerRating::StrongBearish, false), Some(0.65));
    }

    #[test]
    fn triangle_override_returns_none_regardless_of_rating() {
        for r in [
            PowerRating::Neutral,
            PowerRating::SlightBullish,
            PowerRating::Bullish,
            PowerRating::StrongBullish,
            PowerRating::SlightBearish,
            PowerRating::Bearish,
            PowerRating::StrongBearish,
        ] {
            assert_eq!(lookup(r, true), None, "in_triangle should override for {:?}", r);
        }
    }

    #[test]
    fn bullish_bearish_symmetry() {
        assert_eq!(lookup(PowerRating::SlightBullish, false), lookup(PowerRating::SlightBearish, false));
        assert_eq!(lookup(PowerRating::Bullish, false), lookup(PowerRating::Bearish, false));
        assert_eq!(lookup(PowerRating::StrongBullish, false), lookup(PowerRating::StrongBearish, false));
    }
}
