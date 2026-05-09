// market_margin_core(P2)— Environment Core(市場整體融資維持率)
//
// 命名前綴 `market_` 對齊 cores_overview §13.2.1(個股 margin_core 區別)。
// 上游 Silver:market_margin_maintenance_derived,PK 不含 stock_id。
// stock_id 保留字 _market_

use anyhow::Result;
use chrono::NaiveDate;
use environment_loader::MarketMarginMaintenanceSeries;
use fact_schema::{Fact, IndicatorCore, Timeframe};
use serde::Serialize;
use serde_json::json;

inventory::submit! {
    core_registry::CoreRegistration::new(
        "market_margin_core", "0.1.0", core_registry::CoreKind::Environment, "P2",
        "Market Margin Core(市場整體融資維持率)",
    )
}

#[derive(Debug, Clone, Serialize)]
pub struct MarketMarginParams {
    pub timeframe: Timeframe,
    pub ratio_low_threshold: f64,  // 預設 145(維持率預警)
    pub ratio_high_threshold: f64, // 預設 165(過度安全)
}
impl Default for MarketMarginParams { fn default() -> Self { Self { timeframe: Timeframe::Daily, ratio_low_threshold: 145.0, ratio_high_threshold: 165.0 } } }

#[derive(Debug, Clone, Serialize)]
pub struct MarketMarginOutput { pub stock_id: String, pub timeframe: Timeframe, pub events: Vec<MarketMarginEvent> }
#[derive(Debug, Clone, Serialize)]
pub struct MarketMarginEvent { pub date: NaiveDate, pub kind: MarketMarginEventKind, pub value: f64, pub metadata: serde_json::Value }
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum MarketMarginEventKind { RatioLow, RatioHigh }

pub struct MarketMarginCore;
impl MarketMarginCore { pub fn new() -> Self { MarketMarginCore } }
impl Default for MarketMarginCore { fn default() -> Self { MarketMarginCore::new() } }

impl IndicatorCore for MarketMarginCore {
    type Input = MarketMarginMaintenanceSeries;
    type Params = MarketMarginParams;
    type Output = MarketMarginOutput;
    fn name(&self) -> &'static str { "market_margin_core" }
    fn version(&self) -> &'static str { "0.1.0" }
    fn warmup_periods(&self, _: &Self::Params) -> usize { 20 }

    fn compute(&self, input: &Self::Input, params: Self::Params) -> Result<Self::Output> {
        let events = input.points.iter().filter_map(|p| {
            let r = p.ratio?;
            if r > 0.0 && r < params.ratio_low_threshold {
                Some(MarketMarginEvent { date: p.date, kind: MarketMarginEventKind::RatioLow, value: r, metadata: json!({"ratio": r, "threshold": params.ratio_low_threshold}) })
            } else if r >= params.ratio_high_threshold {
                Some(MarketMarginEvent { date: p.date, kind: MarketMarginEventKind::RatioHigh, value: r, metadata: json!({"ratio": r, "threshold": params.ratio_high_threshold}) })
            } else { None }
        }).collect();
        Ok(MarketMarginOutput { stock_id: "_market_".to_string(), timeframe: params.timeframe, events })
    }

    fn produce_facts(&self, output: &Self::Output) -> Vec<Fact> {
        output.events.iter().map(|e| Fact {
            stock_id: output.stock_id.clone(), fact_date: e.date, timeframe: output.timeframe,
            source_core: "market_margin_core".to_string(), source_version: "0.1.0".to_string(),
            params_hash: None,
            statement: format!("Market margin maintenance {:?} on {}: ratio={:.1}", e.kind, e.date, e.value),
            metadata: e.metadata.clone(),
        }).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use environment_loader::MarketMarginRaw;

    #[test]
    fn ratio_low_emitted() {
        let series = MarketMarginMaintenanceSeries { points: vec![
            MarketMarginRaw { date: NaiveDate::parse_from_str("2026-04-22", "%Y-%m-%d").unwrap(), ratio: Some(140.0), total_margin_purchase_balance: None, total_short_sale_balance: None },
        ]};
        let core = MarketMarginCore::new();
        let out = core.compute(&series, MarketMarginParams::default()).unwrap();
        assert_eq!(core.name(), "market_margin_core");
        assert!(out.events.iter().any(|e| e.kind == MarketMarginEventKind::RatioLow));
    }
}
