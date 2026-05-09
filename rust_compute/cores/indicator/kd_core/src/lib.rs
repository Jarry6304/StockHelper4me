// kd_core(P1)— Indicator Core(動量類)
// 對齊 oldm2Spec/indicator_cores_momentum.md §五
// Stochastic Oscillator(%K %D)— George Lane 1950s 標準算法 + 台灣傳統 KD(EMA 平滑)

use anyhow::Result;
use chrono::NaiveDate;
use fact_schema::{Fact, IndicatorCore, Timeframe};
use ohlcv_loader::OhlcvSeries;
use serde::Serialize;
use serde_json::json;

inventory::submit! {
    core_registry::CoreRegistration::new(
        "kd_core", "0.1.0", core_registry::CoreKind::Indicator, "P1",
        "KD Core(Stochastic %K %D)",
    )
}

#[derive(Debug, Clone, Serialize)]
pub struct KdParams { pub timeframe: Timeframe, pub period: usize, pub overbought: f64, pub oversold: f64 }
impl Default for KdParams { fn default() -> Self { Self { timeframe: Timeframe::Daily, period: 9, overbought: 80.0, oversold: 20.0 } } }

#[derive(Debug, Clone, Serialize)]
pub struct KdOutput { pub stock_id: String, pub timeframe: Timeframe, pub series: Vec<KdPoint>, pub events: Vec<KdEvent> }
#[derive(Debug, Clone, Serialize)]
pub struct KdPoint { pub date: NaiveDate, pub k: f64, pub d: f64 }
#[derive(Debug, Clone, Serialize)]
pub struct KdEvent { pub date: NaiveDate, pub kind: KdEventKind, pub value: f64, pub metadata: serde_json::Value }
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum KdEventKind { GoldenCross, DeathCross, Overbought, Oversold }

pub struct KdCore;
impl KdCore { pub fn new() -> Self { KdCore } }
impl Default for KdCore { fn default() -> Self { KdCore::new() } }

impl IndicatorCore for KdCore {
    type Input = OhlcvSeries;
    type Params = KdParams;
    type Output = KdOutput;
    fn name(&self) -> &'static str { "kd_core" }
    fn version(&self) -> &'static str { "0.1.0" }
    fn warmup_periods(&self, params: &Self::Params) -> usize { params.period * 8 }

    fn compute(&self, input: &Self::Input, params: Self::Params) -> Result<Self::Output> {
        let n = input.bars.len();
        let mut series = Vec::with_capacity(n);
        let mut prev_k = 50.0_f64; let mut prev_d = 50.0_f64;
        for i in 0..n {
            let p = params.period.min(i + 1);
            let start = i + 1 - p;
            let lo = input.bars[start..=i].iter().map(|b| b.low).fold(f64::INFINITY, f64::min);
            let hi = input.bars[start..=i].iter().map(|b| b.high).fold(f64::NEG_INFINITY, f64::max);
            let close = input.bars[i].close;
            let rsv = if hi - lo > 1e-9 { (close - lo) / (hi - lo) * 100.0 } else { 50.0 };
            // Taiwan-style KD:K = 2/3 prev_K + 1/3 RSV;D = 2/3 prev_D + 1/3 K
            let k = (2.0 / 3.0) * prev_k + (1.0 / 3.0) * rsv;
            let d = (2.0 / 3.0) * prev_d + (1.0 / 3.0) * k;
            series.push(KdPoint { date: input.bars[i].date, k, d });
            prev_k = k; prev_d = d;
        }
        let mut events = Vec::new();
        for i in 1..series.len() {
            let prev_above = series[i - 1].k > series[i - 1].d;
            let cur_above = series[i].k > series[i].d;
            if !prev_above && cur_above {
                events.push(KdEvent { date: series[i].date, kind: KdEventKind::GoldenCross, value: series[i].k,
                    metadata: json!({"k": series[i].k, "d": series[i].d}) });
            } else if prev_above && !cur_above {
                events.push(KdEvent { date: series[i].date, kind: KdEventKind::DeathCross, value: series[i].k,
                    metadata: json!({"k": series[i].k, "d": series[i].d}) });
            }
            if series[i].k >= params.overbought {
                events.push(KdEvent { date: series[i].date, kind: KdEventKind::Overbought, value: series[i].k, metadata: json!({"k": series[i].k}) });
            } else if series[i].k <= params.oversold {
                events.push(KdEvent { date: series[i].date, kind: KdEventKind::Oversold, value: series[i].k, metadata: json!({"k": series[i].k}) });
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn name() {
        assert_eq!(KdCore::new().name(), "kd_core");
    }
}
