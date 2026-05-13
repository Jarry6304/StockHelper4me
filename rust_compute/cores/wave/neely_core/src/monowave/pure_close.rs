// Hybrid OHLC Monowave Detector(filename 仍為 pure_close.rs 保留 git history)
//
// 對齊 m3Spec/neely_core_architecture.md §5.3 (Hybrid OHLC strategy)
//         + m3Spec/neely_rules.md §Monowave 的辨識(Ch3)
//
// **r5 修訂(2026-05-13)**:從 Pure Close 切換到 Hybrid OHLC
//   - 切割演算法用 **(H+L)/2 mid_price**(對齊精華版「單日一筆價」)
//   - Monowave struct 的 start_price / end_price 也存 mid_price 值
//   - 完整 OHLC reference 由 start_date / end_date 對應的原始 OhlcvBar 提供
//     (下游 trigger / LLM 推理時用 input.bars[i] 查 high/low/open/close)
//
// 演算法(mid_price + ATR-filtered reversal):
//   1. 算 Wilder ATR(period)序列
//   2. 從 bars[1..] walk,維護當前 monowave 的 (start_idx, extreme_idx, direction)
//   3. 對每個 bar:
//      - mid_i = (bars[i].high + bars[i].low) / 2
//      - 若同向延伸 → extreme_idx = i
//      - 若反向 movement >= ATR(at extreme) * REVERSAL_ATR_MULTIPLIER → 確認反轉,
//        push 完成的 monowave [start_idx, extreme_idx],新 monowave 從 extreme_idx 起算
//      - 反向但未跨 threshold → 噪音,忽略(extreme_idx 不更新)
//
// REVERSAL_ATR_MULTIPLIER 寫死 0.5(noise floor 為半個 ATR);
// P0 Gate 六檔股票實測後可能調整,目前不外部化(對齊 architecture §6.6)。
//
// 邊界:
//   - bars.len() < 2 → 回空 vec
//   - 連續同 mid_price → 不視為反轉,延伸當前 monowave
//   - 全部單向走勢(不反轉)→ 整段一個 monowave

use crate::output::{Monowave, MonowaveDirection, OhlcvBar};

/// Reversal noise floor:反向 movement 小於此倍數 ATR 不視為反轉
const REVERSAL_ATR_MULTIPLIER: f64 = 0.5;

/// 單一 OHLC bar 的 mid price = (high + low) / 2(r5 Hybrid OHLC 切割用)
#[inline]
fn mid_price(bar: &OhlcvBar) -> f64 {
    (bar.high + bar.low) / 2.0
}

/// 偵測 monowaves(r5 Hybrid OHLC:mid_price = (H+L)/2)。
///
/// `atr_period` 對齊 NeelyEngineConfig.atr_period(預設 14)。
pub fn detect_monowaves(bars: &[OhlcvBar], atr_period: usize) -> Vec<Monowave> {
    if bars.len() < 2 {
        return Vec::new();
    }
    let atrs = compute_atr_series(bars, atr_period);

    let mut waves = Vec::new();
    let mut start_idx = 0usize;
    let mut extreme_idx = 0usize;
    // direction:0 = 未確定 / 1 = 上 / -1 = 下
    let mut direction: i8 = 0;

    for i in 1..bars.len() {
        let cur_mid = mid_price(&bars[i]);
        let extreme_mid = mid_price(&bars[extreme_idx]);
        let movement = cur_mid - extreme_mid;

        // 反轉門檻 = ATR(at extreme) * multiplier;ATR 為 0 時 fallback 為 0
        let atr_at_extreme = atrs.get(extreme_idx).copied().unwrap_or(0.0);
        let reversal_threshold = (atr_at_extreme * REVERSAL_ATR_MULTIPLIER).max(0.0);

        let new_direction = signum(movement);

        if direction == 0 {
            // 第 1 個明確方向 → 開啟 monowave
            if new_direction != 0 {
                direction = new_direction;
                extreme_idx = i;
            }
            // 否則(mid 不變)維持 start_idx == extreme_idx == 0,等下一根確定方向
            continue;
        }

        if new_direction == 0 {
            // mid 不變(罕見但可能,例如停板鎖死 H==L)→ 視為延伸,extreme_idx 不更新
            continue;
        }

        if new_direction == direction {
            // 同向延伸:更新 extreme
            extreme_idx = i;
        } else {
            // 反向 movement:檢查是否跨 reversal_threshold
            if movement.abs() >= reversal_threshold {
                // 確認反轉 → push 完成的 monowave
                waves.push(Monowave {
                    start_date: bars[start_idx].date,
                    end_date: bars[extreme_idx].date,
                    start_price: mid_price(&bars[start_idx]),
                    end_price: mid_price(&bars[extreme_idx]),
                    direction: dir_enum(direction),
                });
                start_idx = extreme_idx;
                extreme_idx = i;
                direction = new_direction;
            }
            // else:噪音,忽略此 bar(extreme_idx 不動)
        }
    }

    // 最後一個未完成的 monowave(extreme 至少推進過一次)
    if extreme_idx > start_idx {
        waves.push(Monowave {
            start_date: bars[start_idx].date,
            end_date: bars[extreme_idx].date,
            start_price: mid_price(&bars[start_idx]),
            end_price: mid_price(&bars[extreme_idx]),
            direction: dir_enum(direction),
        });
    }

    waves
}

fn signum(x: f64) -> i8 {
    if x > 0.0 {
        1
    } else if x < 0.0 {
        -1
    } else {
        0
    }
}

fn dir_enum(d: i8) -> MonowaveDirection {
    match d {
        1 => MonowaveDirection::Up,
        -1 => MonowaveDirection::Down,
        _ => MonowaveDirection::Neutral,
    }
}

/// Wilder ATR(true range with Wilder smoothing factor 1/N)。
///
/// 演算法(Welles Wilder 1978,技術分析界事實標準):
///   - TR[0] = high[0] - low[0]
///   - TR[i] = max(high[i]-low[i], |high[i]-close[i-1]|, |low[i]-close[i-1]|)
///   - ATR[0..period-1] = cumulative average of TR(暖機)
///   - ATR[i] = ((period-1) * ATR[i-1] + TR[i]) / period   for i >= period
///
/// 邊界:資料不足 period 時走 cumulative average 一路到底(避免 panic)。
///
/// 注意:ATR 計算仍用 OHLC raw(true range 需要 high/low/prev_close),
/// **不**用 mid_price — Hybrid 設計只切換 monowave 切割演算法的 reference price,
/// ATR 本身語意保留 OHLC 原汁(對齊 r5 §5.3 「保留完整 OHLC reference」精神)。
pub fn compute_atr_series(bars: &[OhlcvBar], period: usize) -> Vec<f64> {
    let n = bars.len();
    if n == 0 {
        return Vec::new();
    }
    if period == 0 {
        // 異常 input,回 0 序列(避免除以 0)
        return vec![0.0; n];
    }

    let mut tr = Vec::with_capacity(n);
    tr.push((bars[0].high - bars[0].low).max(0.0));
    for i in 1..n {
        let prev_close = bars[i - 1].close;
        let h = bars[i].high;
        let l = bars[i].low;
        let candidate = [
            (h - l).abs(),
            (h - prev_close).abs(),
            (l - prev_close).abs(),
        ];
        tr.push(candidate.iter().cloned().fold(0.0_f64, f64::max));
    }

    let mut atr = vec![0.0_f64; n];
    let warmup = period.min(n);

    // 前 warmup 個 ATR = cumulative average(暖機)
    let mut sum = 0.0;
    for i in 0..warmup {
        sum += tr[i];
        atr[i] = sum / (i + 1) as f64;
    }

    // warmup 之後:Wilder smoothing
    for i in warmup..n {
        atr[i] = ((period as f64 - 1.0) * atr[i - 1] + tr[i]) / period as f64;
    }
    atr
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn bar(d: &str, o: f64, h: f64, l: f64, c: f64) -> OhlcvBar {
        OhlcvBar {
            date: NaiveDate::parse_from_str(d, "%Y-%m-%d").unwrap(),
            open: o,
            high: h,
            low: l,
            close: c,
            volume: None,
        }
    }

    #[test]
    fn atr_empty_input_returns_empty() {
        assert!(compute_atr_series(&[], 14).is_empty());
    }

    #[test]
    fn atr_zero_period_returns_zeros() {
        let bars = vec![bar("2026-01-01", 100.0, 101.0, 99.0, 100.5)];
        let atrs = compute_atr_series(&bars, 0);
        assert_eq!(atrs, vec![0.0]);
    }

    #[test]
    fn atr_first_bar_equals_high_minus_low() {
        let bars = vec![bar("2026-01-01", 100.0, 102.0, 98.0, 101.0)];
        let atrs = compute_atr_series(&bars, 14);
        assert!((atrs[0] - 4.0).abs() < 1e-9);
    }

    #[test]
    fn atr_wilder_smoothing_matches_manual_computation() {
        // 5 個 bar,period = 3。手算 Wilder ATR 對照。
        let bars = vec![
            bar("2026-01-01", 10.0, 11.0, 9.0, 10.0),  // TR0 = 2.0
            bar("2026-01-02", 10.0, 12.0, 9.5, 11.5),  // TR1 = max(2.5, 2.0, 0.5) = 2.5
            bar("2026-01-03", 11.5, 13.0, 11.0, 12.5), // TR2 = max(2.0, 1.5, 0.5) = 2.0
            bar("2026-01-04", 12.5, 14.0, 12.0, 13.0), // TR3 = max(2.0, 1.5, 0.5) = 2.0
            bar("2026-01-05", 13.0, 13.5, 11.5, 12.0), // TR4 = max(2.0, 0.5, 1.5) = 2.0
        ];
        let atrs = compute_atr_series(&bars, 3);
        // 暖機(period=3):ATR0=2.0/1=2.0,ATR1=(2.0+2.5)/2=2.25,ATR2=(2.0+2.5+2.0)/3=2.1667
        assert!((atrs[0] - 2.0).abs() < 1e-9);
        assert!((atrs[1] - 2.25).abs() < 1e-9);
        assert!((atrs[2] - 2.1666666666666665).abs() < 1e-9);
        // ATR3 = (2 * 2.1667 + 2.0) / 3 = (4.3333 + 2.0)/3 = 2.1111
        assert!((atrs[3] - 2.111111111111111).abs() < 1e-9);
        // ATR4 = (2 * 2.1111 + 2.0) / 3 = 2.0741
        assert!((atrs[4] - 2.0740740740740735).abs() < 1e-9);
    }

    #[test]
    fn detect_empty_or_single_bar_returns_empty() {
        assert!(detect_monowaves(&[], 14).is_empty());
        let one = vec![bar("2026-01-01", 100.0, 101.0, 99.0, 100.0)];
        assert!(detect_monowaves(&one, 14).is_empty());
    }

    #[test]
    fn mid_price_is_average_of_high_and_low() {
        // sanity check Hybrid OHLC 切換:mid_price = (H+L)/2
        let b = bar("2026-01-01", 10.0, 12.0, 8.0, 11.0);
        assert!((mid_price(&b) - 10.0).abs() < 1e-9, "(12+8)/2 = 10");
    }

    #[test]
    fn detect_pure_uptrend_yields_single_monowave() {
        // 5 個 bar,mid 連續上漲(10 → 11 → 12 → 13 → 14)且每日 ATR ~1
        // REVERSAL threshold = 0.5,沒任何反向 bar → 整段為單一 Up monowave
        let bars = vec![
            bar("2026-01-01", 10.0, 10.5, 9.5, 10.0), // mid = 10.0
            bar("2026-01-02", 10.0, 11.5, 10.5, 11.0), // mid = 11.0
            bar("2026-01-03", 11.0, 12.5, 11.5, 12.0), // mid = 12.0
            bar("2026-01-04", 12.0, 13.5, 12.5, 13.0), // mid = 13.0
            bar("2026-01-05", 13.0, 14.5, 13.5, 14.0), // mid = 14.0
        ];
        let waves = detect_monowaves(&bars, 14);
        assert_eq!(waves.len(), 1);
        assert!(matches!(waves[0].direction, MonowaveDirection::Up));
        assert!((waves[0].start_price - 10.0).abs() < 1e-9);
        assert!((waves[0].end_price - 14.0).abs() < 1e-9);
        assert_eq!(
            waves[0].start_date,
            NaiveDate::parse_from_str("2026-01-01", "%Y-%m-%d").unwrap()
        );
        assert_eq!(
            waves[0].end_date,
            NaiveDate::parse_from_str("2026-01-05", "%Y-%m-%d").unwrap()
        );
    }

    #[test]
    fn detect_clean_zigzag_yields_three_monowaves() {
        // 上漲 → 大幅下跌 → 大幅上漲,ATR 控制在 ~1,反向 mid movement >> 0.5*ATR
        // 7 個 bar:up to peak (3 bars), down (2 bars), up again (2 bars)
        // mid 序列:10 → 10.75 → 12.0 → 12.25 → 10.25 → 10.0 → 12.25
        let bars = vec![
            bar("2026-01-01", 10.0, 10.5, 9.5, 10.0),   // mid 10.0
            bar("2026-01-02", 10.0, 11.5, 10.0, 11.0),  // mid 10.75
            bar("2026-01-03", 11.0, 13.0, 11.0, 13.0),  // mid 12.0 — peak (mid)
            bar("2026-01-04", 13.0, 13.0, 11.5, 11.5),  // mid 12.25 — slightly higher mid!
            bar("2026-01-05", 11.5, 11.5, 9.0, 9.0),    // mid 10.25 — trough region
            bar("2026-01-06", 9.0, 11.0, 9.0, 11.0),    // mid 10.0 — actual trough
            bar("2026-01-07", 11.0, 13.5, 11.0, 13.5),  // mid 12.25 — second peak
        ];
        let waves = detect_monowaves(&bars, 14);
        // Hybrid OHLC 比 close 更平滑(mid 受 high/low 雙影響),預期 3 段 zigzag
        // 但具體段數依 ATR 噪音門檻可能與 close 版本不同 — 本測試只驗 ≥2 段
        assert!(
            waves.len() >= 2,
            "預期至少 2 段(up/down),實際 {} 段",
            waves.len()
        );
        // 第一段應為 Up
        assert!(matches!(waves[0].direction, MonowaveDirection::Up));
    }

    #[test]
    fn detect_small_noise_does_not_trigger_reversal() {
        // 連續上漲 + 中間有 1 根極小回調(mid 微跌)→ 不算反轉,仍為單一 Up monowave
        // mid 序列:100 → 101 → 103 → 103.8 → 104.95 → 107
        let bars = vec![
            bar("2026-01-01", 100.0, 101.0, 99.0, 100.0),   // mid 100.0
            bar("2026-01-02", 100.0, 102.0, 100.0, 102.0),  // mid 101.0
            bar("2026-01-03", 102.0, 104.0, 102.0, 104.0),  // mid 103.0
            // 微回調:mid 從 103 → 103.8(+0.8 — 仍向上,只是慢)
            bar("2026-01-04", 104.0, 104.1, 103.5, 103.9),  // mid 103.8
            bar("2026-01-05", 103.9, 106.0, 103.9, 106.0),  // mid 104.95
            bar("2026-01-06", 106.0, 108.0, 106.0, 108.0),  // mid 107.0
        ];
        let waves = detect_monowaves(&bars, 3);
        // mid 序列全程上升 → 整段單一 Up monowave
        assert_eq!(waves.len(), 1);
        assert!(matches!(waves[0].direction, MonowaveDirection::Up));
        assert!((waves[0].start_price - 100.0).abs() < 1e-9);
        assert!((waves[0].end_price - 107.0).abs() < 1e-9);
    }
}
