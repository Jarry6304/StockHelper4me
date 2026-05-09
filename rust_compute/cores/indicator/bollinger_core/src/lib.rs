// bollinger_core(P1)— Indicator Core(波動 / 通道類)
// 對齊 oldm2Spec/indicator_cores_volatility.md(spec user m3Spec 待寫)
// John Bollinger 標準算法:SMA(20) ± 2 × stdev

use anyhow::Result;
use chrono::NaiveDate;
use fact_schema::{Fact, IndicatorCore, Timeframe};
use indicator_kernel::{sma, standard_deviation};
use ohlcv_loader::OhlcvSeries;
use serde::Serialize;
use serde_json::json;

inventory::submit! {
    core_registry::CoreRegistration::new(
        "bollinger_core", "0.1.0", core_registry::CoreKind::Indicator, "P1",
        "Bollinger Core(SMA ± stdev * k)",
    )
}

#[derive(Debug, Clone, Serialize)]
pub struct BollingerParams { pub timeframe: Timeframe, pub period: usize, pub k: f64 }
impl Default for BollingerParams { fn default() -> Self { Self { timeframe: Timeframe::Daily, period: 20, k: 2.0 } } }

#[derive(Debug, Clone, Serialize)]
pub struct BollingerOutput { pub stock_id: String, pub timeframe: Timeframe, pub series: Vec<BollingerPoint>, pub events: Vec<BollingerEvent> }
#[derive(Debug, Clone, Serialize)]
pub struct BollingerPoint { pub date: NaiveDate, pub middle: f64, pub upper: f64, pub lower: f64, pub bandwidth: f64 }
#[derive(Debug, Clone, Serialize)]
pub struct BollingerEvent { pub date: NaiveDate, pub kind: BollingerEventKind, pub value: f64, pub metadata: serde_json::Value }
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum BollingerEventKind { BreakoutUpper, BreakoutLower, Squeeze }

pub struct BollingerCore;
impl BollingerCore { pub fn new() -> Self { BollingerCore } }
impl Default for BollingerCore { fn default() -> Self { BollingerCore::new() } }

impl IndicatorCore for BollingerCore {
    type Input = OhlcvSeries;
    type Params = BollingerParams;
    type Output = BollingerOutput;
    fn name(&self) -> &'static str { "bollinger_core" }
    fn version(&self) -> &'static str { "0.1.0" }
    fn warmup_periods(&self, params: &Self::Params) -> usize { params.period + 5 }

    fn compute(&self, input: &Self::Input, params: Self::Params) -> Result<Self::Output> {
        let n = input.bars.len();
        let closes: Vec<f64> = input.bars.iter().map(|b| b.close).collect();
        // SMA + std 抽到 indicator_kernel(本 PR 重構)
        let means = sma(&closes, params.period);
        let stds = standard_deviation(&closes, params.period);
        let mut series = Vec::with_capacity(n);
        for i in 0..n {
            let mean = means[i];
            let std = stds[i];
            let upper = mean + params.k * std;
            let lower = mean - params.k * std;
            let bw = if mean > 0.0 { (upper - lower) / mean * 100.0 } else { 0.0 };
            series.push(BollingerPoint { date: input.bars[i].date, middle: mean, upper, lower, bandwidth: bw });
        }
        // events
        let mut events = Vec::new();
        // squeeze:bandwidth 在前 N 筆中位數的 50% 以下
        let lookback = params.period * 5;
        for i in lookback..series.len() {
            let close = closes[i];
            if close > series[i].upper {
                events.push(BollingerEvent { date: series[i].date, kind: BollingerEventKind::BreakoutUpper, value: close,
                    metadata: json!({"close": close, "upper": series[i].upper}) });
            } else if close < series[i].lower {
                events.push(BollingerEvent { date: series[i].date, kind: BollingerEventKind::BreakoutLower, value: close,
                    metadata: json!({"close": close, "lower": series[i].lower}) });
            }
            let mut prev_bws: Vec<f64> = series[i - lookback..i].iter().map(|p| p.bandwidth).collect();
            prev_bws.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let median = prev_bws[prev_bws.len() / 2];
            if series[i].bandwidth < median * 0.5 && median > 0.0 {
                events.push(BollingerEvent { date: series[i].date, kind: BollingerEventKind::Squeeze, value: series[i].bandwidth,
                    metadata: json!({"bandwidth": series[i].bandwidth, "median": median}) });
            }
        }
        Ok(BollingerOutput { stock_id: input.stock_id.clone(), timeframe: params.timeframe, series, events })
    }

    fn produce_facts(&self, output: &Self::Output) -> Vec<Fact> {
        output.events.iter().map(|e| Fact {
            stock_id: output.stock_id.clone(), fact_date: e.date, timeframe: output.timeframe,
            source_core: "bollinger_core".to_string(), source_version: "0.1.0".to_string(),
            params_hash: None, statement: format!("Bollinger {:?} on {}: value={:.2}", e.kind, e.date, e.value),
            metadata: e.metadata.clone(),
        }).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn name() { assert_eq!(BollingerCore::new().name(), "bollinger_core"); }
}
