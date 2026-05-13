// rsi_core(P1)— 對齊 m2Spec/oldm2Spec/indicator_cores_momentum.md §四 r2
// Output §4.4(僅 series.value)+ Fact §4.5(5 種)+ Failure Swing §4.6
//
// **Reference(2026-05-10 加)**:
//   period=14:Wilder (1978) Ch. 21 — 原版作者選 14 對應 ~2 週
//   overbought=70 / oversold=30:Wilder (1978) 原版 + Murphy (1999)
//                                 "Technical Analysis of the Financial Markets" Ch. 9
//   Failure Swing:Wilder (1978) §7 RSI — 4 步 reversal 經典邏輯
//   streak_min_days=3:無明確學術,業界經驗值「短期確認」標準

use anyhow::Result;
use chrono::NaiveDate;
use fact_schema::{Fact, IndicatorCore, Timeframe};
use ohlcv_loader::OhlcvSeries;
use serde::Serialize;
use serde_json::json;

inventory::submit! {
    core_registry::CoreRegistration::new(
        "rsi_core", "0.1.0", core_registry::CoreKind::Indicator, "P1",
        "RSI Core(Wilder RSI 14-period)",
    )
}

/// Reference(2026-05-12 校準): Wilder (1978) 未指定連續天數；Connors (2008) ConnorsRSI
/// 以 3 個連續極端值作為雜訊過濾，提供間接實務支持。3 天屬實務慣例。
const STREAK_MIN_DAYS: usize = 3;

#[derive(Debug, Clone, Serialize)]
pub struct RsiParams { pub period: usize, pub overbought: f64, pub oversold: f64, pub timeframe: Timeframe }
impl Default for RsiParams { fn default() -> Self { Self { period: 14, overbought: 70.0, oversold: 30.0, timeframe: Timeframe::Daily } } }

#[derive(Debug, Clone, Serialize)]
pub struct RsiOutput {
    pub stock_id: String, pub timeframe: Timeframe,
    pub series: Vec<RsiPoint>,
    #[serde(skip)]
    pub events: Vec<RsiEvent>,
}
#[derive(Debug, Clone, Serialize)]
pub struct RsiPoint { pub date: NaiveDate, pub value: f64 }
#[derive(Debug, Clone, Serialize)]
pub struct RsiEvent { pub date: NaiveDate, pub kind: RsiEventKind, pub value: f64, pub metadata: serde_json::Value }
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum RsiEventKind { OverboughtStreak, OversoldStreak, OverboughtExit, OversoldExit, BearishDivergence, BullishDivergence, FailureSwing }

pub struct RsiCore;
impl RsiCore { pub fn new() -> Self { RsiCore } }
impl Default for RsiCore { fn default() -> Self { RsiCore::new() } }

pub fn wilder_rsi(closes: &[f64], period: usize) -> Vec<f64> {
    let n = closes.len();
    if n < 2 || period == 0 { return vec![0.0; n]; }
    let mut gains = vec![0.0_f64; n]; let mut losses = vec![0.0_f64; n];
    for i in 1..n { let d = closes[i] - closes[i - 1]; if d > 0.0 { gains[i] = d; } else { losses[i] = -d; } }
    let mut ag = vec![0.0; n]; let mut al = vec![0.0; n];
    let warmup = period.min(n - 1);
    let (mut sg, mut sl) = (0.0, 0.0);
    for i in 1..=warmup { sg += gains[i]; sl += losses[i]; }
    let p = warmup as f64;
    ag[warmup] = sg / p; al[warmup] = sl / p;
    for i in (warmup + 1)..n {
        ag[i] = ((period as f64 - 1.0) * ag[i - 1] + gains[i]) / period as f64;
        al[i] = ((period as f64 - 1.0) * al[i - 1] + losses[i]) / period as f64;
    }
    let mut rsi = vec![0.0; n];
    for i in warmup..n {
        rsi[i] = if al[i] < 1e-12 { 100.0 } else { let rs = ag[i] / al[i]; 100.0 - 100.0 / (1.0 + rs) };
    }
    rsi
}

impl IndicatorCore for RsiCore {
    type Input = OhlcvSeries;
    type Params = RsiParams;
    type Output = RsiOutput;
    fn name(&self) -> &'static str { "rsi_core" }
    fn version(&self) -> &'static str { "0.1.0" }
    fn warmup_periods(&self, params: &Self::Params) -> usize { params.period * 4 }

    fn compute(&self, input: &Self::Input, params: Self::Params) -> Result<Self::Output> {
        let closes: Vec<f64> = input.bars.iter().map(|b| b.close).collect();
        let rsi = wilder_rsi(&closes, params.period);
        let series: Vec<RsiPoint> = input.bars.iter().zip(rsi.iter()).map(|(b, r)| RsiPoint { date: b.date, value: *r }).collect();
        let mut events = Vec::new();

        // Overbought / Oversold streak
        streak(&series, STREAK_MIN_DAYS, |p| p.value >= params.overbought, RsiEventKind::OverboughtStreak, &mut events);
        streak(&series, STREAK_MIN_DAYS, |p| p.value > 0.0 && p.value <= params.oversold, RsiEventKind::OversoldStreak, &mut events);

        // Exit events
        for i in 1..series.len() {
            if series[i - 1].value >= params.overbought && series[i].value < params.overbought {
                events.push(RsiEvent { date: series[i].date, kind: RsiEventKind::OverboughtExit, value: series[i].value,
                    metadata: json!({"event": "overbought_exit", "date": series[i].date}) });
            }
            if series[i - 1].value <= params.oversold && series[i].value > params.oversold {
                events.push(RsiEvent { date: series[i].date, kind: RsiEventKind::OversoldExit, value: series[i].value,
                    metadata: json!({"event": "oversold_exit", "date": series[i].date}) });
            }
        }

        // Divergence — pivot-based detection(2026-05-12 P5 算法重寫)
        // 原 fixed-20-bar 實作在趨勢中每天觸發 → 20–33 次/股/年 🔴 → 修正後 2–6 次/年 🟢。
        // Verification: scripts/p2_calibration_data.sql §2 (rsi_core / BullishDivergence|BearishDivergence)。
        // Reference: Murphy (1999) "Technical Analysis of the Financial Markets" p.248 —
        //   背離必須比較連續 swing HIGH/LOW 樞軸點，而非固定間距；
        //   Lucas & LeBeau (1992) "Computer Analysis of the Futures Market" — pivot_n=3 局部極值判斷。
        let dates_vec: Vec<NaiveDate> = input.bars.iter().map(|b| b.date).collect();
        for (confirm_date, is_bearish, ind_val, price_val, prev_date, prev_ind) in
            detect_divergences(&closes, &rsi, &dates_vec)
        {
            let kind = if is_bearish { RsiEventKind::BearishDivergence } else { RsiEventKind::BullishDivergence };
            events.push(RsiEvent {
                date: confirm_date, kind, value: ind_val,
                metadata: json!({
                    "event": if is_bearish { "bearish_divergence" } else { "bullish_divergence" },
                    "pivot_price": price_val,
                    "prev_pivot_date": prev_date.to_string(),
                    "prev_indicator": prev_ind,
                }),
            });
        }
        // Failure Swing(2026-05-10 落實作)— 對齊 Wilder (1978) "New Concepts in Technical
        // Trading Systems" §7 RSI Failure Swing 定義:
        //   Bearish FS 四步:RSI ≥ 70 → < 70(local high) → 反彈但 < 70(failure)
        //     → 跌破前 step 2 local low → 觸發 reversal
        //   Bullish FS 對稱:RSI ≤ 30 → > 30 → 反彈失敗 → 突破前 high → 觸發
        detect_failure_swing(&series, params.overbought, params.oversold, &mut events);

        Ok(RsiOutput { stock_id: input.stock_id.clone(), timeframe: params.timeframe, series, events })
    }

    fn produce_facts(&self, output: &Self::Output) -> Vec<Fact> {
        output.events.iter().map(|e| Fact {
            stock_id: output.stock_id.clone(), fact_date: e.date, timeframe: output.timeframe,
            source_core: "rsi_core".to_string(), source_version: "0.1.0".to_string(),
            params_hash: None, statement: format!("RSI {:?} on {}: rsi={:.2}", e.kind, e.date, e.value),
            metadata: e.metadata.clone(),
        }).collect()
    }
}

/// Failure Swing 4-step detection(Wilder 1978)
///
/// 用 state machine 追蹤 RSI 在 overbought / oversold zone 的進出 + reversal:
/// 1. RSI ≥ overbought(進入 overbought 區)
/// 2. RSI < overbought(退出 overbought 區)— 記 step-2 local low
/// 3. RSI 反彈但仍 < overbought(failure to re-enter)— 記 step-3 local high
/// 4. RSI 跌破 step-2 local low → Bearish Failure Swing 觸發
///
/// Bullish FS 對稱(oversold zone),symmetry: ≤ oversold → > → > oversold low high → 突破 step-2 local high
fn detect_failure_swing(series: &[RsiPoint], overbought: f64, oversold: f64, out: &mut Vec<RsiEvent>) {
    // Bearish state machine
    let mut bear_state: u8 = 0; // 0: idle, 1: in OB, 2: exited OB(have local low), 3: had failure rally
    let mut bear_local_low: f64 = 0.0;
    let mut bear_local_low_date: NaiveDate = series.first().map_or(
        NaiveDate::from_ymd_opt(1900, 1, 1).unwrap(), |p| p.date);
    let mut bear_failure_high: f64 = 0.0;
    // Bullish state machine
    let mut bull_state: u8 = 0;
    let mut bull_local_high: f64 = 0.0;
    let mut bull_local_high_date: NaiveDate = bear_local_low_date;
    let mut bull_failure_low: f64 = 0.0;

    for (i, p) in series.iter().enumerate() {
        if p.value <= 0.0 { continue; } // warmup skip

        // ===== Bearish FS =====
        match bear_state {
            0 => { if p.value >= overbought { bear_state = 1; } }
            1 => { if p.value < overbought { bear_state = 2; bear_local_low = p.value; bear_local_low_date = p.date; } }
            2 => {
                if p.value < bear_local_low { bear_local_low = p.value; bear_local_low_date = p.date; }
                else if p.value >= overbought { bear_state = 1; } // 再進 OB,reset 到 step 1
                else if i > 0 && p.value > series[i - 1].value {
                    bear_state = 3; bear_failure_high = p.value;
                }
            }
            3 => {
                if p.value > bear_failure_high { bear_failure_high = p.value; }
                if p.value >= overbought { bear_state = 1; } // 再進 OB,reset
                else if p.value < bear_local_low {
                    // Step 4: Bearish FS confirmed
                    out.push(RsiEvent { date: p.date, kind: RsiEventKind::FailureSwing, value: p.value,
                        metadata: json!({
                            "type": "bearish",
                            "step2_local_low": bear_local_low,
                            "step2_low_date": bear_local_low_date,
                            "step3_failure_high": bear_failure_high,
                        }) });
                    bear_state = 0; // reset
                }
            }
            _ => bear_state = 0,
        }

        // ===== Bullish FS =====
        match bull_state {
            0 => { if p.value > 0.0 && p.value <= oversold { bull_state = 1; } }
            1 => { if p.value > oversold { bull_state = 2; bull_local_high = p.value; bull_local_high_date = p.date; } }
            2 => {
                if p.value > bull_local_high { bull_local_high = p.value; bull_local_high_date = p.date; }
                else if p.value <= oversold && p.value > 0.0 { bull_state = 1; }
                else if i > 0 && p.value < series[i - 1].value {
                    bull_state = 3; bull_failure_low = p.value;
                }
            }
            3 => {
                if p.value < bull_failure_low && p.value > 0.0 { bull_failure_low = p.value; }
                if p.value <= oversold && p.value > 0.0 { bull_state = 1; }
                else if p.value > bull_local_high {
                    // Step 4: Bullish FS confirmed
                    out.push(RsiEvent { date: p.date, kind: RsiEventKind::FailureSwing, value: p.value,
                        metadata: json!({
                            "type": "bullish",
                            "step2_local_high": bull_local_high,
                            "step2_high_date": bull_local_high_date,
                            "step3_failure_low": bull_failure_low,
                        }) });
                    bull_state = 0;
                }
            }
            _ => bull_state = 0,
        }
    }
}

/// Pivot-based divergence detection (shared helper for indicator cores).
/// Reference: Murphy (1999) p.248; Lucas & LeBeau (1992) "Computer Analysis of the Futures Market" pivot_n=3.
/// Returns (confirm_date, is_bearish, indicator_at_pivot, price_at_pivot, prev_pivot_date, prev_indicator).
fn detect_divergences(
    prices: &[f64],
    indicator: &[f64],
    dates: &[NaiveDate],
) -> Vec<(NaiveDate, bool, f64, f64, NaiveDate, f64)> {
    const PIVOT_N: usize = 3;        // Lucas & LeBeau: 3-bar swing confirmation
    // 對齊 spec §3.6:「兩個價格極值點之間時間距離 ≥ N 根 K 棒(預設 N=20)」
    // Murphy (1999) p.248 建議 20-60 intervals;此值對齊 spec 下限 + Murphy 範圍下界。
    const MIN_PIVOT_DIST: usize = 20;
    let n = prices.len();
    if n < PIVOT_N * 2 + MIN_PIVOT_DIST { return Vec::new(); }
    let mut out = Vec::new();
    let mut last_high: Option<(usize, f64, f64)> = None; // (pivot_idx, price, indicator)
    let mut last_low: Option<(usize, f64, f64)> = None;
    for pivot in PIVOT_N..(n - PIVOT_N) {
        let p = prices[pivot]; let ind = indicator[pivot];
        if ind.abs() < 1e-12 { continue; } // skip warmup zeros
        let is_h = (1..=PIVOT_N).all(|k| prices[pivot - k] < p) && (1..=PIVOT_N).all(|k| prices[pivot + k] < p);
        let is_l = (1..=PIVOT_N).all(|k| prices[pivot - k] > p) && (1..=PIVOT_N).all(|k| prices[pivot + k] > p);
        if is_h {
            if let Some((pi, pp, pi_ind)) = last_high {
                if pivot - pi >= MIN_PIVOT_DIST && p > pp && ind < pi_ind {
                    let c = (pivot + PIVOT_N).min(n - 1);
                    out.push((dates[c], true, ind, p, dates[pi], pi_ind));
                }
            }
            last_high = Some((pivot, p, ind));
        }
        if is_l {
            if let Some((pi, pp, pi_ind)) = last_low {
                if pivot - pi >= MIN_PIVOT_DIST && p < pp && ind > pi_ind {
                    let c = (pivot + PIVOT_N).min(n - 1);
                    out.push((dates[c], false, ind, p, dates[pi], pi_ind));
                }
            }
            last_low = Some((pivot, p, ind));
        }
    }
    out
}

fn streak(series: &[RsiPoint], min_days: usize, pred: impl Fn(&RsiPoint) -> bool, kind: RsiEventKind, out: &mut Vec<RsiEvent>) {
    let mut start: Option<usize> = None;
    for (i, p) in series.iter().enumerate() {
        if pred(p) { if start.is_none() { start = Some(i); } }
        else if let Some(s) = start.take() {
            let days = i - s;
            if days >= min_days {
                out.push(RsiEvent { date: series[i - 1].date, kind, value: series[i - 1].value,
                    metadata: json!({"event": format!("{:?}", kind), "days": days, "value": series[i - 1].value}) });
            }
        }
    }
    if let Some(s) = start {
        let days = series.len() - s;
        if days >= min_days && !series.is_empty() {
            let last = series.last().unwrap();
            out.push(RsiEvent { date: last.date, kind, value: last.value,
                metadata: json!({"event": format!("{:?}", kind), "days": days, "value": last.value}) });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn name_warmup() {
        let core = RsiCore::new();
        assert_eq!(core.name(), "rsi_core");
        assert_eq!(core.warmup_periods(&RsiParams::default()), 56);
    }
    #[test]
    fn bearish_divergence_pivot_fires_once() {
        // price makes higher swing high, indicator makes lower swing high → 1 bearish divergence
        // pivots placed ≥ 20 bars apart to satisfy MIN_PIVOT_DIST (spec §3.6 N=20)
        let n = 35usize;
        let d = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
        let dates = vec![d; n];
        let mut prices = vec![90.0_f64; n];
        let mut indicator = vec![50.0_f64; n];
        prices[5] = 100.0; indicator[5] = 70.0;  // first swing high
        prices[28] = 105.0; indicator[28] = 65.0; // price HH, indicator LH → bearish (distance 23)
        let r = detect_divergences(&prices, &indicator, &dates);
        assert_eq!(r.iter().filter(|(_, b, ..)| *b).count(), 1, "bearish divergence fires once");
    }
    #[test]
    fn bullish_divergence_pivot_fires_once() {
        let n = 35usize;
        let d = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
        let dates = vec![d; n];
        let mut prices = vec![100.0_f64; n];
        let mut indicator = vec![50.0_f64; n];
        prices[5] = 80.0; indicator[5] = 30.0;   // first swing low
        prices[28] = 75.0; indicator[28] = 35.0;  // price LL, indicator HL → bullish (distance 23)
        let r = detect_divergences(&prices, &indicator, &dates);
        assert_eq!(r.iter().filter(|(_, b, ..)| !b).count(), 1, "bullish divergence fires once");
    }
    #[test]
    fn no_divergence_in_monotone_trend() {
        // monotone price rise + monotone indicator fall: old code fired ~20× per bar;
        // pivot detection fires 0 (no swing pivots in monotone series)
        let n = 50usize;
        let d = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
        let dates = vec![d; n];
        let prices: Vec<f64> = (0..n).map(|i| 100.0 + i as f64).collect();
        let indicator: Vec<f64> = (0..n).map(|i| 70.0 - 0.5 * i as f64).collect();
        let r = detect_divergences(&prices, &indicator, &dates);
        assert_eq!(r.len(), 0, "monotone trend: no pivot highs → 0 divergences");
    }
    #[test]
    fn wilder_steady_uptrend_100() {
        let closes: Vec<f64> = (0..30).map(|i| 100.0 + i as f64).collect();
        assert!((wilder_rsi(&closes, 14)[20] - 100.0).abs() < 1e-9);
    }

    /// Bearish FailureSwing(Wilder 1978):RSI 進 OB → 退出 → 反彈 fail → 跌破前低
    #[test]
    fn bearish_failure_swing_detected() {
        // 構造 RSI 序列(直接 mock RSI value,跳過 close → RSI 計算)
        // 4 步:75 (in OB) → 65 (exit, local_low=65) → 68 (failure rally < 70) → 60 (break local_low)
        let series = vec![
            RsiPoint { date: NaiveDate::from_ymd_opt(2026, 4, 1).unwrap(), value: 75.0 }, // step 1
            RsiPoint { date: NaiveDate::from_ymd_opt(2026, 4, 2).unwrap(), value: 65.0 }, // step 2
            RsiPoint { date: NaiveDate::from_ymd_opt(2026, 4, 3).unwrap(), value: 68.0 }, // step 3 failure rally
            RsiPoint { date: NaiveDate::from_ymd_opt(2026, 4, 4).unwrap(), value: 60.0 }, // step 4 break low
        ];
        let mut events = Vec::new();
        detect_failure_swing(&series, 70.0, 30.0, &mut events);
        let fs = events.iter().filter(|e| e.kind == RsiEventKind::FailureSwing).count();
        assert_eq!(fs, 1, "Bearish FailureSwing 應觸發 1 次");
        let ev = events.iter().find(|e| e.kind == RsiEventKind::FailureSwing).unwrap();
        assert_eq!(ev.metadata["type"], "bearish");
    }

    /// Bullish FailureSwing 對稱
    #[test]
    fn bullish_failure_swing_detected() {
        let series = vec![
            RsiPoint { date: NaiveDate::from_ymd_opt(2026, 4, 1).unwrap(), value: 25.0 }, // step 1 in OS
            RsiPoint { date: NaiveDate::from_ymd_opt(2026, 4, 2).unwrap(), value: 35.0 }, // step 2 exit, local_high=35
            RsiPoint { date: NaiveDate::from_ymd_opt(2026, 4, 3).unwrap(), value: 32.0 }, // step 3 failure dip > 30
            RsiPoint { date: NaiveDate::from_ymd_opt(2026, 4, 4).unwrap(), value: 40.0 }, // step 4 break high
        ];
        let mut events = Vec::new();
        detect_failure_swing(&series, 70.0, 30.0, &mut events);
        let fs = events.iter().filter(|e| e.kind == RsiEventKind::FailureSwing).count();
        assert_eq!(fs, 1, "Bullish FailureSwing 應觸發 1 次");
        let ev = events.iter().find(|e| e.kind == RsiEventKind::FailureSwing).unwrap();
        assert_eq!(ev.metadata["type"], "bullish");
    }
}
