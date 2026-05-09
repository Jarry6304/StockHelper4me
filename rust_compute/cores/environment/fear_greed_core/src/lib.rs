// fear_greed_core(P2)— Environment Core(恐慌貪婪指數)
//
// 已知架構例外(§6.2):暫直讀 Bronze fear_greed_index(無 Silver derived)
// stock_id 保留字 _global_

use anyhow::Result;
use chrono::NaiveDate;
use environment_loader::FearGreedIndexSeries;
use fact_schema::{Fact, IndicatorCore, Timeframe};
use serde::Serialize;
use serde_json::json;

inventory::submit! {
    core_registry::CoreRegistration::new(
        "fear_greed_core", "0.1.0", core_registry::CoreKind::Environment, "P2",
        "Fear Greed Core(恐慌貪婪指數)",
    )
}

#[derive(Debug, Clone, Serialize)]
pub struct FearGreedParams {
    pub timeframe: Timeframe,
    pub extreme_fear_threshold: f64, // 預設 25(<25 為極度恐慌)
    pub extreme_greed_threshold: f64, // 預設 75(>75 為極度貪婪)
}
impl Default for FearGreedParams { fn default() -> Self { Self { timeframe: Timeframe::Daily, extreme_fear_threshold: 25.0, extreme_greed_threshold: 75.0 } } }

#[derive(Debug, Clone, Serialize)]
pub struct FearGreedOutput { pub stock_id: String, pub timeframe: Timeframe, pub events: Vec<FearGreedEvent> }
#[derive(Debug, Clone, Serialize)]
pub struct FearGreedEvent { pub date: NaiveDate, pub kind: FearGreedEventKind, pub value: f64, pub metadata: serde_json::Value }
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum FearGreedEventKind { ExtremeFear, ExtremeGreed }

pub struct FearGreedCore;
impl FearGreedCore { pub fn new() -> Self { FearGreedCore } }
impl Default for FearGreedCore { fn default() -> Self { FearGreedCore::new() } }

impl IndicatorCore for FearGreedCore {
    type Input = FearGreedIndexSeries;
    type Params = FearGreedParams;
    type Output = FearGreedOutput;
    fn name(&self) -> &'static str { "fear_greed_core" }
    fn version(&self) -> &'static str { "0.1.0" }
    fn warmup_periods(&self, _: &Self::Params) -> usize { 1 }

    fn compute(&self, input: &Self::Input, params: Self::Params) -> Result<Self::Output> {
        let events = input.points.iter().filter_map(|p| {
            let v = p.value?;
            if v <= params.extreme_fear_threshold {
                Some(FearGreedEvent { date: p.date, kind: FearGreedEventKind::ExtremeFear, value: v, metadata: json!({"value": v, "threshold": params.extreme_fear_threshold}) })
            } else if v >= params.extreme_greed_threshold {
                Some(FearGreedEvent { date: p.date, kind: FearGreedEventKind::ExtremeGreed, value: v, metadata: json!({"value": v, "threshold": params.extreme_greed_threshold}) })
            } else { None }
        }).collect();
        Ok(FearGreedOutput { stock_id: "_global_".to_string(), timeframe: params.timeframe, events })
    }

    fn produce_facts(&self, output: &Self::Output) -> Vec<Fact> {
        output.events.iter().map(|e| Fact {
            stock_id: output.stock_id.clone(), fact_date: e.date, timeframe: output.timeframe,
            source_core: "fear_greed_core".to_string(), source_version: "0.1.0".to_string(),
            params_hash: None,
            statement: format!("{:?} on {}: value={:.1}", e.kind, e.date, e.value),
            metadata: e.metadata.clone(),
        }).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use environment_loader::FearGreedRaw;

    #[test]
    fn extreme_fear_emitted() {
        let series = FearGreedIndexSeries { points: vec![
            FearGreedRaw { date: NaiveDate::parse_from_str("2026-04-22", "%Y-%m-%d").unwrap(), value: Some(15.0) },
        ]};
        let core = FearGreedCore::new();
        let out = core.compute(&series, FearGreedParams::default()).unwrap();
        assert!(out.events.iter().any(|e| e.kind == FearGreedEventKind::ExtremeFear));
    }
}
