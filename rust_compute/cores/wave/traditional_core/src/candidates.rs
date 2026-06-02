// candidates.rs — Stage 2:窮舉合法 5/3 浪候選 + 附形態假設
//
// 對齊 references/engine.md:Stage 2 候選**即攜帶形態假設**(HypoKind),使 Stage 3 硬規則
// 能按假設套用(R5 僅 Impulse、R7/R8/R9 僅對角)。輕量幾何生成閘(`[工程添加]`)避免 forest
// 過爆,但不做淘汰(淘汰在 Stage 3 硬 Validator)。

use crate::output::{Direction, Pivot, PivotKind};

/// Stage 2-3 用的粗形態假設(Stage 4 classifier 再產出完整 TraditionalPatternType)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HypoKind {
    Impulse,
    Diagonal,
    Zigzag,
    Flat,
    Triangle,
}

#[derive(Debug, Clone)]
pub struct Candidate {
    pub id: String,
    /// pivot 子序列(motive / triangle 6 個 / zigzag / flat 4 個)
    pub pivots: Vec<Pivot>,
    pub hypo: HypoKind,
    pub direction: Direction,
}

fn direction_of(first: &Pivot) -> Direction {
    // 首 pivot 為 Low → 首段上行(actionary up);為 High → 首段下行
    match first.kind {
        PivotKind::Low => Direction::Up,
        PivotKind::High => Direction::Down,
    }
}

fn amp(a: &Pivot, b: &Pivot) -> f64 {
    (b.price - a.price).abs()
}

/// 窮舉候選。motive/triangle 取 6-pivot(5 leg)連續窗;corrective 取 4-pivot(3 leg)連續窗。
pub fn generate(pivots: &[Pivot]) -> Vec<Candidate> {
    let mut out = Vec::new();
    let n = pivots.len();

    // 5-leg(6 pivot)窗:motive(Impulse + Diagonal)+ Triangle
    if n >= 6 {
        for i in 0..=(n - 6) {
            let win: Vec<Pivot> = pivots[i..i + 6].to_vec();
            let dir = direction_of(&win[0]);
            // motive:每窗皆出 Impulse 與 Diagonal 兩假設(R5 與 R9 在 Stage 3 分流)
            out.push(Candidate {
                id: format!("imp_{i}"),
                pivots: win.clone(),
                hypo: HypoKind::Impulse,
                direction: dir,
            });
            out.push(Candidate {
                id: format!("dia_{i}"),
                pivots: win.clone(),
                hypo: HypoKind::Diagonal,
                direction: dir,
            });
            // Triangle 生成閘:5 leg 幅度單調收斂或發散(`[工程添加]`)
            let legs = [
                amp(&win[0], &win[1]),
                amp(&win[1], &win[2]),
                amp(&win[2], &win[3]),
                amp(&win[3], &win[4]),
                amp(&win[4], &win[5]),
            ];
            let contracting = legs[0] > legs[2] && legs[2] > legs[4];
            let expanding = legs[0] < legs[2] && legs[2] < legs[4];
            if contracting || expanding {
                out.push(Candidate {
                    id: format!("tri_{i}"),
                    pivots: win,
                    hypo: HypoKind::Triangle,
                    direction: dir,
                });
            }
        }
    }

    // 3-leg(4 pivot)窗:Zigzag / Flat(以 B 對 A 回撤比例輕量分流;重疊區雙出為 alternates)
    if n >= 4 {
        for i in 0..=(n - 4) {
            let win: Vec<Pivot> = pivots[i..i + 4].to_vec();
            let dir = direction_of(&win[0]);
            let amp_a = amp(&win[0], &win[1]).max(1e-9);
            let amp_b = amp(&win[1], &win[2]);
            let r_b = amp_b / amp_a; // B 對 A 回撤比
            if r_b <= 0.9 {
                out.push(Candidate {
                    id: format!("zz_{i}"),
                    pivots: win.clone(),
                    hypo: HypoKind::Zigzag,
                    direction: dir,
                });
            }
            if r_b >= 0.5 {
                out.push(Candidate {
                    id: format!("flat_{i}"),
                    pivots: win,
                    hypo: HypoKind::Flat,
                    direction: dir,
                });
            }
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn piv(idx: usize, price: f64, kind: PivotKind) -> Pivot {
        Pivot {
            bar_index: idx,
            date: NaiveDate::from_ymd_opt(2024, 1, 1).unwrap() + chrono::Duration::days(idx as i64),
            price,
            kind,
        }
    }

    #[test]
    fn generates_motive_and_corrective_windows() {
        // 7 pivot 交替序列 → 至少 2 個 motive 窗 + 多個 corrective 窗
        let pivots = vec![
            piv(0, 10.0, PivotKind::Low),
            piv(1, 14.0, PivotKind::High),
            piv(2, 12.0, PivotKind::Low),
            piv(3, 20.0, PivotKind::High),
            piv(4, 17.0, PivotKind::Low),
            piv(5, 24.0, PivotKind::High),
            piv(6, 21.0, PivotKind::Low),
        ];
        let cands = generate(&pivots);
        assert!(cands.iter().any(|c| c.hypo == HypoKind::Impulse));
        assert!(cands.iter().any(|c| c.hypo == HypoKind::Diagonal));
        assert!(cands.iter().any(|c| matches!(c.hypo, HypoKind::Zigzag | HypoKind::Flat)));
        // 首窗 Low 起 → Up
        let imp = cands.iter().find(|c| c.hypo == HypoKind::Impulse).unwrap();
        assert_eq!(imp.direction, Direction::Up);
    }
}
