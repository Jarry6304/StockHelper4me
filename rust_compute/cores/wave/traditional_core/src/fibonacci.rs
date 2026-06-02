// fibonacci.rs — Stage 6:Fib 投影(回撤 / 交匯)
//
// 對齊 traditional_rules.md L16–25 + references/engine.md:
//   - **永不淘汰**(形態優先,比例為輔);僅產出 `expected_fib_zones`(供 LLM / 視覺)。
//   - 一律從**正統終點(orthodox terminal = 最後一個 pivot)**量起。
//   - v1 產出結構淨移動的回撤區(.382 / .5 / .618),方向恆指向原點側 → 不會投影錯方向。

use crate::candidates::Candidate;
use crate::output::FibZone;

const RETRACE_RATIOS: [f64; 3] = [0.382, 0.5, 0.618];

pub fn project(c: &Candidate, root_label: &str, fib_tol: f64) -> Vec<FibZone> {
    let p = &c.pivots;
    let last = p.len() - 1;
    let origin = p[0].price;
    let terminal = p[last].price;
    let mv = terminal - origin; // 帶號淨移動
    if mv.abs() < 1e-9 {
        return Vec::new();
    }
    let band = (fib_tol * mv.abs()).max(1e-6);
    RETRACE_RATIOS
        .iter()
        .map(|&r| {
            let center = terminal - r * mv; // 自正統終點回撤向原點
            FibZone {
                label: format!("{root_label} retrace {:.3}", r),
                low: center - band,
                high: center + band,
                source_ratio: r,
            }
        })
        .collect()
}
