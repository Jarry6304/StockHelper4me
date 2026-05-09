// indicator_kernel — Cores 共用 indicator math kernel
//
// 對齊 m2Spec/oldm2Spec/indicator_cores_*.md(spec user m3Spec 待寫)。
//
// 範圍:
//   - Moving Averages:sma / ema / wma
//   - Wilder smoothing(Welles Wilder 1978 標準算法)
//     - true_range / wilder_atr / wilder_rsi 共用 smoothing 因子 1/N
//   - 統計:standard_deviation(window)
//
// 設計原則:
//   - 純 stateless function,easy to golden-test
//   - 每個函式都有 golden test 對應 TA-Lib / pandas-ta 預期值
//   - 邊界處理(empty input / period=0)一律 return zero series 不 panic
//
// 後續 cores 一律 dep 此 crate,**不**自己重新實作 indicator math。

use ohlcv_loader::OhlcvBar;

// ---------------------------------------------------------------------------
// Moving Averages
// ---------------------------------------------------------------------------

/// Simple Moving Average
///
/// `out[i] = mean(values[max(0, i-period+1)..=i])`,前 period-1 筆走 cumulative average(暖機)
pub fn sma(values: &[f64], period: usize) -> Vec<f64> {
    let n = values.len();
    let mut out = vec![0.0; n];
    if n == 0 || period == 0 {
        return out;
    }
    let mut sum = 0.0;
    for i in 0..n {
        sum += values[i];
        if i >= period {
            sum -= values[i - period];
        }
        let div = (i + 1).min(period) as f64;
        out[i] = sum / div;
    }
    out
}

/// Exponential Moving Average
///
/// alpha = 2 / (period + 1);out[0] = values[0];
/// out[i] = alpha * values[i] + (1 - alpha) * out[i-1]
pub fn ema(values: &[f64], period: usize) -> Vec<f64> {
    let n = values.len();
    let mut out = vec![0.0; n];
    if n == 0 || period == 0 {
        return out;
    }
    let alpha = 2.0 / (period as f64 + 1.0);
    out[0] = values[0];
    for i in 1..n {
        out[i] = alpha * values[i] + (1.0 - alpha) * out[i - 1];
    }
    out
}

/// Weighted Moving Average(linear weights — 越近權重越大)
///
/// out[i] = sum(w_k * v_{i-(p-1-k)} for k in 0..p) / sum(w_k for k in 0..p)
/// 其中 w_k = k + 1,p = min(i+1, period)
pub fn wma(values: &[f64], period: usize) -> Vec<f64> {
    let n = values.len();
    let mut out = vec![0.0; n];
    if n == 0 || period == 0 {
        return out;
    }
    for i in 0..n {
        let p = (i + 1).min(period);
        let mut num = 0.0;
        let mut den = 0.0;
        for k in 0..p {
            let w = (k + 1) as f64;
            num += w * values[i - (p - 1 - k)];
            den += w;
        }
        out[i] = if den > 0.0 { num / den } else { 0.0 };
    }
    out
}

// ---------------------------------------------------------------------------
// True Range / ATR / Wilder smoothing
// ---------------------------------------------------------------------------

/// True Range:max(H-L, |H - prev_close|, |L - prev_close|)
///
/// `tr[0] = high[0] - low[0]`(無 prev close,回退到 H-L);
/// `tr[i] = max(high[i]-low[i], |high[i] - close[i-1]|, |low[i] - close[i-1]|)`
pub fn true_range(bars: &[OhlcvBar]) -> Vec<f64> {
    let n = bars.len();
    let mut out = Vec::with_capacity(n);
    if n == 0 {
        return out;
    }
    out.push((bars[0].high - bars[0].low).max(0.0));
    for i in 1..n {
        let prev_close = bars[i - 1].close;
        let h = bars[i].high;
        let l = bars[i].low;
        let cands = [(h - l).abs(), (h - prev_close).abs(), (l - prev_close).abs()];
        out.push(cands.iter().cloned().fold(0.0_f64, f64::max));
    }
    out
}

/// Wilder smoothing 一步:`new = ((period - 1) * prev + cur) / period`
///
/// 對齊 Welles Wilder 1978 標準。等價於 EMA with alpha = 1/period(非 2/(N+1))。
pub fn wilder_smooth_step(prev: f64, cur: f64, period: usize) -> f64 {
    if period == 0 {
        return cur;
    }
    ((period as f64 - 1.0) * prev + cur) / period as f64
}

/// Wilder smoothing of a series:暖機 period 筆走 cumulative average,之後走 wilder_smooth_step
///
/// 廣泛用於 ATR / RSI / ADX 等 Wilder-style indicators。
pub fn wilder_smoothing(values: &[f64], period: usize) -> Vec<f64> {
    let n = values.len();
    let mut out = vec![0.0; n];
    if n == 0 || period == 0 {
        return out;
    }
    let warmup = period.min(n);
    let mut sum = 0.0;
    for i in 0..warmup {
        sum += values[i];
        out[i] = sum / (i + 1) as f64;
    }
    for i in warmup..n {
        out[i] = wilder_smooth_step(out[i - 1], values[i], period);
    }
    out
}

/// Wilder ATR(true range with Wilder smoothing factor 1/N)
///
/// 對齊 Welles Wilder 1978(技術分析界事實標準)。
/// 同時用於 atr_core 與 neely_core monowave detection。
pub fn wilder_atr(bars: &[OhlcvBar], period: usize) -> Vec<f64> {
    let tr = true_range(bars);
    wilder_smoothing(&tr, period)
}

// ---------------------------------------------------------------------------
// Wilder RSI
// ---------------------------------------------------------------------------

/// Wilder RSI:up/down avg + Wilder smoothing → RS = avg_gain / avg_loss → RSI = 100 - 100/(1+RS)
///
/// 邊界:
/// - n < 2 或 period = 0 → 回 0 序列
/// - avg_loss < epsilon → RSI = 100(無下跌,純上漲)
/// - 暖機期間(< period)→ rsi 為 0(尚未有 valid RS)
pub fn wilder_rsi(closes: &[f64], period: usize) -> Vec<f64> {
    let n = closes.len();
    if n < 2 || period == 0 {
        return vec![0.0; n];
    }
    let mut gains = vec![0.0_f64; n];
    let mut losses = vec![0.0_f64; n];
    for i in 1..n {
        let d = closes[i] - closes[i - 1];
        if d > 0.0 {
            gains[i] = d;
        } else {
            losses[i] = -d;
        }
    }
    // 暖機:第一筆 avg = sum(period 筆 gains) / period;之後走 Wilder smoothing
    let warmup = period.min(n - 1);
    let mut avg_gain = vec![0.0; n];
    let mut avg_loss = vec![0.0; n];
    let mut sg = 0.0;
    let mut sl = 0.0;
    for i in 1..=warmup {
        sg += gains[i];
        sl += losses[i];
    }
    let p = warmup as f64;
    avg_gain[warmup] = sg / p;
    avg_loss[warmup] = sl / p;
    for i in (warmup + 1)..n {
        avg_gain[i] = wilder_smooth_step(avg_gain[i - 1], gains[i], period);
        avg_loss[i] = wilder_smooth_step(avg_loss[i - 1], losses[i], period);
    }
    let mut rsi = vec![0.0; n];
    for i in warmup..n {
        rsi[i] = if avg_loss[i] < 1e-12 {
            100.0
        } else {
            let rs = avg_gain[i] / avg_loss[i];
            100.0 - 100.0 / (1.0 + rs)
        };
    }
    rsi
}

// ---------------------------------------------------------------------------
// 統計
// ---------------------------------------------------------------------------

/// Rolling standard deviation(population stddev,n 而非 n-1)
///
/// 對齊 Bollinger Bands 慣例(John Bollinger 用 population stddev)。
pub fn standard_deviation(values: &[f64], period: usize) -> Vec<f64> {
    let n = values.len();
    let mut out = vec![0.0; n];
    if n == 0 || period == 0 {
        return out;
    }
    for i in 0..n {
        let p = (i + 1).min(period);
        let start = i + 1 - p;
        let win = &values[start..=i];
        let mean: f64 = win.iter().sum::<f64>() / p as f64;
        let var: f64 = win.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / p as f64;
        out[i] = var.sqrt();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn b(d: &str, h: f64, l: f64, c: f64) -> OhlcvBar {
        OhlcvBar {
            date: NaiveDate::parse_from_str(d, "%Y-%m-%d").unwrap(),
            open: c,
            high: h,
            low: l,
            close: c,
            volume: None,
        }
    }

    // ---------- SMA / EMA / WMA ----------

    #[test]
    fn sma_simple() {
        let v = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let r = sma(&v, 3);
        // 暖機:r[0]=1, r[1]=1.5, r[2]=2;windowed: r[3]=3, r[4]=4
        assert!((r[0] - 1.0).abs() < 1e-9);
        assert!((r[1] - 1.5).abs() < 1e-9);
        assert!((r[2] - 2.0).abs() < 1e-9);
        assert!((r[3] - 3.0).abs() < 1e-9);
        assert!((r[4] - 4.0).abs() < 1e-9);
    }

    #[test]
    fn sma_empty_or_zero_period_returns_zero_vec() {
        assert!(sma(&[], 3).is_empty());
        let r = sma(&[1.0, 2.0], 0);
        assert_eq!(r, vec![0.0, 0.0]);
    }

    #[test]
    fn ema_first_equals_input_first() {
        let v = vec![10.0, 20.0, 30.0, 40.0];
        let r = ema(&v, 3);
        assert!((r[0] - 10.0).abs() < 1e-9);
        // alpha = 2/(3+1) = 0.5
        // r[1] = 0.5*20 + 0.5*10 = 15
        assert!((r[1] - 15.0).abs() < 1e-9);
        // r[2] = 0.5*30 + 0.5*15 = 22.5
        assert!((r[2] - 22.5).abs() < 1e-9);
        // r[3] = 0.5*40 + 0.5*22.5 = 31.25
        assert!((r[3] - 31.25).abs() < 1e-9);
    }

    #[test]
    fn wma_weights_increasing() {
        let v = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let r = wma(&v, 3);
        // r[2] = (1*1 + 2*2 + 3*3) / (1+2+3) = 14/6 ≈ 2.3333
        assert!((r[2] - 2.3333333333333335).abs() < 1e-9);
        // r[3] = (1*2 + 2*3 + 3*4) / 6 = 20/6 ≈ 3.3333
        assert!((r[3] - 3.3333333333333335).abs() < 1e-9);
    }

    // ---------- True Range ----------

    #[test]
    fn true_range_first_bar_high_minus_low() {
        let bars = vec![b("2026-01-01", 102.0, 98.0, 100.0)];
        let tr = true_range(&bars);
        assert!((tr[0] - 4.0).abs() < 1e-9);
    }

    #[test]
    fn true_range_uses_max_of_three() {
        // 第二根 bar:high gap up,prev close 比 low 還低
        // bars[0]: H=102 L=98 C=100
        // bars[1]: H=110 L=105 C=108
        //   max(110-105=5, |110-100|=10, |105-100|=5) = 10
        let bars = vec![b("2026-01-01", 102.0, 98.0, 100.0), b("2026-01-02", 110.0, 105.0, 108.0)];
        let tr = true_range(&bars);
        assert!((tr[1] - 10.0).abs() < 1e-9);
    }

    // ---------- Wilder ATR ----------

    #[test]
    fn wilder_atr_warmup_then_smoothing() {
        // 與 neely_core::monowave::pure_close::compute_atr_series 同 input,確保 algorithm 一致
        let bars = vec![
            b("2026-01-01", 11.0, 9.0, 10.0),  // TR0 = 2.0
            b("2026-01-02", 12.0, 9.5, 11.5),  // TR1 = max(2.5, 2.0, 0.5) = 2.5
            b("2026-01-03", 13.0, 11.0, 12.5), // TR2 = max(2.0, 1.5, 0.5) = 2.0
            b("2026-01-04", 14.0, 12.0, 13.0), // TR3 = max(2.0, 1.5, 0.5) = 2.0
            b("2026-01-05", 13.5, 11.5, 12.0), // TR4 = max(2.0, 0.5, 1.5) = 2.0
        ];
        let atr = wilder_atr(&bars, 3);
        assert!((atr[0] - 2.0).abs() < 1e-9);
        assert!((atr[1] - 2.25).abs() < 1e-9);
        assert!((atr[2] - 2.1666666666666665).abs() < 1e-9);
        assert!((atr[3] - 2.111111111111111).abs() < 1e-9);
        assert!((atr[4] - 2.0740740740740735).abs() < 1e-9);
    }

    // ---------- Wilder RSI ----------

    #[test]
    fn wilder_rsi_steady_uptrend_returns_100() {
        // 連續純上漲 → loss=0 → RSI=100
        let closes: Vec<f64> = (0..30).map(|i| 100.0 + i as f64).collect();
        let rsi = wilder_rsi(&closes, 14);
        assert!((rsi[20] - 100.0).abs() < 1e-9);
    }

    #[test]
    fn wilder_rsi_steady_downtrend_returns_0() {
        let closes: Vec<f64> = (0..30).map(|i| 100.0 - i as f64).collect();
        let rsi = wilder_rsi(&closes, 14);
        assert!(rsi[20] < 1e-6, "純下跌應 RSI ≈ 0,實際 {}", rsi[20]);
    }

    #[test]
    fn wilder_rsi_alternating_returns_50ish() {
        // 完全交替漲跌 1 點 → gains/losses 平均相等 → RSI = 50
        let closes: Vec<f64> = (0..40).map(|i| if i % 2 == 0 { 100.0 } else { 101.0 }).collect();
        let rsi = wilder_rsi(&closes, 14);
        // 暖機後 RSI 接近 50(可能略偏因 Wilder smoothing 不對稱)
        assert!((rsi[30] - 50.0).abs() < 5.0, "交替漲跌 RSI 應 ~50,實際 {}", rsi[30]);
    }

    // ---------- Standard Deviation ----------

    #[test]
    fn standard_deviation_constant_is_zero() {
        let v = vec![5.0; 10];
        let s = standard_deviation(&v, 5);
        for &x in &s {
            assert!(x.abs() < 1e-9);
        }
    }

    #[test]
    fn standard_deviation_simple_window() {
        // values = [2, 4, 4, 4, 5, 5, 7, 9],population std for last 8 = 2.0
        let v = vec![2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0];
        let s = standard_deviation(&v, 8);
        assert!((s[7] - 2.0).abs() < 1e-9);
    }

    // ---------- Wilder smoothing step ----------

    #[test]
    fn wilder_smooth_step_zero_period_returns_cur() {
        assert!((wilder_smooth_step(10.0, 20.0, 0) - 20.0).abs() < 1e-9);
    }

    #[test]
    fn wilder_smooth_step_period_14() {
        // ((14 - 1) * 100 + 50) / 14 = (1300 + 50) / 14 = 1350/14 ≈ 96.428...
        let r = wilder_smooth_step(100.0, 50.0, 14);
        assert!((r - 96.42857142857143).abs() < 1e-9);
    }
}
