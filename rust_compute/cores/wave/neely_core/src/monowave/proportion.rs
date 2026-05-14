// Rule of Proportion:算 monowave 的 magnitude / time / 45° metrics
//
// 對齊 m3Spec/neely_core_architecture.md §三 / §四(4.2 Item 1.2)/ §七 Stage 2。
//
// 本模組只計算 metrics(供 Stage 4 Validator R3 等規則消費),
// 不做篩選 / 標註(篩選歸 Validator)。
//
// 45° 參照(Neely 體系內建慣例):
//   - 1 ATR per 1 bar 為 45°(slope = 1.0)
//   - slope > 1.0 → steeper than 45°(快速移動)
//   - slope < 1.0 → shallower than 45°(緩慢移動)
//   - 此 reference 不外部化,寫死(對齊 §4.4)

use crate::output::Monowave;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct ProportionMetrics {
    /// |end_price - start_price|
    pub magnitude: f64,

    /// 持續 bar 數(end_idx - start_idx + 1,end inclusive)
    pub duration_bars: usize,

    /// magnitude / atr_at_start —「N 倍 ATR 規模」;atr_at_start = 0 時為 0.0
    pub atr_relative: f64,

    /// 45° reference:atr_relative / duration_bars
    /// (= 1.0 表示 1 ATR/bar 的 45° 線)
    pub slope_vs_45deg: f64,
}

pub fn compute_proportion_metrics(
    monowave: &Monowave,
    atr_at_start: f64,
    duration_bars: usize,
) -> ProportionMetrics {
    let magnitude = (monowave.end_price - monowave.start_price).abs();
    let atr_relative = if atr_at_start > 0.0 {
        magnitude / atr_at_start
    } else {
        0.0
    };
    let slope_vs_45deg = if duration_bars > 0 {
        atr_relative / duration_bars as f64
    } else {
        0.0
    };

    ProportionMetrics {
        magnitude,
        duration_bars,
        atr_relative,
        slope_vs_45deg,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::MonowaveDirection;
    use chrono::NaiveDate;

    fn mw(start: f64, end: f64) -> Monowave {
        Monowave {
            start_date: NaiveDate::parse_from_str("2026-01-01", "%Y-%m-%d").unwrap(),
            end_date: NaiveDate::parse_from_str("2026-01-05", "%Y-%m-%d").unwrap(),
            start_price: start,
            end_price: end,
            direction: MonowaveDirection::Up,
        }
    }

    #[test]
    fn typical_metrics() {
        // magnitude=4, ATR=2, duration=4
        // → atr_relative = 2.0(2 倍 ATR)
        // → slope = 2.0 / 4 = 0.5(緩於 45°)
        let m = mw(100.0, 104.0);
        let metrics = compute_proportion_metrics(&m, 2.0, 4);
        assert!((metrics.magnitude - 4.0).abs() < 1e-9);
        assert_eq!(metrics.duration_bars, 4);
        assert!((metrics.atr_relative - 2.0).abs() < 1e-9);
        assert!((metrics.slope_vs_45deg - 0.5).abs() < 1e-9);
    }

    #[test]
    fn forty_five_degree_exactly() {
        // 1 ATR per 1 bar → slope = 1.0
        let m = mw(100.0, 105.0);
        let metrics = compute_proportion_metrics(&m, 1.0, 5);
        assert!((metrics.slope_vs_45deg - 1.0).abs() < 1e-9);
    }

    #[test]
    fn steeper_than_45() {
        // 5 ATR per 2 bar → slope = 2.5
        let m = mw(100.0, 110.0);
        let metrics = compute_proportion_metrics(&m, 2.0, 2);
        assert!((metrics.slope_vs_45deg - 2.5).abs() < 1e-9);
    }

    #[test]
    fn zero_atr_yields_zero_metrics() {
        let m = mw(100.0, 105.0);
        let metrics = compute_proportion_metrics(&m, 0.0, 5);
        assert!((metrics.magnitude - 5.0).abs() < 1e-9);
        assert_eq!(metrics.atr_relative, 0.0);
        assert_eq!(metrics.slope_vs_45deg, 0.0);
    }

    #[test]
    fn zero_duration_yields_zero_slope() {
        let m = mw(100.0, 105.0);
        let metrics = compute_proportion_metrics(&m, 1.0, 0);
        assert_eq!(metrics.slope_vs_45deg, 0.0);
    }

    #[test]
    fn descending_monowave_uses_absolute_magnitude() {
        let m = mw(105.0, 100.0);
        let metrics = compute_proportion_metrics(&m, 1.0, 5);
        assert!((metrics.magnitude - 5.0).abs() < 1e-9);
        assert!((metrics.atr_relative - 5.0).abs() < 1e-9);
        assert!((metrics.slope_vs_45deg - 1.0).abs() < 1e-9);
    }
}
