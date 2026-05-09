// obv_core(P1)— Indicator Core(量能類)
// 對齊 oldm2Spec/indicator_cores_volume.md(spec user m3Spec 待寫)
// On-Balance Volume(Joe Granville 1963):cumulative volume × price direction
//
// **本 PR 範圍**:OBV 計算 + 量價背離(price 創高 / OBV 未創高)skeleton
// TODO:divergence 細節需 swing point detection — 留 PR-future 接 neely monowave

use anyhow::Result;
use chrono::NaiveDate;
use fact_schema::{Fact, IndicatorCore, Timeframe};
use ohlcv_loader::OhlcvSeries;
use serde::Serialize;
use serde_json::json;

inventory::submit! {
    core_registry::CoreRegistration::new(
        "obv_core", "0.1.0", core_registry::CoreKind::Indicator, "P1",
        "OBV Core(On-Balance Volume + 量價背離 skeleton)",
    )
}

#[derive(Debug, Clone, Serialize)]
pub struct ObvParams { pub timeframe: Timeframe, pub divergence_lookback: usize }
impl Default for ObvParams { fn default() -> Self { Self { timeframe: Timeframe::Daily, divergence_lookback: 20 } } }

#[derive(Debug, Clone, Serialize)]
pub struct ObvOutput { pub stock_id: String, pub timeframe: Timeframe, pub series: Vec<ObvPoint>, pub events: Vec<ObvEvent> }
#[derive(Debug, Clone, Serialize)]
pub struct ObvPoint { pub date: NaiveDate, pub obv: i64 }
#[derive(Debug, Clone, Serialize)]
pub struct ObvEvent { pub date: NaiveDate, pub kind: ObvEventKind, pub value: f64, pub metadata: serde_json::Value }
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum ObvEventKind { BullishDivergence, BearishDivergence }

pub struct ObvCore;
impl ObvCore { pub fn new() -> Self { ObvCore } }
impl Default for ObvCore { fn default() -> Self { ObvCore::new() } }

impl IndicatorCore for ObvCore {
    type Input = OhlcvSeries;
    type Params = ObvParams;
    type Output = ObvOutput;
    fn name(&self) -> &'static str { "obv_core" }
    fn version(&self) -> &'static str { "0.1.0" }
    fn warmup_periods(&self, _: &Self::Params) -> usize { 0 }

    fn compute(&self, input: &Self::Input, params: Self::Params) -> Result<Self::Output> {
        let mut series = Vec::with_capacity(input.bars.len());
        let mut obv: i64 = 0;
        let mut prev_close: Option<f64> = None;
        for b in &input.bars {
            let v = b.volume.unwrap_or(0);
            if let Some(prev) = prev_close {
                if b.close > prev { obv += v; }
                else if b.close < prev { obv -= v; }
            }
            series.push(ObvPoint { date: b.date, obv });
            prev_close = Some(b.close);
        }
        // Divergence 偵測:當前 close 創 N 期高 + OBV 未創 N 期高 → BearishDivergence
        let lb = params.divergence_lookback;
        let mut events = Vec::new();
        for i in lb..series.len() {
            let win_close = &input.bars[i - lb..i];
            let prev_max_close = win_close.iter().map(|b| b.close).fold(f64::NEG_INFINITY, f64::max);
            let prev_max_obv = series[i - lb..i].iter().map(|p| p.obv).max().unwrap_or(i64::MIN);
            let prev_min_close = win_close.iter().map(|b| b.close).fold(f64::INFINITY, f64::min);
            let prev_min_obv = series[i - lb..i].iter().map(|p| p.obv).min().unwrap_or(i64::MAX);
            if input.bars[i].close > prev_max_close && series[i].obv < prev_max_obv {
                events.push(ObvEvent { date: series[i].date, kind: ObvEventKind::BearishDivergence, value: series[i].obv as f64,
                    metadata: json!({"close": input.bars[i].close, "obv": series[i].obv, "lookback": lb}) });
            } else if input.bars[i].close < prev_min_close && series[i].obv > prev_min_obv {
                events.push(ObvEvent { date: series[i].date, kind: ObvEventKind::BullishDivergence, value: series[i].obv as f64,
                    metadata: json!({"close": input.bars[i].close, "obv": series[i].obv, "lookback": lb}) });
            }
        }
        Ok(ObvOutput { stock_id: input.stock_id.clone(), timeframe: params.timeframe, series, events })
    }

    fn produce_facts(&self, output: &Self::Output) -> Vec<Fact> {
        output.events.iter().map(|e| Fact {
            stock_id: output.stock_id.clone(), fact_date: e.date, timeframe: output.timeframe,
            source_core: "obv_core".to_string(), source_version: "0.1.0".to_string(),
            params_hash: None, statement: format!("OBV {:?} on {}: obv={}", e.kind, e.date, e.value as i64),
            metadata: e.metadata.clone(),
        }).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn name() { assert_eq!(ObvCore::new().name(), "obv_core"); }
}
