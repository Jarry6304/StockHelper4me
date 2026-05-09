// fibonacci — Stage 10b:Fibonacci 投影
//
// 對齊 m2Spec/oldm2Spec/neely_core.md §三 / §七 Stage 10 / §十四。
// 子模組:
//   - ratios.rs     — 比率清單(38.2%、61.8%、100%、161.8% 等)寫死(§4.4)
//   - projection.rs — expected_fib_zones 計算
//
// 設計原則:
//   - **Fibonacci 不獨立成 Core**(§十,cores_overview §十)— 屬 Neely 內部子模組
//   - 比率清單與 ±4% 容差寫死,**不可外部化**(§4.4)
//
// **M3 PR-6 階段**(先實踐以後再改):
//   - ratios.rs:寫死 Neely 體系標準 Fibonacci 比率
//   - projection.rs:從 W1 magnitude 投影 expected_fib_zones(供 W2 / W3 / W4 終點預期)
//   - apply_to_forest():對 Forest 中所有 Scenario 套 Fibonacci 投影

pub mod projection;
pub mod ratios;

pub use projection::{compute_expected_fib_zones, FibProjection};
pub use ratios::{NEELY_FIB_RATIOS, FIB_TOLERANCE_PCT};

use crate::output::Scenario;

/// 對 Forest 中所有 Scenario 套 Fibonacci 投影,寫入 Scenario.expected_fib_zones。
///
/// **M3 PR-6 階段**:用 wave_tree.children[0] 的 start/end 作為「W1 端點」基準,
/// 投影出常用 Fibonacci 比率對應的價格區間(38.2% / 61.8% / 100% / 161.8%)。
pub fn apply_to_forest(forest: &mut [Scenario]) {
    for scenario in forest.iter_mut() {
        scenario.expected_fib_zones = compute_expected_fib_zones(scenario);
    }
}
