// fibonacci/projection.rs — Stage 10b:expected_fib_zones 計算
//
// 對齊 m2Spec/oldm2Spec/neely_core.md §三 / §七 Stage 10 / §十四。
//
// **M3 PR-6 階段**(先實踐以後再改):
//   - 從 wave_tree.children[0]("W1")的端點推算 expected_fib_zones
//   - 對 NEELY_FIB_RATIOS 每個比率算對應價格區間(low / high 帶 ±FIB_TOLERANCE_PCT)
//   - PR-6b 校準:更精確的投影(如 W3 從 W2 終點起投 / W4 retracement 各種變形)

use super::ratios::{FIB_TOLERANCE_PCT, NEELY_FIB_RATIOS};
use crate::output::{FibZone, Scenario};

/// 內部用 Fib 投影紀錄(供 caller 進一步處理)。
#[derive(Debug, Clone)]
pub struct FibProjection {
    pub label: String,
    pub ratio: f64,
    pub price: f64,
}

/// 從 Scenario.wave_tree 推算 expected_fib_zones。
///
/// **M3 PR-6 階段**:
///   - 取 wave_tree.children[0]("W1")作為基準波
///   - 但 wave_tree 沒帶 price(只帶 date) — 此版本暫回空 vec(留 PR-6b 接 monowave price)
///
/// **限制**:wave_tree 只有 date 沒有 price,需要從 monowave_series 反查;
/// PR-6 階段 expose 介面,PR-6b 補實作(改傳 monowave_series 進來)。
pub fn compute_expected_fib_zones(_scenario: &Scenario) -> Vec<FibZone> {
    // PR-6b:從 monowave_series 推 W1 price endpoints,套 NEELY_FIB_RATIOS
    Vec::new()
}

/// 給定 W1 端點 price,計算所有 NEELY_FIB_RATIOS 對應的 FibZone。
///
/// 適用範圍:供 PR-6b 接 caller 直接傳 W1 price 用。
pub fn project_from_w1(w1_start: f64, w1_end: f64) -> Vec<FibZone> {
    let w1_magnitude = (w1_end - w1_start).abs();
    if w1_magnitude < f64::EPSILON {
        return Vec::new();
    }
    let direction_sign = if w1_end > w1_start { 1.0 } else { -1.0 };

    NEELY_FIB_RATIOS
        .iter()
        .map(|&ratio| {
            let price = w1_end + direction_sign * w1_magnitude * (ratio - 0.0);
            // 帶 ±tolerance% 區間
            let half_tol = price * FIB_TOLERANCE_PCT / 100.0 / 2.0;
            FibZone {
                label: format!("Fib {:.1}%", ratio * 100.0),
                low: price.min(price - half_tol).min(price + half_tol),
                high: price.max(price - half_tol).max(price + half_tol),
                source_ratio: ratio,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_w1_yields_no_zones() {
        // W1 magnitude 為 0 → 回空
        assert!(project_from_w1(100.0, 100.0).is_empty());
    }

    #[test]
    fn project_count_matches_ratios() {
        let zones = project_from_w1(100.0, 110.0);
        assert_eq!(zones.len(), NEELY_FIB_RATIOS.len());
    }

    #[test]
    fn project_38_2_pct_extension_position() {
        // W1 100→110 上升;從 W1 終點 110 起,Fib 38.2% extension
        // = 110 + 10 × 0.382 = 113.82
        let zones = project_from_w1(100.0, 110.0);
        let z = zones.iter().find(|z| (z.source_ratio - 0.382).abs() < 1e-9).unwrap();
        let center = (z.low + z.high) / 2.0;
        assert!((center - 113.82).abs() < 0.5, "center={}", center);
    }

    #[test]
    fn project_descending_w1() {
        // W1 100→90 下降;從 W1 終點 90 起,Fib 0.5 extension
        // = 90 + (-1) × 10 × 0.5 = 85
        let zones = project_from_w1(100.0, 90.0);
        let z = zones.iter().find(|z| (z.source_ratio - 0.500).abs() < 1e-9).unwrap();
        let center = (z.low + z.high) / 2.0;
        assert!((center - 85.0).abs() < 0.5);
    }
}
