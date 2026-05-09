// us_market_core(P2)— Environment Core(SPY / VIX,stock_id 保留字 _global_)
//
// **本 PR 範圍**:VIX 高 / 低事件 + SPY 大幅變動
// TODO:VIX/SPY 比值 / 跨市場 correlation — 留 PR-future

use anyhow::Result;
use chrono::NaiveDate;
use environment_loader::MarketIndexUsSeries;
use fact_schema::{Fact, IndicatorCore, Timeframe};
use serde::Serialize;
use serde_json::json;

inventory::submit! {
    core_registry::CoreRegistration::new(
        "us_market_core", "0.1.0", core_registry::CoreKind::Environment, "P2",
        "US Market Core(SPY / VIX 美股環境變數)",
    )
}

#[derive(Debug, Clone, Serialize)]
pub struct UsMarketParams {
    pub timeframe: Timeframe,
    pub vix_high_threshold: f64,    // 預設 30(恐慌)
    pub vix_low_threshold: f64,     // 預設 12(過度樂觀)
    pub spy_change_pct_threshold: f64, // 預設 1.5%
}
impl Default for UsMarketParams { fn default() -> Self { Self { timeframe: Timeframe::Daily, vix_high_threshold: 30.0, vix_low_threshold: 12.0, spy_change_pct_threshold: 1.5 } } }

#[derive(Debug, Clone, Serialize)]
pub struct UsMarketOutput { pub stock_id: String, pub timeframe: Timeframe, pub events: Vec<UsMarketEvent> }
#[derive(Debug, Clone, Serialize)]
pub struct UsMarketEvent { pub date: NaiveDate, pub kind: UsMarketEventKind, pub value: f64, pub metadata: serde_json::Value }
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum UsMarketEventKind { VixHigh, VixLow, SpyLargeMove }

pub struct UsMarketCore;
impl UsMarketCore { pub fn new() -> Self { UsMarketCore } }
impl Default for UsMarketCore { fn default() -> Self { UsMarketCore::new() } }

impl IndicatorCore for UsMarketCore {
    type Input = MarketIndexUsSeries;
    type Params = UsMarketParams;
    type Output = UsMarketOutput;
    fn name(&self) -> &'static str { "us_market_core" }
    fn version(&self) -> &'static str { "0.1.0" }
    fn warmup_periods(&self, _: &Self::Params) -> usize { 20 }

    fn compute(&self, input: &Self::Input, params: Self::Params) -> Result<Self::Output> {
        let mut events = Vec::new();
        let is_vix = input.stock_id.contains("VIX") || input.stock_id == "^VIX";
        let mut prev_close: Option<f64> = None;
        for p in &input.points {
            let close = p.close.unwrap_or(0.0);
            if is_vix {
                if close >= params.vix_high_threshold {
                    events.push(UsMarketEvent { date: p.date, kind: UsMarketEventKind::VixHigh, value: close,
                        metadata: json!({ "vix": close, "threshold": params.vix_high_threshold }) });
                } else if close > 0.0 && close <= params.vix_low_threshold {
                    events.push(UsMarketEvent { date: p.date, kind: UsMarketEventKind::VixLow, value: close,
                        metadata: json!({ "vix": close, "threshold": params.vix_low_threshold }) });
                }
            } else if let Some(prev) = prev_close {
                if prev > 0.0 {
                    let change = (close - prev) / prev * 100.0;
                    if change.abs() >= params.spy_change_pct_threshold {
                        events.push(UsMarketEvent { date: p.date, kind: UsMarketEventKind::SpyLargeMove, value: change,
                            metadata: json!({ "change_pct": change, "close": close }) });
                    }
                }
            }
            prev_close = Some(close);
        }
        Ok(UsMarketOutput { stock_id: "_global_".to_string(), timeframe: params.timeframe, events })
    }

    fn produce_facts(&self, output: &Self::Output) -> Vec<Fact> {
        output.events.iter().map(|e| Fact {
            stock_id: output.stock_id.clone(), fact_date: e.date, timeframe: output.timeframe,
            source_core: "us_market_core".to_string(), source_version: "0.1.0".to_string(),
            params_hash: None,
            statement: format!("{:?} on {}: value={:.2}", e.kind, e.date, e.value),
            metadata: e.metadata.clone(),
        }).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use environment_loader::MarketIndexUsRaw;

    #[test]
    fn vix_high_emitted() {
        let series = MarketIndexUsSeries { stock_id: "^VIX".to_string(), points: vec![
            MarketIndexUsRaw { date: NaiveDate::parse_from_str("2026-04-22", "%Y-%m-%d").unwrap(), close: Some(35.0), volume: None },
        ]};
        let core = UsMarketCore::new();
        let out = core.compute(&series, UsMarketParams::default()).unwrap();
        assert_eq!(out.stock_id, "_global_");
        assert!(out.events.iter().any(|e| e.kind == UsMarketEventKind::VixHigh));
    }
}
