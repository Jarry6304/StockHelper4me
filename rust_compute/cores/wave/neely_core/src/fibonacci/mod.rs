// fibonacci — Stage 10b:Fibonacci 投影
//
// 對齊 m3Spec/neely_core_architecture.md §7.1 Stage 10b + §4.5 容差規範
//       + m3Spec/neely_rules.md §Ch12 Fibonacci Relationships
//
// 子模組:
//   - ratios.rs     — 比率清單(0.236, 0.382, 0.5, 0.618, 0.786, 1.0, 1.272,
//                     1.618, 2.0, 2.618)+ ±4% 容差寫死(architecture §4.5)
//   - projection.rs — Internal(retracement)+ External(extension)分離投影
//
// 設計原則:
//   - **Fibonacci 不獨立成 Core**(cores_overview §十)— 屬 Neely 內部子模組
//   - 比率清單與 ±4% 容差寫死,**不可外部化**(architecture §4.5 / §6.6)
//
// **Phase 10 PR(r5 alignment)**:
//   - Internal / External Fibonacci 分離(對齊 Ch12)
//   - apply_to_forest 接 monowave_series,反查 W1 prices 後實際投影
//   - 取代 Phase 6 階段 compute_expected_fib_zones 回空 vec 的 placeholder

pub mod projection;
pub mod ratios;
/// v4.2 P1.2 #11:Waterfall Effect ±5% 偵測(原 projection.rs:19 註解 deferred P11+,本 PR 落地)
pub mod waterfall;

pub use projection::{
    compute_expected_fib_zones, project_external_from_w1, project_from_w1, project_internal_from_w1,
    FibProjection,
};
pub use ratios::{FIB_TOLERANCE_PCT, NEELY_FIB_RATIOS, WATERFALL_TOLERANCE_PCT};

use crate::output::{FibZone, Monowave, Scenario};

/// 對 Forest 中所有 Scenario 套 Fibonacci 投影,寫入 Scenario.expected_fib_zones。
///
/// **Phase 10**:依 scenario.wave_tree.children[0]("W1")日期反查 monowaves
/// 取得 W1 price,投影 Internal + External Fibonacci 區。
pub fn apply_to_forest(forest: &mut [Scenario], monowaves: &[Monowave]) {
    for scenario in forest.iter_mut() {
        scenario.expected_fib_zones = compute_expected_fib_zones(scenario, monowaves);
    }
}

/// 中點距離 < 此比例的兩個 FibZone 視為同一價位(去重門檻)。
const FIB_ZONE_DEDUP_PCT: f64 = 0.003;

/// 去重一組 FibZone:依中點升序排序,丟掉與前一個保留項中點距離 < 0.3% 的近重複。
fn dedup_fib_zones(mut zones: Vec<FibZone>) -> Vec<FibZone> {
    zones.retain(|z| z.low.is_finite() && z.high.is_finite() && z.low > 0.0);
    zones.sort_by(|a, b| {
        let ma = (a.low + a.high) / 2.0;
        let mb = (b.low + b.high) / 2.0;
        ma.partial_cmp(&mb).unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut out: Vec<FibZone> = Vec::new();
    for z in zones {
        let mid = (z.low + z.high) / 2.0;
        let dup = out.last().map_or(false, |prev| {
            let pmid = (prev.low + prev.high) / 2.0;
            pmid > 0.0 && ((mid - pmid).abs() / pmid) < FIB_ZONE_DEDUP_PCT
        });
        if !dup {
            out.push(z);
        }
    }
    out
}

/// Fusion Layer P1.1:全 forest scenario 的 `expected_fib_zones` 去重聯集。
///
/// 對齊 m3Spec/fusion_layer.md §6 #1。寫進 `NeelyCoreOutput.flat_fib_zones`,
/// 供 Fusion `key_levels` 模組直接讀(不必重跑 Neely)。
pub fn flatten_fib_zones(forest: &[Scenario]) -> Vec<FibZone> {
    let all: Vec<FibZone> = forest
        .iter()
        .flat_map(|s| s.expected_fib_zones.iter().cloned())
        .collect();
    dedup_fib_zones(all)
}

#[cfg(test)]
mod flat_fib_tests {
    use super::dedup_fib_zones;
    use crate::output::FibZone;

    fn z(low: f64, high: f64, ratio: f64) -> FibZone {
        FibZone {
            label: format!("fib_{ratio}"),
            low,
            high,
            source_ratio: ratio,
        }
    }

    #[test]
    fn dedup_collapses_near_duplicate_zones() {
        // 三個中點都落 ~100 → 收斂成 1
        let out = dedup_fib_zones(vec![
            z(99.0, 101.0, 0.618),
            z(99.1, 101.1, 0.618),
            z(99.05, 101.05, 0.5),
        ]);
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn dedup_keeps_distinct_levels() {
        // 中點 100 vs 120 差 20% → 兩個都留
        let out = dedup_fib_zones(vec![z(99.0, 101.0, 0.618), z(119.0, 121.0, 1.0)]);
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn dedup_drops_non_finite_and_sorts_by_midpoint() {
        let out = dedup_fib_zones(vec![
            z(119.0, 121.0, 1.0),
            z(f64::NAN, 101.0, 0.5),
            z(49.0, 51.0, 0.382),
        ]);
        assert_eq!(out.len(), 2);
        assert!(out[0].low < out[1].low);
    }
}
