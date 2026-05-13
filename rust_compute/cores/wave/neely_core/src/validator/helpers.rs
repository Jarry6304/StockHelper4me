// validator/helpers.rs — Wave-level rule 共用 helper(PR-3c-1 落地)
//
// 對齊 m3Spec/neely_core_architecture.md §4.2 容差規範:
//   - ±4% Fibonacci 標準容差(寫死)
//   - ±5% Triangle leg equality(寫死)
//   - ±10% 一般規則容差(寫死)
//
// 這些常數不可外部化(§4.5 / §6.6)。

use crate::monowave::ClassifiedMonowave;

/// Fibonacci 標準容差(§4.2 寫死)
pub const FIB_TOLERANCE_PCT: f64 = 4.0;

/// Neely 標準 Fibonacci 比率(§Ch12,5 個 standard ratios)
pub const NEELY_FIB_RATIOS_PCT: &[f64] = &[38.2, 61.8, 100.0, 161.8, 261.8];

/// Monowave magnitude(取 metrics.magnitude;對齊 ClassifiedMonowave 既有 field)
pub fn magnitude(c: &ClassifiedMonowave) -> f64 {
    c.metrics.magnitude
}

/// 安全百分比計算:numerator / denominator × 100。denom ≈ 0 → None。
pub fn safe_pct(numerator: f64, denominator: f64) -> Option<f64> {
    if denominator.abs() < 1e-9 {
        None
    } else {
        Some(numerator / denominator * 100.0)
    }
}

/// 檢查 value 是否在 target ± tolerance_pct% 範圍內。
/// 例:within_tolerance(105.0, 100.0, 4.0) → 105 在 96-104 範圍?否,回 false
pub fn within_tolerance(value: f64, target: f64, tolerance_pct: f64) -> bool {
    (value - target).abs() <= tolerance_pct
}

/// 檢查 value 是否匹配任一 Neely 標準 Fibonacci 比率(±4%)。
pub fn matches_any_fib_ratio(pct_value: f64) -> bool {
    NEELY_FIB_RATIOS_PCT
        .iter()
        .any(|&fib| within_tolerance(pct_value, fib, FIB_TOLERANCE_PCT))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_pct_handles_zero_denom() {
        assert_eq!(safe_pct(10.0, 0.0), None);
        assert!(safe_pct(10.0, 1e-10).is_none());
    }

    #[test]
    fn safe_pct_basic() {
        let r = safe_pct(50.0, 100.0).unwrap();
        assert!((r - 50.0).abs() < 1e-9);
    }

    #[test]
    fn within_tolerance_basic() {
        assert!(within_tolerance(61.0, 61.8, 4.0));
        assert!(within_tolerance(65.0, 61.8, 4.0));
        assert!(!within_tolerance(70.0, 61.8, 4.0));
    }

    #[test]
    fn matches_fib_38_2() {
        assert!(matches_any_fib_ratio(38.2));
        assert!(matches_any_fib_ratio(40.0)); // within 4% of 38.2
        assert!(matches_any_fib_ratio(36.0)); // within 4% of 38.2(差 2.2)
    }

    #[test]
    fn matches_fib_61_8() {
        assert!(matches_any_fib_ratio(61.8));
        assert!(matches_any_fib_ratio(58.0)); // 差 3.8,within 4%
        assert!(matches_any_fib_ratio(65.5)); // 差 3.7,within 4%
    }

    #[test]
    fn matches_fib_100() {
        assert!(matches_any_fib_ratio(100.0));
        assert!(matches_any_fib_ratio(96.0));
        assert!(matches_any_fib_ratio(103.5));
        assert!(!matches_any_fib_ratio(80.0)); // 80 不在任何 ratio ±4 內(38.2 / 61.8 都太遠)
    }

    #[test]
    fn matches_fib_161_8() {
        assert!(matches_any_fib_ratio(161.8));
        assert!(matches_any_fib_ratio(158.0));
        assert!(matches_any_fib_ratio(165.0));
    }

    #[test]
    fn no_match_outside_all_ratios() {
        // 50% 距離 38.2 太遠 + 距離 61.8 太遠
        assert!(!matches_any_fib_ratio(50.0));
        assert!(!matches_any_fib_ratio(80.0));
        assert!(!matches_any_fib_ratio(130.0));
    }
}
