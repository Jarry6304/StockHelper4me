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

pub use projection::{
    compute_expected_fib_zones, project_external_from_w1, project_from_w1, project_internal_from_w1,
    FibProjection,
};
pub use ratios::{FIB_TOLERANCE_PCT, NEELY_FIB_RATIOS, WATERFALL_TOLERANCE_PCT};

use crate::output::{Monowave, Scenario};

/// 對 Forest 中所有 Scenario 套 Fibonacci 投影,寫入 Scenario.expected_fib_zones。
///
/// **Phase 10**:依 scenario.wave_tree.children[0]("W1")日期反查 monowaves
/// 取得 W1 price,投影 Internal + External Fibonacci 區。
pub fn apply_to_forest(forest: &mut [Scenario], monowaves: &[Monowave]) {
    for scenario in forest.iter_mut() {
        scenario.expected_fib_zones = compute_expected_fib_zones(scenario, monowaves);
    }
}
