// candidates.rs — Candidate / HypoKind adapter 型別。
//
// v3 後:`scenario.rs` 把每個 top `EngineNode` 包成 `Candidate`,重用既有
// `guidelines::evaluate` / `fibonacci::project` / `triggers::build`(三者簽章吃 `&Candidate`,
// 只用 pivot.price + direction)。v1 的 `generate()`(扁平單度數窮舉)已被 `compaction` 取代並移除。

use crate::output::{Direction, Pivot};

/// 粗形態假設(guidelines 依此選 motive / corrective / triangle 指引集)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HypoKind {
    Impulse,
    Diagonal,
    Zigzag,
    Flat,
    Triangle,
}

/// 形態的 pivot 端點序 + 形態假設 + 方向(scenario.rs 從 EngineNode 構造)。
#[derive(Debug, Clone)]
pub struct Candidate {
    pub id: String,
    /// motive/triangle 6 個端點 / zigzag/flat 4 個端點
    pub pivots: Vec<Pivot>,
    pub hypo: HypoKind,
    pub direction: Direction,
}
