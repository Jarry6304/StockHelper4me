// guidelines.rs — Stage 5:客觀指引 + 限定語計數(對齊 traditional_rules.md L10–13 / L20–25 + §A)
//
// 模態(references/engine.md §規則模態):
//   - 指引 / 限定語 **不淘汰**,計入 `preference_score`。
//   - advisory(`RightLook` / `WavePersonality` / `Volume`)**不計數** → v1 不評估(主觀 / 資料稀疏)。
//
// 容差:Fibonacci 比率匹配用 `fib_tolerance`(絕對,作用於無量綱比值),**僅計指引、永不淘汰**。

use crate::candidates::{Candidate, HypoKind};
use crate::output::{Direction, GuidelineId, Pivot, QualifierId};

fn amp(p: &[Pivot], a: usize, b: usize) -> f64 {
    (p[b].price - p[a].price).abs()
}

fn near(ratio: f64, target: f64, tol: f64) -> bool {
    (ratio - target).abs() <= tol
}

fn near_any(ratio: f64, targets: &[f64], tol: f64) -> bool {
    targets.iter().any(|t| near(ratio, *t, tol))
}

pub fn evaluate(c: &Candidate, fib_tol: f64) -> (Vec<GuidelineId>, Vec<QualifierId>) {
    let mut g = Vec::new();
    let mut q = Vec::new();
    let p = &c.pivots;
    match c.hypo {
        HypoKind::Impulse | HypoKind::Diagonal => {
            let w1 = amp(p, 0, 1);
            let w2 = amp(p, 1, 2);
            let w3 = amp(p, 2, 3);
            let w4 = amp(p, 3, 4);
            let w5 = amp(p, 4, 5);

            // L10 交替:浪 2 / 浪 4 風格差異(深 ≥0.5 = sharp;淺 = sideways)
            let w2_sharp = (w2 / w1.max(1e-9)) >= 0.5;
            let w4_sharp = (w4 / w3.max(1e-9)) >= 0.5;
            if w2_sharp != w4_sharp {
                g.push(GuidelineId::Alternation);
            }

            // L12 等長:浪 3 為延伸(最長)時,浪 1 ≈ 浪 5(±10% 一般近似)
            if w3 > w1 && w3 > w5 {
                let m = w1.max(w5).max(1e-9);
                if (w1 - w5).abs() / m <= 0.10 {
                    g.push(GuidelineId::Equality);
                }
            }

            // L20 回撤:浪 2 ≈ .618/.5,浪 4 ≈ .382
            if near_any(w2 / w1.max(1e-9), &[0.618, 0.5], fib_tol) {
                g.push(GuidelineId::FibWave2Retrace);
            }
            if near(w4 / w3.max(1e-9), 0.382, fib_tol) {
                g.push(GuidelineId::FibWave4Retrace);
            }
            // L20 推動倍數:浪 5 對浪 1 ≈ .618 / 1.0 / 1.618 / 2.618
            if near_any(w5 / w1.max(1e-9), &[0.618, 1.0, 1.618, 2.618], fib_tol) {
                g.push(GuidelineId::FibMotiveMultiple);
            }

            // 限定語(僅對角):R7/R8「浪 1/4 重疊」特徵(浪 4 端點進入浪 1 領域)
            if c.hypo == HypoKind::Diagonal {
                let overlap = match c.direction {
                    Direction::Up => p[4].price <= p[1].price,
                    Direction::Down => p[4].price >= p[1].price,
                };
                if overlap {
                    q.push(QualifierId::DiagonalWave1Wave4Overlap);
                }
            }
        }
        HypoKind::Zigzag | HypoKind::Flat => {
            // L20 修正倍數:C 對 A ≈ equality / .618 / 1.618
            let a = amp(p, 0, 1);
            let cc = amp(p, 2, 3);
            if near_any(cc / a.max(1e-9), &[1.0, 0.618, 1.618], fib_tol) {
                g.push(GuidelineId::FibCorrectiveMultiple);
            }
        }
        HypoKind::Triangle => {
            // L20 三角形:c≈.618a 或 e≈.618c
            let a = amp(p, 0, 1);
            let cc = amp(p, 2, 3);
            let e = amp(p, 4, 5);
            if near(cc / a.max(1e-9), 0.618, fib_tol) || near(e / cc.max(1e-9), 0.618, fib_tol) {
                g.push(GuidelineId::FibCorrectiveMultiple);
            }
            // R13(三角形位置)= 限定語,但 v1 無上層 degree context 無法判位置 → 不授予
        }
    }
    (g, q)
}
