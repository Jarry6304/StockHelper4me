// fibonacci/projection.rs — Stage 10b:expected_fib_zones 計算
//
// 對齊 m3Spec/neely_core_architecture.md §7.1 Stage 10b + §4.5 容差規範
//       + m3Spec/neely_rules.md §Ch12 Fibonacci Relationships
//
// **Phase 10 PR(r5 alignment)— Internal / External Fibonacci 分離 + 接 monowave price**
//
//   Spec §Ch12 Fibonacci 分兩類:
//     - Internal Fibonacci(retracement 內部回測): 0.236 / 0.382 / 0.500 / 0.618 / 0.786
//       用途:W2 retracement of W1、W4 retracement of W3、b retracement of a 等
//       formula:`price = source_end - direction_sign × source_magnitude × ratio`
//     - External Fibonacci(extension 外部延伸): 1.000 / 1.272 / 1.618 / 2.000 / 2.618
//       用途:W3 / W5 extension of W1 等
//       formula:`price = source_end + direction_sign × source_magnitude × (ratio - 1.0)`
//                (從 source_end 開始,延伸 source_magnitude × (ratio - 1.0))
//
//   ±4% 容差(FIB_TOLERANCE_PCT)— architecture §4.5 / §6.6 寫死,不可外部化。
//   Waterfall Effect ±5%(WATERFALL_TOLERANCE_PCT)— Ch12 特例,
//   留 P11+ Reverse Logic 偵測時啟用。

use super::ratios::{FIB_TOLERANCE_PCT, NEELY_FIB_RATIOS};
use crate::output::{FibZone, Monowave, Scenario};

/// 內部用 Fib 投影紀錄(供 caller 進一步處理)。
#[derive(Debug, Clone)]
pub struct FibProjection {
    pub label: String,
    pub ratio: f64,
    pub price: f64,
}

/// 從 Scenario.wave_tree 推算 expected_fib_zones(Internal + External 都含)。
///
/// **Phase 10**:依 scenario.wave_tree.children[0]("W1")的日期反查 monowave_series
/// 取得 W1 start_price / end_price,套 internal + external 兩組投影。
///
/// 若 wave_tree 沒 children 或對應 monowave 找不到 → 回空 vec。
pub fn compute_expected_fib_zones(scenario: &Scenario, monowaves: &[Monowave]) -> Vec<FibZone> {
    let Some(w1_node) = scenario.wave_tree.children.first() else {
        return Vec::new();
    };
    let Some(w1_mw) = monowaves
        .iter()
        .find(|m| m.start_date == w1_node.start && m.end_date == w1_node.end)
    else {
        return Vec::new();
    };
    let mut zones = project_internal_from_w1(w1_mw.start_price, w1_mw.end_price);
    zones.extend(project_external_from_w1(w1_mw.start_price, w1_mw.end_price));
    zones
}

/// Internal Fibonacci(retracement)— W2 / W4 / b 回測 W1 等 source 波的目標區。
///
/// 公式:`price = source_end - direction_sign × source_magnitude × ratio`,
/// ratio ∈ {0.236, 0.382, 0.5, 0.618, 0.786}。
pub fn project_internal_from_w1(w1_start: f64, w1_end: f64) -> Vec<FibZone> {
    let magnitude = (w1_end - w1_start).abs();
    if magnitude < f64::EPSILON {
        return Vec::new();
    }
    let direction_sign = if w1_end > w1_start { 1.0 } else { -1.0 };
    NEELY_FIB_RATIOS
        .iter()
        .filter(|&&r| r < 1.0)
        .map(|&ratio| {
            let price = w1_end - direction_sign * magnitude * ratio;
            zone_from_price(format!("Internal {:.1}%", ratio * 100.0), ratio, price)
        })
        .collect()
}

/// External Fibonacci(extension)— W3 / W5 延伸 W1 等 source 波的目標區。
///
/// 公式:`price = source_end + direction_sign × source_magnitude × (ratio - 1.0)`,
/// ratio ∈ {1.0, 1.272, 1.618, 2.0, 2.618}。
///
/// ratio = 1.0 對應「equal length」(price = source_end,代表延伸後到達 source_end +
/// 0 = source_end;但 Neely Fibonacci 上下文中 ratio=1.0 通常指「再走一個 source_magnitude」),
/// 這裡採後者語意,price = source_end + direction_sign × magnitude × 1.0 = source_end + magnitude。
pub fn project_external_from_w1(w1_start: f64, w1_end: f64) -> Vec<FibZone> {
    let magnitude = (w1_end - w1_start).abs();
    if magnitude < f64::EPSILON {
        return Vec::new();
    }
    let direction_sign = if w1_end > w1_start { 1.0 } else { -1.0 };
    NEELY_FIB_RATIOS
        .iter()
        .filter(|&&r| r >= 1.0)
        .map(|&ratio| {
            let price = w1_end + direction_sign * magnitude * ratio;
            zone_from_price(format!("External {:.1}%", ratio * 100.0), ratio, price)
        })
        .collect()
}

fn zone_from_price(label: String, ratio: f64, price: f64) -> FibZone {
    let half_tol = price.abs() * FIB_TOLERANCE_PCT / 100.0 / 2.0;
    FibZone {
        label,
        low: price - half_tol,
        high: price + half_tol,
        source_ratio: ratio,
    }
}

/// 給定 W1 端點 price,計算所有 NEELY_FIB_RATIOS 對應的 FibZone(Internal + External 合併)。
///
/// 保留以維持向後相容 — 新 caller 應改用 `project_internal_from_w1` /
/// `project_external_from_w1` 並依需求選 internal / external。
pub fn project_from_w1(w1_start: f64, w1_end: f64) -> Vec<FibZone> {
    let mut zones = project_internal_from_w1(w1_start, w1_end);
    zones.extend(project_external_from_w1(w1_start, w1_end));
    zones
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_w1_yields_no_zones() {
        // W1 magnitude 為 0 → 回空
        assert!(project_from_w1(100.0, 100.0).is_empty());
        assert!(project_internal_from_w1(100.0, 100.0).is_empty());
        assert!(project_external_from_w1(100.0, 100.0).is_empty());
    }

    #[test]
    fn internal_count_matches_retracement_ratios() {
        // NEELY_FIB_RATIOS 中 < 1.0 的:0.236 / 0.382 / 0.5 / 0.618 / 0.786 = 5
        let zones = project_internal_from_w1(100.0, 110.0);
        assert_eq!(zones.len(), 5);
        assert!(zones.iter().all(|z| z.source_ratio < 1.0));
        assert!(zones.iter().all(|z| z.label.starts_with("Internal")));
    }

    #[test]
    fn external_count_matches_extension_ratios() {
        // NEELY_FIB_RATIOS 中 >= 1.0 的:1.0 / 1.272 / 1.618 / 2.0 / 2.618 = 5
        let zones = project_external_from_w1(100.0, 110.0);
        assert_eq!(zones.len(), 5);
        assert!(zones.iter().all(|z| z.source_ratio >= 1.0));
        assert!(zones.iter().all(|z| z.label.starts_with("External")));
    }

    #[test]
    fn internal_61_8_pct_retrace_up_wave() {
        // W1 100→110 上升;61.8% 回測 = 110 - 10×0.618 = 103.82
        let zones = project_internal_from_w1(100.0, 110.0);
        let z = zones
            .iter()
            .find(|z| (z.source_ratio - 0.618).abs() < 1e-9)
            .unwrap();
        let center = (z.low + z.high) / 2.0;
        assert!((center - 103.82).abs() < 0.5, "center={}", center);
    }

    #[test]
    fn external_161_8_pct_extension_up_wave() {
        // W1 100→110 上升;161.8% 延伸 = 110 + 10×1.618 = 126.18
        let zones = project_external_from_w1(100.0, 110.0);
        let z = zones
            .iter()
            .find(|z| (z.source_ratio - 1.618).abs() < 1e-9)
            .unwrap();
        let center = (z.low + z.high) / 2.0;
        assert!((center - 126.18).abs() < 0.5, "center={}", center);
    }

    #[test]
    fn external_extension_descending_w1() {
        // W1 100→90 下降;161.8% extension = 90 - 10×1.618 = 73.82
        let zones = project_external_from_w1(100.0, 90.0);
        let z = zones
            .iter()
            .find(|z| (z.source_ratio - 1.618).abs() < 1e-9)
            .unwrap();
        let center = (z.low + z.high) / 2.0;
        assert!((center - 73.82).abs() < 0.5);
    }

    #[test]
    fn internal_retrace_descending_w1() {
        // W1 100→90 下降;38.2% 回測 = 90 + 10×0.382 = 93.82
        let zones = project_internal_from_w1(100.0, 90.0);
        let z = zones
            .iter()
            .find(|z| (z.source_ratio - 0.382).abs() < 1e-9)
            .unwrap();
        let center = (z.low + z.high) / 2.0;
        assert!((center - 93.82).abs() < 0.5);
    }
}
