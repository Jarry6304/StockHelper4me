// monowave.rs — v3 Stage 1:degree-0 monowave 偵測(最小方向單位)
//
// 對齊 traditional_rules.md L1-3(碎形最小單位 = 一條線)+ v3 plan。
// **close-based、不 ATR 過濾**(過濾會抹掉低度數);一根 bar 反轉即收;唯一 `[工程添加]` = ε
// 數值雜訊守門(`|Δclose| < ext×ε` 視平盤,只去浮點塵 / 平盤,不抹結構)。
//
// 單 monowave 無內部結構(線內不可見)→ 這是遞迴觸底,degree 0→1 的子浪細分無法檢查(忠於原書)。

use crate::node::EngineNode;
use crate::output::TradBar;

fn mw(bars: &[TradBar], s: usize, e: usize) -> EngineNode {
    EngineNode::monowave(s, e, bars[s].date, bars[e].date, bars[s].close, bars[e].close)
}

/// 偵測 degree-0 monowave 序列(時序遞增,方向交替)。
pub fn detect_monowaves(bars: &[TradBar], epsilon: f64) -> Vec<EngineNode> {
    let n = bars.len();
    if n < 2 {
        return Vec::new();
    }
    let mut out: Vec<EngineNode> = Vec::new();
    let mut seg_start = 0usize; // 當前 monowave 起點 bar
    let mut ext_idx = 0usize; // 當前 run 的極值 bar
    let mut dir: i8 = 0; // 0 未定 / 1 上 / -1 下

    for i in 1..n {
        let c = bars[i].close;
        let ext_c = bars[ext_idx].close;
        let thr = (ext_c.abs() * epsilon).max(0.0);
        match dir {
            0 => {
                let base = bars[seg_start].close;
                if c > base + thr {
                    dir = 1;
                    ext_idx = i;
                } else if c < base - thr {
                    dir = -1;
                    ext_idx = i;
                }
            }
            1 => {
                if c >= ext_c {
                    ext_idx = i;
                } else if c < ext_c - thr {
                    out.push(mw(bars, seg_start, ext_idx)); // 收上行 monowave
                    seg_start = ext_idx;
                    ext_idx = i;
                    dir = -1;
                }
            }
            _ => {
                if c <= ext_c {
                    ext_idx = i;
                } else if c > ext_c + thr {
                    out.push(mw(bars, seg_start, ext_idx)); // 收下行 monowave
                    seg_start = ext_idx;
                    ext_idx = i;
                    dir = 1;
                }
            }
        }
    }

    // 收尾:最後一段(若有方向)
    if dir != 0 && ext_idx > seg_start {
        out.push(mw(bars, seg_start, ext_idx));
    } else if out.is_empty() {
        // 全平盤 → 退化單 monowave(下游以 monowaves.len() < 3 判 insufficient)
        out.push(mw(bars, 0, n - 1));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn bar(day: u32, close: f64) -> TradBar {
        TradBar {
            date: NaiveDate::from_ymd_opt(2024, 1, day).unwrap(),
            open: close,
            high: close + 0.05,
            low: close - 0.05,
            close,
            volume: Some(1000),
        }
    }

    #[test]
    fn detects_alternating_monowaves() {
        // up 10→20, down 20→14, up 14→26
        let mut bars = Vec::new();
        let mut d = 1u32;
        for v in [10.0, 13.0, 16.0, 20.0] {
            bars.push(bar(d, v));
            d += 1;
        }
        for v in [18.0, 16.0, 14.0] {
            bars.push(bar(d, v));
            d += 1;
        }
        for v in [18.0, 22.0, 26.0] {
            bars.push(bar(d, v));
            d += 1;
        }
        let mws = detect_monowaves(&bars, 0.0);
        assert!(mws.len() >= 3, "expect >=3 monowaves, got {}", mws.len());
        // 方向交替
        for w in mws.windows(2) {
            assert_ne!(w[0].direction, w[1].direction, "monowaves must alternate");
        }
        // 全 degree-0 / mode Unknown
        for m in &mws {
            assert_eq!(m.degree_level, 0);
        }
    }

    #[test]
    fn flat_series_one_degenerate_monowave() {
        let bars: Vec<_> = (1..=4).map(|d| bar(d, 10.0)).collect();
        let mws = detect_monowaves(&bars, 0.0);
        assert_eq!(mws.len(), 1);
    }
}
