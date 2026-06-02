// rules.rs — v3 硬規則的「幾何半」(端點價格判定),供 patterns/* groupers 共用。
// 對齊 traditional_rules.md §A;從 v1 validator.rs salvage。children index 0-based:
// motive 5-window = [浪1,浪2,浪3,浪4,浪5];corrective 3-window = [A,B,C]。
// **orthodox 端點紀律**:一律讀 node.start_price/end_price,不讀 intrabar 極值。

use crate::node::EngineNode;

/// 方向是否嚴格交替(impulse/zigzag/flat/diagonal/triangle 的結構前提)。
pub fn alternates(w: &[EngineNode]) -> bool {
    w.windows(2).all(|p| p[0].direction != p[1].direction)
}

/// 視窗淨方向是否向上(以首節點起點 → 末節點終點)。
pub fn window_up(w: &[EngineNode]) -> bool {
    w.last().map(|l| l.end_price).unwrap_or(0.0) >= w[0].start_price
}

/// R1:浪 2 回撤 ≤ 浪 1 之 100%。
pub fn r1_ok(w: &[EngineNode]) -> bool {
    w[1].amp() <= w[0].amp()
}

/// R3:浪 3 端點超越浪 1 端點(方向感知)。
pub fn r3_ok(w: &[EngineNode], up: bool) -> bool {
    if up {
        w[2].end_price > w[0].end_price
    } else {
        w[2].end_price < w[0].end_price
    }
}

/// 浪 3 在 1/3/5 中最短(百分比)。
pub fn wave3_shortest(w: &[EngineNode]) -> bool {
    w[2].pct() < w[0].pct() && w[2].pct() < w[4].pct()
}

/// R4:浪 3 永不最短。
pub fn r4_ok(w: &[EngineNode]) -> bool {
    !wave3_shortest(w)
}

/// R5(衝擊浪)浪 4 不重疊浪 1(方向感知;up: 浪4 端點 > 浪1 端點)。
pub fn r5_no_overlap(w: &[EngineNode], up: bool) -> bool {
    if up {
        w[3].end_price > w[0].end_price
    } else {
        w[3].end_price < w[0].end_price
    }
}

/// R9(對角)反向子浪不完全回撤前行動子浪(strict)+ 浪 3 永不最短。
pub fn r9_ok(w: &[EngineNode]) -> bool {
    w[1].amp() < w[0].amp() && w[3].amp() < w[2].amp() && !wave3_shortest(w)
}
