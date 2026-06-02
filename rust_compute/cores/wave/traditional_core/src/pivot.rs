// pivot.rs — Stage 1:ATR×k swing pivot 偵測(ZigZag)
//
// 原書(Frost & Prechter EWP)假設可見圖、無 pivot 演算法 → 本階段為 `[工程添加]`。
// 以 Wilder ATR(t) × swing_atr_multiplier 為逐 bar 反轉門檻,產生交替的 High/Low pivot。
// 單一決定性 pivot 設定(對齊 SPEC 風險段「起手單一決定性 pivot + cap」),解耦後可獨立調(不回歸 Neely)。

use crate::output::{Pivot, PivotKind, TradBar};

/// `[工程添加]` Wilder ATR 序列(seed = 前 period 個 TR 之 SMA)。
fn wilder_atr(bars: &[TradBar], period: usize) -> Vec<f64> {
    let n = bars.len();
    let mut atr = vec![0.0; n];
    if n == 0 {
        return atr;
    }
    let p = period.max(1);
    let mut tr = vec![0.0; n];
    for i in 0..n {
        tr[i] = if i == 0 {
            bars[i].high - bars[i].low
        } else {
            let h = bars[i].high;
            let l = bars[i].low;
            let pc = bars[i - 1].close;
            (h - l).max((h - pc).abs()).max((l - pc).abs())
        };
    }
    let seed_len = p.min(n);
    let seed: f64 = tr[..seed_len].iter().sum::<f64>() / seed_len as f64;
    for i in 0..n {
        atr[i] = if i < seed_len {
            seed
        } else {
            (atr[i - 1] * (p as f64 - 1.0) + tr[i]) / p as f64
        };
    }
    atr
}

/// 偵測交替的 swing pivot。反轉門檻 = ATR(t) × k(逐 bar 動態,適應價格量級變化)。
///
/// `[工程添加]`。輸出依時序遞增;首尾 pivot 為暫定(最近一段以當前極值收尾)。
pub fn detect_pivots(bars: &[TradBar], atr_period: usize, k: f64) -> Vec<Pivot> {
    let n = bars.len();
    if n < 2 {
        return Vec::new();
    }
    let atr = wilder_atr(bars, atr_period);
    let mut pivots: Vec<Pivot> = Vec::new();

    let anchor = 0usize; // 初始錨點(bar 0)
    let mut dir: i8 = 0; // 0 未定 / 1 上升段(找 High)/ -1 下降段(找 Low)
    let mut ext_idx = 0usize; // 當前極值 bar index

    for i in 1..n {
        let thr = (atr[i] * k).max(1e-9);
        match dir {
            0 => {
                if bars[i].high - bars[anchor].low >= thr {
                    pivots.push(mk(bars, anchor, PivotKind::Low));
                    dir = 1;
                    ext_idx = i;
                } else if bars[anchor].high - bars[i].low >= thr {
                    pivots.push(mk(bars, anchor, PivotKind::High));
                    dir = -1;
                    ext_idx = i;
                }
            }
            1 => {
                if bars[i].high >= bars[ext_idx].high {
                    ext_idx = i;
                }
                if bars[ext_idx].high - bars[i].low >= thr {
                    pivots.push(mk(bars, ext_idx, PivotKind::High));
                    dir = -1;
                    ext_idx = i;
                }
            }
            _ => {
                if bars[i].low <= bars[ext_idx].low {
                    ext_idx = i;
                }
                if bars[i].high - bars[ext_idx].low >= thr {
                    pivots.push(mk(bars, ext_idx, PivotKind::Low));
                    dir = 1;
                    ext_idx = i;
                }
            }
        }
    }

    // 收尾:最近一段以當前極值補上暫定 pivot
    if dir != 0 {
        let kind = if dir == 1 { PivotKind::High } else { PivotKind::Low };
        let dup = pivots.last().map(|p| p.bar_index == ext_idx).unwrap_or(false);
        if !dup {
            pivots.push(mk(bars, ext_idx, kind));
        }
    }
    pivots
}

fn mk(bars: &[TradBar], idx: usize, kind: PivotKind) -> Pivot {
    let price = match kind {
        PivotKind::High => bars[idx].high,
        PivotKind::Low => bars[idx].low,
    };
    Pivot {
        bar_index: idx,
        date: bars[idx].date,
        price,
        kind,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn bar(day: u32, high: f64, low: f64) -> TradBar {
        TradBar {
            date: NaiveDate::from_ymd_opt(2024, 1, day).unwrap(),
            open: (high + low) / 2.0,
            high,
            low,
            close: (high + low) / 2.0,
            volume: Some(1000),
        }
    }

    #[test]
    fn detects_alternating_pivots_on_zigzag() {
        // 構造明顯的 上→下→上 三段,幅度遠大於 ATR×k
        let mut bars = Vec::new();
        // 上升 (10→20)
        for d in 1..=6 {
            let v = 10.0 + d as f64 * 2.0;
            bars.push(bar(d, v + 0.2, v - 0.2));
        }
        // 下降 (20→13)
        for d in 7..=12 {
            let v = 22.0 - (d - 6) as f64 * 1.5;
            bars.push(bar(d, v + 0.2, v - 0.2));
        }
        // 上升 (13→25)
        for d in 13..=20 {
            let v = 13.0 + (d - 12) as f64 * 1.5;
            bars.push(bar(d, v + 0.2, v - 0.2));
        }
        let pivots = detect_pivots(&bars, 3, 2.0);
        assert!(pivots.len() >= 3, "should detect at least 3 pivots, got {}", pivots.len());
        // pivot 應交替 High/Low
        for w in pivots.windows(2) {
            assert_ne!(w[0].kind, w[1].kind, "pivots must alternate H/L");
        }
    }

    #[test]
    fn empty_on_tiny_series() {
        let bars = vec![bar(1, 10.0, 9.0)];
        assert!(detect_pivots(&bars, 14, 3.0).is_empty());
    }
}
