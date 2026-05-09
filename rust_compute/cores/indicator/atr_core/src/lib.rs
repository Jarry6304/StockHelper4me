// atr_core(P1)— Indicator Core(波動率)
//
// 對齊 oldm2Spec/indicator_cores_volatility.md(spec user m3Spec 待寫)。
// Wilder ATR 與 neely_core::monowave::pure_close::compute_atr_series 同算法,
// 但本 core 是獨立 Indicator(輸出 ATR 數值序列 + 異常事件)。

use anyhow::Result;
use chrono::NaiveDate;
use fact_schema::{Fact, IndicatorCore, Timeframe};
use ohlcv_loader::OhlcvSeries;
use serde::Serialize;
use serde_json::json;

inventory::submit! {
    core_registry::CoreRegistration::new(
        "atr_core", "0.1.0", core_registry::CoreKind::Indicator, "P1",
        "ATR Core(Wilder ATR + 波動率異常)",
    )
}

#[derive(Debug, Clone, Serialize)]
pub struct AtrParams { pub timeframe: Timeframe, pub period: usize, pub spike_multiplier: f64 }
impl Default for AtrParams { fn default() -> Self { Self { timeframe: Timeframe::Daily, period: 14, spike_multiplier: 2.0 } } }

#[derive(Debug, Clone, Serialize)]
pub struct AtrOutput { pub stock_id: String, pub timeframe: Timeframe, pub series: Vec<AtrPoint>, pub events: Vec<AtrEvent> }
#[derive(Debug, Clone, Serialize)]
pub struct AtrPoint { pub date: NaiveDate, pub atr: f64 }
#[derive(Debug, Clone, Serialize)]
pub struct AtrEvent { pub date: NaiveDate, pub kind: AtrEventKind, pub value: f64, pub metadata: serde_json::Value }
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum AtrEventKind { Spike, Compression }

pub struct AtrCore;
impl AtrCore { pub fn new() -> Self { AtrCore } }
impl Default for AtrCore { fn default() -> Self { AtrCore::new() } }

pub fn wilder_atr(bars: &[ohlcv_loader::OhlcvBar], period: usize) -> Vec<f64> {
    let n = bars.len();
    if n == 0 || period == 0 { return vec![0.0; n]; }
    let mut tr = Vec::with_capacity(n);
    tr.push(bars[0].high - bars[0].low);
    for i in 1..n {
        let prev = bars[i - 1].close;
        let h = bars[i].high; let l = bars[i].low;
        let candidate = [(h - l).abs(), (h - prev).abs(), (l - prev).abs()];
        tr.push(candidate.iter().cloned().fold(0.0_f64, f64::max));
    }
    let mut atr = vec![0.0; n];
    let warmup = period.min(n);
    let mut sum = 0.0;
    for i in 0..warmup { sum += tr[i]; atr[i] = sum / (i + 1) as f64; }
    for i in warmup..n {
        atr[i] = ((period as f64 - 1.0) * atr[i - 1] + tr[i]) / period as f64;
    }
    atr
}

impl IndicatorCore for AtrCore {
    type Input = OhlcvSeries;
    type Params = AtrParams;
    type Output = AtrOutput;
    fn name(&self) -> &'static str { "atr_core" }
    fn version(&self) -> &'static str { "0.1.0" }
    fn warmup_periods(&self, params: &Self::Params) -> usize { params.period * 4 }

    fn compute(&self, input: &Self::Input, params: Self::Params) -> Result<Self::Output> {
        let atrs = wilder_atr(&input.bars, params.period);
        let series: Vec<AtrPoint> = input.bars.iter().zip(atrs.iter()).map(|(b, a)| AtrPoint { date: b.date, atr: *a }).collect();
        // Events:當前 ATR > 過去 N 天均 ATR × spike_multiplier → Spike
        let mut events = Vec::new();
        let lookback = params.period * 2;
        for i in lookback..series.len() {
            let prev_avg: f64 = series[i - lookback..i].iter().map(|p| p.atr).sum::<f64>() / lookback as f64;
            if prev_avg > 0.0 {
                let r = series[i].atr / prev_avg;
                if r >= params.spike_multiplier {
                    events.push(AtrEvent { date: series[i].date, kind: AtrEventKind::Spike, value: r, metadata: json!({"atr": series[i].atr, "prev_avg": prev_avg, "ratio": r}) });
                } else if r < 0.5 {
                    events.push(AtrEvent { date: series[i].date, kind: AtrEventKind::Compression, value: r, metadata: json!({"atr": series[i].atr, "prev_avg": prev_avg, "ratio": r}) });
                }
            }
        }
        Ok(AtrOutput { stock_id: input.stock_id.clone(), timeframe: params.timeframe, series, events })
    }

    fn produce_facts(&self, output: &Self::Output) -> Vec<Fact> {
        output.events.iter().map(|e| Fact {
            stock_id: output.stock_id.clone(), fact_date: e.date, timeframe: output.timeframe,
            source_core: "atr_core".to_string(), source_version: "0.1.0".to_string(),
            params_hash: None,
            statement: format!("ATR {:?} on {}: ratio={:.2}", e.kind, e.date, e.value),
            metadata: e.metadata.clone(),
        }).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ohlcv_loader::OhlcvBar;

    #[test]
    fn name_and_warmup() {
        let core = AtrCore::new();
        assert_eq!(core.name(), "atr_core");
        assert_eq!(core.warmup_periods(&AtrParams::default()), 56); // 14 × 4
    }

    #[test]
    fn empty_series_no_panic() {
        let series = OhlcvSeries { stock_id: "2330".to_string(), timeframe: Timeframe::Daily, bars: vec![] };
        let core = AtrCore::new();
        let out = core.compute(&series, AtrParams::default()).unwrap();
        assert!(out.series.is_empty());
    }

    fn b(d: &str, h: f64, l: f64, c: f64) -> OhlcvBar {
        OhlcvBar { date: chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").unwrap(), open: c, high: h, low: l, close: c, volume: None }
    }

    #[test]
    fn first_atr_equals_high_low() {
        let bars = vec![b("2026-01-01", 102.0, 98.0, 100.0)];
        let atrs = wilder_atr(&bars, 14);
        assert!((atrs[0] - 4.0).abs() < 1e-9);
    }
}
