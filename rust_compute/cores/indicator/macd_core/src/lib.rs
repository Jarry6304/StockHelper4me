// macd_core(P1)— Indicator Core
// 對齊 m3Spec/indicator_cores_momentum.md §三 macd_core(spec r4)
// Output §3.4(僅 series)+ Fact §3.5(5 種)+ Divergence §3.6(嚴格規則式)
//
// **Reference(2026-05-10 加 / 2026-05-13 校驗)**:
//   fast=12 / slow=26 / signal=9:Appel, Gerald (1979).
//                                  "The Moving Average Convergence Divergence Method"
//                                  原作者設計,12/26 對應 2 月/4 月 EMA(原始月線分析)
//   MIN_PIVOT_DIST=12:NEoWave 經驗值(2026-05-14 P0 Gate v4 校準後)。原 spec §3.6 預設 20
//                      過於保守 → Divergence 0.27-0.33/yr 低於 Murphy 1-4/yr 下限 3× →
//                      讓步至 12-bar 距離(spec §3.6「N」結構性條件 ≥ 2 × PIVOT_N = 6 仍滿足)

use anyhow::Result;
use chrono::NaiveDate;
use fact_schema::{Fact, IndicatorCore, Timeframe};
use ohlcv_loader::OhlcvSeries;
use serde::Serialize;
use serde_json::json;

inventory::submit! {
    core_registry::CoreRegistration::new(
        "macd_core", "0.1.0", core_registry::CoreKind::Indicator, "P1",
        "MACD Core(12/26/9 — 5 種事件)",
    )
}


#[derive(Debug, Clone, Serialize)]
pub struct MacdParams {
    pub fast: usize,
    pub slow: usize,
    pub signal: usize,
    pub timeframe: Timeframe,
}
impl Default for MacdParams { fn default() -> Self { Self { fast: 12, slow: 26, signal: 9, timeframe: Timeframe::Daily } } }

/// HistogramZeroCross 最小間距 — 防止 histogram 在 0 附近快速來回產生噪音。
///
/// **校準歷史**:
/// - 2026-05-12 v1.32:從無 spacing → 5(目標 ≤ 12/yr)
/// - 2026-05-14 P0 Gate v3(1264 stocks production):觀察 HistogramZeroCross 19.01/yr,
///   仍 1.6× 超標 ≤ 12/yr 目標 → 升至 **8**(預期 ×0.625 降至 ~12/yr,落入目標範圍上沿)
///
/// Verification: docs/benchmarks/neely_p0_gate_results_v3_2026-05-14.md §N + scripts/p2_calibration_data.sql §2
/// 8-bar ≈ 1.5 週,短於 kd_core(15-bar)因 MACD histogram 本質波動更平滑。
const MIN_ZERO_CROSS_SPACING: usize = 8;

/// GoldenCross / DeathCross 最小間距。
///
/// **校準歷史**:
/// - 2026-05-12 v1.32:從無 spacing → 10(目標 5-7/yr,防止快速 whipsaw)
/// - 2026-05-14 P0 Gate v3(1264 stocks production):觀察 GoldenCross 9.58/yr / DeathCross 9.42/yr,
///   仍 1.4× 超標 5-7/yr 目標 → 升至 **15**(預期 ×0.66 降至 ~6.4/yr,落入目標範圍中段)
///
/// Verification: docs/benchmarks/neely_p0_gate_results_v3_2026-05-14.md §N + scripts/p2_calibration_data.sql §2
/// 15-bar = 3 週,與 kd_core MIN_KD_CROSS_SPACING 對齊(同屬 MACD-family cross events)。
const MIN_MACD_CROSS_SPACING: usize = 15;

#[derive(Debug, Clone, Serialize)]
pub struct MacdOutput {
    pub stock_id: String,
    pub timeframe: Timeframe,
    pub series: Vec<MacdPoint>,
    /// 內部 events,不寫進 indicator_values JSONB(對齊 spec §3.4 僅 series)
    /// produce_facts() 從這裡讀
    #[serde(skip)]
    pub events: Vec<MacdEvent>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MacdPoint {
    pub date: NaiveDate,
    pub macd_line: f64,
    pub signal_line: f64,
    pub histogram: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct MacdEvent { pub date: NaiveDate, pub kind: MacdEventKind, pub value: f64, pub metadata: serde_json::Value }
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum MacdEventKind { GoldenCross, DeathCross, HistogramExpansion, BearishDivergence, BullishDivergence, HistogramZeroCross }

pub struct MacdCore;
impl MacdCore { pub fn new() -> Self { MacdCore } }
impl Default for MacdCore { fn default() -> Self { MacdCore::new() } }

fn ema(values: &[f64], period: usize) -> Vec<f64> {
    let mut out = vec![0.0; values.len()];
    if values.is_empty() || period == 0 { return out; }
    let alpha = 2.0 / (period as f64 + 1.0);
    out[0] = values[0];
    for i in 1..values.len() { out[i] = alpha * values[i] + (1.0 - alpha) * out[i - 1]; }
    out
}

impl IndicatorCore for MacdCore {
    type Input = OhlcvSeries;
    type Params = MacdParams;
    type Output = MacdOutput;
    fn name(&self) -> &'static str { "macd_core" }
    fn version(&self) -> &'static str { "0.1.0" }
    /// §3.3:slow * 4
    fn warmup_periods(&self, params: &Self::Params) -> usize { params.slow * 4 }

    fn compute(&self, input: &Self::Input, params: Self::Params) -> Result<Self::Output> {
        let closes: Vec<f64> = input.bars.iter().map(|b| b.close).collect();
        let ema_fast = ema(&closes, params.fast);
        let ema_slow = ema(&closes, params.slow);
        let macd_line: Vec<f64> = (0..closes.len()).map(|i| ema_fast[i] - ema_slow[i]).collect();
        let signal_line = ema(&macd_line, params.signal);
        let series: Vec<MacdPoint> = (0..closes.len()).map(|i| MacdPoint {
            date: input.bars[i].date, macd_line: macd_line[i], signal_line: signal_line[i],
            histogram: macd_line[i] - signal_line[i],
        }).collect();
        let mut events = Vec::new();
        // GoldenCross / DeathCross + HistogramZeroCross — with minimum spacing to suppress whipsaw
        let mut last_golden_i: Option<usize> = None;
        let mut last_death_i: Option<usize> = None;
        let mut last_zero_cross_i: Option<usize> = None;
        for i in 1..series.len() {
            let prev_above = series[i - 1].macd_line > series[i - 1].signal_line;
            let cur_above = series[i].macd_line > series[i].signal_line;
            if !prev_above && cur_above {
                if last_golden_i.map_or(true, |li| i - li >= MIN_MACD_CROSS_SPACING) {
                    events.push(MacdEvent { date: series[i].date, kind: MacdEventKind::GoldenCross, value: series[i].macd_line,
                        metadata: json!({"event": "golden_cross", "macd": series[i].macd_line, "signal": series[i].signal_line}) });
                    last_golden_i = Some(i);
                }
            } else if prev_above && !cur_above {
                if last_death_i.map_or(true, |li| i - li >= MIN_MACD_CROSS_SPACING) {
                    events.push(MacdEvent { date: series[i].date, kind: MacdEventKind::DeathCross, value: series[i].macd_line,
                        metadata: json!({"event": "death_cross", "macd": series[i].macd_line, "signal": series[i].signal_line}) });
                    last_death_i = Some(i);
                }
            }
            // Histogram zero cross — minimum spacing suppresses rapid oscillation around 0
            if series[i - 1].histogram.signum() != series[i].histogram.signum() && series[i].histogram != 0.0 {
                if last_zero_cross_i.map_or(true, |li| i - li >= MIN_ZERO_CROSS_SPACING) {
                    let dir = if series[i].histogram > 0.0 { "positive" } else { "negative" };
                    events.push(MacdEvent { date: series[i].date, kind: MacdEventKind::HistogramZeroCross, value: series[i].histogram,
                        metadata: json!({"event": "histogram_zero_cross", "direction": dir}) });
                    last_zero_cross_i = Some(i);
                }
            }
        }
        // HistogramExpansion(連續 |histogram| 增大)
        let mut exp_count = 0;
        for i in 1..series.len() {
            if series[i].histogram.abs() > series[i - 1].histogram.abs() && series[i].histogram.signum() == series[i - 1].histogram.signum() {
                exp_count += 1;
            } else {
                if exp_count >= 5 { // 5 根以上才視為 expansion
                    events.push(MacdEvent { date: series[i - 1].date, kind: MacdEventKind::HistogramExpansion, value: exp_count as f64,
                        metadata: json!({"event": "histogram_expansion", "bars": exp_count, "end_date": series[i - 1].date}) });
                }
                exp_count = 0;
            }
        }
        // Divergence — pivot-based detection(2026-05-12 P5 算法重寫)
        // 原 fixed-20-bar 每天比較 → 20–33 次/年 🔴 → Pivot 版 2–6 次/年 🟢。
        // Verification: scripts/p2_calibration_data.sql §2 (macd_core / BullishDivergence|BearishDivergence)。
        // Reference: Murphy (1999) p.248; Lucas & LeBeau (1992) "Computer Analysis of the Futures Market" pivot_n=3。
        let macd_vals: Vec<f64> = series.iter().map(|p| p.macd_line).collect();
        let dates_vec: Vec<NaiveDate> = input.bars.iter().map(|b| b.date).collect();
        for (confirm_date, is_bearish, ind_val, price_val, prev_date, prev_ind) in
            detect_divergences(&closes, &macd_vals, &dates_vec)
        {
            let kind = if is_bearish { MacdEventKind::BearishDivergence } else { MacdEventKind::BullishDivergence };
            events.push(MacdEvent {
                date: confirm_date, kind, value: ind_val,
                metadata: json!({
                    "event": if is_bearish { "bearish_divergence" } else { "bullish_divergence" },
                    "pivot_price": price_val,
                    "prev_pivot_date": prev_date.to_string(),
                    "prev_macd": prev_ind,
                }),
            });
        }
        Ok(MacdOutput { stock_id: input.stock_id.clone(), timeframe: params.timeframe, series, events })
    }

    fn produce_facts(&self, output: &Self::Output) -> Vec<Fact> {
        output.events.iter().map(|e| Fact {
            stock_id: output.stock_id.clone(), fact_date: e.date, timeframe: output.timeframe,
            source_core: "macd_core".to_string(), source_version: "0.1.0".to_string(),
            params_hash: None, statement: format!("MACD {:?} on {}: macd={:.4}", e.kind, e.date, e.value),
            metadata: fact_schema::with_event_kind(e.metadata.clone(), &e.kind),
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
    // **校準歷史**:
    // - 2026-05-12 v1.32 P5 algorithm rewrite:fixed-20-bar window → pivot-based,MIN=10
    // - 2026-05-13 v1.33:10 → 20 對齊 spec §3.6 預設值
    // - 2026-05-14 P0 Gate v4 production 1264 stocks:Divergence 0.27-0.33/yr 低於
    //   Murphy (1999) p.248 預期 1-4/yr 下限 3× → 升 20 → **12** 讓步至 Murphy 下限
    //   預期 events_per_stock_per_year × 2.5 → ~0.7-0.8/yr,接近 Murphy 1/yr 下限。
    //   12 ≥ 2× PIVOT_N(=6),仍符 spec §3.6 結構性要求(N=12 為 NEoWave 經驗值)。
    const MIN_PIVOT_DIST: usize = 12;
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn name_warmup() {
        let core = MacdCore::new();
        assert_eq!(core.name(), "macd_core");
        assert_eq!(core.warmup_periods(&MacdParams::default()), 26 * 4);
    }
    #[test]
    fn macd_spacing_constants() {
        assert_eq!(MIN_ZERO_CROSS_SPACING, 8);  // P0 Gate v3 校準 2026-05-14:5 → 8
        assert_eq!(MIN_MACD_CROSS_SPACING, 15); // P0 Gate v3 校準 2026-05-14:10 → 15
    }

    #[test]
    fn macd_bearish_divergence_fires_once() {
        // pivots placed ≥ 20 bars apart, well above MIN_PIVOT_DIST=12 (P0 Gate v4 校準後)
        let n = 35usize;
        let d = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
        let dates = vec![d; n];
        let mut prices = vec![90.0_f64; n];
        let mut indicator = vec![1.0_f64; n]; // non-zero background
        prices[5] = 100.0; indicator[5] = 5.0;
        prices[28] = 105.0; indicator[28] = 3.0; // price HH, macd LH → bearish (distance 23)
        let r = detect_divergences(&prices, &indicator, &dates);
        assert_eq!(r.iter().filter(|(_, b, ..)| *b).count(), 1);
    }
    #[test]
    fn macd_no_divergence_monotone() {
        let n = 50usize;
        let d = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
        let dates = vec![d; n];
        let prices: Vec<f64> = (0..n).map(|i| 100.0 + i as f64).collect();
        let indicator: Vec<f64> = (0..n).map(|i| 5.0 - 0.1 * i as f64).collect();
        assert_eq!(detect_divergences(&prices, &indicator, &dates).len(), 0);
    }
}
