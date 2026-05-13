// kd_core(P1)— 對齊 m2Spec/oldm2Spec/indicator_cores_momentum.md §五 r2
// Params §5.2(period 9 / k_smooth 3 / d_smooth 3)/ Output §5.4(僅 k/d)/ warmup §5.3
//
// **Reference(2026-05-10 加)**:
//   period=9:**Asian markets convention / KDJ variant**(George Lane 1957 原版用 14;
//             9 days 是 Asian / 台股慣例,對應 short-term day trader 設定)。
//             ⚠️ 沒明確學術 cite,Phase 2 可考慮對齊國際標準 14
//   k_smooth=3 / d_smooth=3:Lane (1957) 原版 slow stochastic 14/3/3 的 smooth 參數
//   overbought=80 / oversold=20:KDJ 台股慣例(對比 RSI 70/30,KD 走較極端)

use anyhow::Result;
use chrono::NaiveDate;
use fact_schema::{Fact, IndicatorCore, Timeframe};
use ohlcv_loader::OhlcvSeries;
use serde::Serialize;
use serde_json::json;

inventory::submit! {
    core_registry::CoreRegistration::new(
        "kd_core", "0.1.0", core_registry::CoreKind::Indicator, "P1",
        "KD Core(Stochastic %K %D)— 台股慣例 9/3/3",
    )
}

/// Reference(2026-05-12 校準): 同 rsi_core。Wilder (1978) 未指定連續天數；
/// Connors (2008) ConnorsRSI 3-consecutive 提供間接實務支持。
const STREAK_MIN_DAYS: usize = 3;

/// GoldenCross / DeathCross 最小間距(防止短周期 KD 快速來回的 whipsaw 噪音)。
/// Production data 校準(2026-05-12): GoldenCross 17/yr 🟠 → 目標 8–12/yr 🟢。
/// Verification: scripts/p2_calibration_data.sql §2 (kd_core / GoldenCross|DeathCross)。
/// 10-bar = 2 週,排除 < 2 週的反轉視為雜訊。KD period=9 × MIN_SPACING=10 ≈ 1.1 個完整周期。
const MIN_KD_CROSS_SPACING: usize = 10;

#[derive(Debug, Clone, Serialize)]
pub struct KdParams {
    pub period: usize,    // 預設 9(台股 §5.6)
    pub k_smooth: usize,  // 預設 3
    pub d_smooth: usize,  // 預設 3
    pub overbought: f64,  // 預設 80.0
    pub oversold: f64,    // 預設 20.0
    pub timeframe: Timeframe,
}
impl Default for KdParams { fn default() -> Self { Self { period: 9, k_smooth: 3, d_smooth: 3, overbought: 80.0, oversold: 20.0, timeframe: Timeframe::Daily } } }

#[derive(Debug, Clone, Serialize)]
pub struct KdOutput {
    pub stock_id: String, pub timeframe: Timeframe,
    pub series: Vec<KdPoint>,
    #[serde(skip)]
    pub events: Vec<KdEvent>,
}
#[derive(Debug, Clone, Serialize)]
pub struct KdPoint { pub date: NaiveDate, pub k: f64, pub d: f64 }
#[derive(Debug, Clone, Serialize)]
pub struct KdEvent { pub date: NaiveDate, pub kind: KdEventKind, pub value: f64, pub metadata: serde_json::Value }
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum KdEventKind { GoldenCross, DeathCross, OverboughtStreak, OversoldStreak, BearishDivergence, BullishDivergence }

pub struct KdCore;
impl KdCore { pub fn new() -> Self { KdCore } }
impl Default for KdCore { fn default() -> Self { KdCore::new() } }

impl IndicatorCore for KdCore {
    type Input = OhlcvSeries;
    type Params = KdParams;
    type Output = KdOutput;
    fn name(&self) -> &'static str { "kd_core" }
    fn version(&self) -> &'static str { "0.1.0" }
    /// §5.3:`period + k_smooth + d_smooth + 10`
    fn warmup_periods(&self, params: &Self::Params) -> usize {
        params.period + params.k_smooth + params.d_smooth + 10
    }

    fn compute(&self, input: &Self::Input, params: Self::Params) -> Result<Self::Output> {
        let n = input.bars.len();
        let mut series = Vec::with_capacity(n);
        // 台股慣例:K = (k_smooth-1)/k_smooth × prev_K + 1/k_smooth × RSV
        // 同 D。預設 k_smooth = 3 → 2/3 prev + 1/3 cur(對齊台股 KD 慣例)
        let mut prev_k = 50.0_f64; let mut prev_d = 50.0_f64;
        for i in 0..n {
            let p = params.period.min(i + 1);
            let start = i + 1 - p;
            let lo = input.bars[start..=i].iter().map(|b| b.low).fold(f64::INFINITY, f64::min);
            let hi = input.bars[start..=i].iter().map(|b| b.high).fold(f64::NEG_INFINITY, f64::max);
            let close = input.bars[i].close;
            let rsv = if hi - lo > 1e-9 { (close - lo) / (hi - lo) * 100.0 } else { 50.0 };
            let ks = params.k_smooth as f64;
            let ds = params.d_smooth as f64;
            let k = ((ks - 1.0) / ks) * prev_k + (1.0 / ks) * rsv;
            let d = ((ds - 1.0) / ds) * prev_d + (1.0 / ds) * k;
            series.push(KdPoint { date: input.bars[i].date, k, d });
            prev_k = k; prev_d = d;
        }
        let mut events = Vec::new();
        let mut last_golden_i: Option<usize> = None;
        let mut last_death_i: Option<usize> = None;
        for i in 1..series.len() {
            let prev_above = series[i - 1].k > series[i - 1].d;
            let cur_above = series[i].k > series[i].d;
            if !prev_above && cur_above {
                if last_golden_i.map_or(true, |li| i - li >= MIN_KD_CROSS_SPACING) {
                    events.push(KdEvent { date: series[i].date, kind: KdEventKind::GoldenCross, value: series[i].k,
                        metadata: json!({"event": "golden_cross", "k": series[i].k, "d": series[i].d}) });
                    last_golden_i = Some(i);
                }
            } else if prev_above && !cur_above {
                if last_death_i.map_or(true, |li| i - li >= MIN_KD_CROSS_SPACING) {
                    events.push(KdEvent { date: series[i].date, kind: KdEventKind::DeathCross, value: series[i].k,
                        metadata: json!({"event": "death_cross", "k": series[i].k, "d": series[i].d}) });
                    last_death_i = Some(i);
                }
            }
        }
        // streaks
        kd_streak(&series, STREAK_MIN_DAYS, |p| p.k >= params.overbought, KdEventKind::OverboughtStreak, &mut events);
        kd_streak(&series, STREAK_MIN_DAYS, |p| p.k <= params.oversold, KdEventKind::OversoldStreak, &mut events);
        // Divergence — pivot-based detection(2026-05-12 P5 算法重寫)
        // 原 fixed-20-bar 每天比較 → 20–33 次/年 🔴 → Pivot 版 2–6 次/年 🟢。
        // Verification: scripts/p2_calibration_data.sql §2 (kd_core / BullishDivergence|BearishDivergence)。
        // Reference: Murphy (1999) p.248; Lucas & LeBeau (1992) "Computer Analysis of the Futures Market" pivot_n=3。
        let closes: Vec<f64> = input.bars.iter().map(|b| b.close).collect();
        let k_vals: Vec<f64> = series.iter().map(|p| p.k).collect();
        let dates_vec: Vec<NaiveDate> = input.bars.iter().map(|b| b.date).collect();
        for (confirm_date, is_bearish, ind_val, price_val, prev_date, prev_ind) in
            detect_divergences(&closes, &k_vals, &dates_vec)
        {
            let kind = if is_bearish { KdEventKind::BearishDivergence } else { KdEventKind::BullishDivergence };
            events.push(KdEvent {
                date: confirm_date, kind, value: ind_val,
                metadata: json!({
                    "event": if is_bearish { "bearish_divergence" } else { "bullish_divergence" },
                    "pivot_price": price_val,
                    "prev_pivot_date": prev_date.to_string(),
                    "prev_k": prev_ind,
                }),
            });
        }
        Ok(KdOutput { stock_id: input.stock_id.clone(), timeframe: params.timeframe, series, events })
    }

    fn produce_facts(&self, output: &Self::Output) -> Vec<Fact> {
        output.events.iter().map(|e| Fact {
            stock_id: output.stock_id.clone(), fact_date: e.date, timeframe: output.timeframe,
            source_core: "kd_core".to_string(), source_version: "0.1.0".to_string(),
            params_hash: None, statement: format!("KD {:?} on {}: k={:.2}", e.kind, e.date, e.value),
            metadata: e.metadata.clone(),
        }).collect()
    }
}

/// Pivot-based divergence detection. Reference: Murphy (1999) p.248; Lucas & LeBeau (1992) pivot_n=3.
fn detect_divergences(
    prices: &[f64],
    indicator: &[f64],
    dates: &[NaiveDate],
) -> Vec<(NaiveDate, bool, f64, f64, NaiveDate, f64)> {
    const PIVOT_N: usize = 3;
    // 對齊 spec(indicator_cores_momentum.md §3.6):兩極值點距離 ≥ N=20。
    const MIN_PIVOT_DIST: usize = 20;
    let n = prices.len();
    if n < PIVOT_N * 2 + MIN_PIVOT_DIST { return Vec::new(); }
    let mut out = Vec::new();
    let mut last_high: Option<(usize, f64, f64)> = None;
    let mut last_low: Option<(usize, f64, f64)> = None;
    for pivot in PIVOT_N..(n - PIVOT_N) {
        let p = prices[pivot]; let ind = indicator[pivot];
        if ind.abs() < 1e-12 { continue; }
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

fn kd_streak(series: &[KdPoint], min_days: usize, pred: impl Fn(&KdPoint) -> bool, kind: KdEventKind, out: &mut Vec<KdEvent>) {
    let mut start: Option<usize> = None;
    for (i, p) in series.iter().enumerate() {
        if pred(p) { if start.is_none() { start = Some(i); } }
        else if let Some(s) = start.take() {
            if i - s >= min_days {
                out.push(KdEvent { date: series[i - 1].date, kind, value: series[i - 1].k,
                    metadata: json!({"days": i - s, "k": series[i - 1].k}) });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn name_warmup() {
        let core = KdCore::new();
        assert_eq!(core.name(), "kd_core");
        assert_eq!(core.warmup_periods(&KdParams::default()), 9 + 3 + 3 + 10);
    }
    #[test]
    fn kd_bearish_divergence_fires_once() {
        // pivots placed ≥ 20 bars apart to satisfy MIN_PIVOT_DIST (spec §3.6 N=20)
        let n = 35usize;
        let d = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
        let dates = vec![d; n];
        let mut prices = vec![90.0_f64; n];
        let mut indicator = vec![50.0_f64; n];
        prices[5] = 100.0; indicator[5] = 80.0;
        prices[28] = 105.0; indicator[28] = 75.0; // price HH, indicator LH (distance 23)
        let r = detect_divergences(&prices, &indicator, &dates);
        assert_eq!(r.iter().filter(|(_, b, ..)| *b).count(), 1);
    }
    #[test]
    fn kd_cross_spacing_constant_is_10() {
        assert_eq!(MIN_KD_CROSS_SPACING, 10);
    }

    #[test]
    fn kd_no_divergence_monotone() {
        let n = 50usize;
        let d = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
        let dates = vec![d; n];
        let prices: Vec<f64> = (0..n).map(|i| 100.0 + i as f64).collect();
        let indicator: Vec<f64> = (0..n).map(|i| 80.0 - 0.5 * i as f64).collect();
        assert_eq!(detect_divergences(&prices, &indicator, &dates).len(), 0);
    }
}
