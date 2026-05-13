// fibonacci — Stage 10b:Fibonacci 投影 + per-pattern alignment(PR-6b-2 完整實作)
//
// 對齊 m3Spec/neely_core_architecture.md §9.5 + neely_rules.md Ch12 line 2533-2557。
// 子模組:
//   - ratios.rs     — 5 standard ratios + ±4% 容差(spec r5)
//   - projection.rs — per-pattern fibonacci_alignment 計算 + W1 投影
//
// 設計原則:
//   - **Fibonacci 不獨立成 Core**(cores_overview §十)— 屬 Neely 內部子模組
//   - 比率清單與 ±4% 容差寫死(§4.5)
//
// **PR-6b-2 階段(2026-05-13)**:
//   - 從 r2 10 ratios 砍至 5 standard(對齊 spec r5)
//   - 加 per-pattern fibonacci_alignment 計算 — Impulse/Zigzag/Flat/Triangle
//   - 寫入 Scenario.structural_facts.fibonacci_alignment(StructuralFacts 8 子欄位)

pub mod projection;
pub mod ratios;

pub use projection::{compute_expected_fib_zones, compute_internal_alignment, FibProjection};
pub use ratios::{
    FIB_TOLERANCE_PCT, NEELY_FIB_RATIOS, NEELY_FIB_RATIOS_PCT, TRIANGLE_LEG_EQ_TOLERANCE_PCT,
    WATERFALL_TOLERANCE_PCT,
};

use crate::monowave::ClassifiedMonowave;
use crate::output::Scenario;

/// 對 Forest 中所有 Scenario 套 Fibonacci 投影 + alignment 計算。
///
/// **PR-6b-2 階段**:
///   - 寫入 expected_fib_zones(目前空,完整需 monowave price endpoints)
///   - 寫入 structural_facts.fibonacci_alignment(per-pattern matched ratios)
pub fn apply_to_forest(forest: &mut [Scenario], classified: &[ClassifiedMonowave]) {
    for scenario in forest.iter_mut() {
        scenario.expected_fib_zones = compute_expected_fib_zones(scenario);
        scenario.structural_facts.fibonacci_alignment =
            compute_internal_alignment(scenario, classified);
    }
}

/// Backward-compat:不傳 classified 的版本(只算 expected_fib_zones,
/// fibonacci_alignment 留空)。用於 caller 還沒 thread monowave through。
#[deprecated(note = "use apply_to_forest(forest, classified) for complete alignment")]
pub fn apply_to_forest_legacy(forest: &mut [Scenario]) {
    for scenario in forest.iter_mut() {
        scenario.expected_fib_zones = compute_expected_fib_zones(scenario);
    }
}
