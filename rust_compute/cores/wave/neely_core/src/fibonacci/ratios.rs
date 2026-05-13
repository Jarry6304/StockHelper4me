// fibonacci/ratios.rs — Neely Ch12 5 個標準 Fibonacci 比率(PR-6b-2 落地)
//
// 對齊 m3Spec/neely_core_architecture.md §4.2 + neely_rules.md Ch12 line 2533-2557。
// **不可外部化**(§4.5):比率清單與容差寫死 Rust 常數。
//
// **PR-6b-2 階段(2026-05-13)**:從 r2 10 ratios 砍至 spec r5 5 個 standard ratios。
// 非 Neely 體系的(0.236 / 0.5 / 0.786 / 1.272 / 2.0)移除 — Neely Ch12 沒列。
//
// 來源:Neely 原書 《Mastering Elliott Wave》 Ch12 Advanced Fibonacci Extensions。

/// Neely Ch12 5 個標準 Fibonacci 比率(spec r5 §4.2 + neely_rules.md Ch12 line 2533-2557)。
/// 含 Retracement(< 1.0)+ Extension(≥ 1.0)。
pub const NEELY_FIB_RATIOS: &[f64] = &[
    0.382, // 38.2% retracement
    0.618, // 61.8% golden ratio
    1.000, // 100% equality
    1.618, // 161.8% golden extension
    2.618, // 261.8% extreme extension
];

/// 百分比版本(用於規則內 ratio 計算)。
pub const NEELY_FIB_RATIOS_PCT: &[f64] = &[38.2, 61.8, 100.0, 161.8, 261.8];

/// Neely 規則的 ±4% 相對容差(§4.2 寫死)。
pub const FIB_TOLERANCE_PCT: f64 = 4.0;

/// Waterfall Effect ±5% 例外(§4.2,Neely 書裡特例,寫死)。
pub const WATERFALL_TOLERANCE_PCT: f64 = 5.0;

/// Triangle leg equality 容差 ±5%(§4.2)。
pub const TRIANGLE_LEG_EQ_TOLERANCE_PCT: f64 = 5.0;

/// 檢查 `actual_pct` 是否匹配任一 Neely 標準 Fibonacci 比率(±4% 容差)。
/// 回傳第一個匹配的比率(%),否則 None。
pub fn match_fib_ratio(actual_pct: f64) -> Option<f64> {
    NEELY_FIB_RATIOS_PCT
        .iter()
        .find(|&&fib| (actual_pct - fib).abs() <= FIB_TOLERANCE_PCT)
        .copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ratios_count_5_per_spec() {
        // r5 spec §4.2 / Ch12 line 2533-2557:5 個 standard ratios
        assert_eq!(NEELY_FIB_RATIOS.len(), 5);
        assert_eq!(NEELY_FIB_RATIOS_PCT.len(), 5);
    }

    #[test]
    fn ratios_in_ascending_order() {
        let sorted: Vec<f64> = {
            let mut v = NEELY_FIB_RATIOS.to_vec();
            v.sort_by(|a, b| a.partial_cmp(b).unwrap());
            v
        };
        assert_eq!(NEELY_FIB_RATIOS, sorted.as_slice());
    }

    #[test]
    fn five_standard_ratios_present() {
        assert!(NEELY_FIB_RATIOS.contains(&0.382));
        assert!(NEELY_FIB_RATIOS.contains(&0.618));
        assert!(NEELY_FIB_RATIOS.contains(&1.0));
        assert!(NEELY_FIB_RATIOS.contains(&1.618));
        assert!(NEELY_FIB_RATIOS.contains(&2.618));
    }

    #[test]
    fn non_neely_ratios_removed() {
        // r2 -> r5 砍掉 0.236 / 0.5 / 0.786 / 1.272 / 2.0(非 Neely)
        assert!(!NEELY_FIB_RATIOS.contains(&0.236));
        assert!(!NEELY_FIB_RATIOS.contains(&0.5));
        assert!(!NEELY_FIB_RATIOS.contains(&0.786));
        assert!(!NEELY_FIB_RATIOS.contains(&1.272));
        assert!(!NEELY_FIB_RATIOS.contains(&2.0));
    }

    #[test]
    fn tolerance_constants() {
        assert_eq!(FIB_TOLERANCE_PCT, 4.0);
        assert_eq!(WATERFALL_TOLERANCE_PCT, 5.0);
        assert_eq!(TRIANGLE_LEG_EQ_TOLERANCE_PCT, 5.0);
    }

    #[test]
    fn match_fib_ratio_exact() {
        assert_eq!(match_fib_ratio(38.2), Some(38.2));
        assert_eq!(match_fib_ratio(61.8), Some(61.8));
        assert_eq!(match_fib_ratio(161.8), Some(161.8));
    }

    #[test]
    fn match_fib_ratio_within_tolerance() {
        assert_eq!(match_fib_ratio(40.0), Some(38.2)); // 38.2 + 1.8 < 4
        assert_eq!(match_fib_ratio(65.0), Some(61.8)); // 61.8 + 3.2 < 4
        assert_eq!(match_fib_ratio(99.0), Some(100.0));
    }

    #[test]
    fn match_fib_ratio_no_match() {
        assert_eq!(match_fib_ratio(50.0), None);  // 50 不在任何 ±4 內
        assert_eq!(match_fib_ratio(130.0), None);
        assert_eq!(match_fib_ratio(200.0), None);
    }
}
