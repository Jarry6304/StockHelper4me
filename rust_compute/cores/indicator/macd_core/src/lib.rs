// macd_core(P1)— Indicator Core
// 對齊 m2Spec/oldm2Spec/indicator_cores_momentum.md §三 macd_core(spec r2)
// Output §3.4(僅 series)+ Fact §3.5(5 種)+ Divergence §3.6(嚴格規則式)

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

/// 背離規則:兩極值點間至少 N 根 K 棒(spec §3.6 預設 20,寫死)
const DIVERGENCE_MIN_BARS: usize = 20;

#[derive(Debug, Clone, Serialize)]
pub struct MacdParams {
    pub fast: usize,
    pub slow: usize,
    pub signal: usize,
    pub timeframe: Timeframe,
}
impl Default for MacdParams { fn default() -> Self { Self { fast: 12, slow: 26, signal: 9, timeframe: Timeframe::Daily } } }

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
        // GoldenCross / DeathCross + HistogramZeroCross
        for i in 1..series.len() {
            let prev_above = series[i - 1].macd_line > series[i - 1].signal_line;
            let cur_above = series[i].macd_line > series[i].signal_line;
            if !prev_above && cur_above {
                events.push(MacdEvent { date: series[i].date, kind: MacdEventKind::GoldenCross, value: series[i].macd_line,
                    metadata: json!({"event": "golden_cross", "macd": series[i].macd_line, "signal": series[i].signal_line}) });
            } else if prev_above && !cur_above {
                events.push(MacdEvent { date: series[i].date, kind: MacdEventKind::DeathCross, value: series[i].macd_line,
                    metadata: json!({"event": "death_cross", "macd": series[i].macd_line, "signal": series[i].signal_line}) });
            }
            // Histogram zero cross
            if series[i - 1].histogram.signum() != series[i].histogram.signum() && series[i].histogram != 0.0 {
                let dir = if series[i].histogram > 0.0 { "positive" } else { "negative" };
                events.push(MacdEvent { date: series[i].date, kind: MacdEventKind::HistogramZeroCross, value: series[i].histogram,
                    metadata: json!({"event": "histogram_zero_cross", "direction": dir}) });
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
        // Divergence(嚴格 §3.6:兩極值點 ≥ DIVERGENCE_MIN_BARS K 棒,price/MACD 反向)
        // best-guess:price HH but MACD LH → bearish_divergence;反之 bullish
        let n = series.len();
        if n > DIVERGENCE_MIN_BARS * 2 {
            for i in DIVERGENCE_MIN_BARS..n {
                let prev_idx = i.saturating_sub(DIVERGENCE_MIN_BARS);
                let price_now = closes[i];
                let price_prev = closes[prev_idx];
                let macd_now = series[i].macd_line;
                let macd_prev = series[prev_idx].macd_line;
                if price_now > price_prev && macd_now < macd_prev {
                    events.push(MacdEvent { date: series[i].date, kind: MacdEventKind::BearishDivergence, value: macd_now,
                        metadata: json!({"event": "bearish_divergence", "price_date": series[prev_idx].date, "indicator_date": series[i].date}) });
                } else if price_now < price_prev && macd_now > macd_prev {
                    events.push(MacdEvent { date: series[i].date, kind: MacdEventKind::BullishDivergence, value: macd_now,
                        metadata: json!({"event": "bullish_divergence", "price_date": series[prev_idx].date, "indicator_date": series[i].date}) });
                }
            }
        }
        Ok(MacdOutput { stock_id: input.stock_id.clone(), timeframe: params.timeframe, series, events })
    }

    fn produce_facts(&self, output: &Self::Output) -> Vec<Fact> {
        output.events.iter().map(|e| Fact {
            stock_id: output.stock_id.clone(), fact_date: e.date, timeframe: output.timeframe,
            source_core: "macd_core".to_string(), source_version: "0.1.0".to_string(),
            params_hash: None, statement: format!("MACD {:?} on {}: macd={:.4}", e.kind, e.date, e.value),
            metadata: e.metadata.clone(),
        }).collect()
    }
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
}
