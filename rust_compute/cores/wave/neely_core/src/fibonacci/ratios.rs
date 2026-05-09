// fibonacci/ratios.rs — Neely 體系 Fibonacci 比率清單(寫死)
//
// 對齊 m2Spec/oldm2Spec/neely_core.md §4.4 / §十四 / §6.6。
// **不可外部化**:比率清單與容差寫死 Rust 常數。
//
// 來源:Neely 原書(《Mastering Elliott Wave》,Glenn Neely 1990)。
// 各比率對應「波浪間預期比例」(W2 retracement of W1 / W3 extension of W1 etc)。

/// Neely 體系標準 Fibonacci 比率(寫死)。
///
/// 含 Retracement(< 1.0)+ Extension(>= 1.0)。
/// 0.382 / 0.618 為最常用 retracement;1.0 / 1.618 / 2.618 為常用 extension。
pub const NEELY_FIB_RATIOS: &[f64] = &[
    0.236, // 23.6% retracement(較淺)
    0.382, // 38.2% retracement(常用)
    0.500, // 50% retracement(實務常見,雖非嚴格 Fib)
    0.618, // 61.8% golden ratio(最重要)
    0.786, // 78.6% retracement(深 retracement)
    1.000, // 100% extension(equal)
    1.272, // 127.2% extension
    1.618, // 161.8% extension(golden 延伸)
    2.000, // 200% extension
    2.618, // 261.8% extension
];

/// Neely 規則的 ±4% 相對容差(§10.4 容差規範,寫死)。
/// 例:W3 ≥ W1 × 0.96(相對 -4%)
pub const FIB_TOLERANCE_PCT: f64 = 4.0;

/// Waterfall Effect ±5% 例外(§10.4,Neely 書裡特例,寫死)。
pub const WATERFALL_TOLERANCE_PCT: f64 = 5.0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ratios_count_and_order() {
        assert_eq!(NEELY_FIB_RATIOS.len(), 10);
        // 升序排列,方便 caller binary search
        let sorted: Vec<f64> = {
            let mut v = NEELY_FIB_RATIOS.to_vec();
            v.sort_by(|a, b| a.partial_cmp(b).unwrap());
            v
        };
        assert_eq!(NEELY_FIB_RATIOS, sorted.as_slice());
    }

    #[test]
    fn key_ratios_present() {
        assert!(NEELY_FIB_RATIOS.contains(&0.382));
        assert!(NEELY_FIB_RATIOS.contains(&0.618));
        assert!(NEELY_FIB_RATIOS.contains(&1.0));
        assert!(NEELY_FIB_RATIOS.contains(&1.618));
    }

    #[test]
    fn tolerance_constants() {
        assert_eq!(FIB_TOLERANCE_PCT, 4.0);
        assert_eq!(WATERFALL_TOLERANCE_PCT, 5.0);
    }
}
