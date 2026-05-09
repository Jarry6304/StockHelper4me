// valuation_core(P2)— Fundamental Core(日頻)
//
// 對齊 oldm2Spec/fundamental_cores.md(暫 ref,user m3Spec 待寫)。
//
// **本 PR 範圍**:
//   - PER / PBR / Yield 異常 + N 期歷史高低
//
// TODO:
//   - 預設 thresholds 以「行業中位數」校準需要 cross-sector 資料 — 留 P0 後

use anyhow::Result;
use chrono::NaiveDate;
use fact_schema::{Fact, IndicatorCore, Timeframe};
use fundamental_loader::ValuationDailySeries;
use serde::Serialize;
use serde_json::json;

inventory::submit! {
    core_registry::CoreRegistration::new(
        "valuation_core",
        "0.1.0",
        core_registry::CoreKind::Fundamental,
        "P2",
        "Valuation Core(PER / PBR / Dividend Yield)",
    )
}

const MILESTONE_LOOKBACK: usize = 252; // ~1 年

#[derive(Debug, Clone, Serialize)]
pub struct ValuationParams {
    pub timeframe: Timeframe,
    pub per_high_threshold: f64,
    pub per_low_threshold: f64,
    pub yield_high_threshold: f64,
}

impl Default for ValuationParams {
    fn default() -> Self {
        Self { timeframe: Timeframe::Daily, per_high_threshold: 30.0, per_low_threshold: 10.0, yield_high_threshold: 5.0 }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ValuationOutput {
    pub stock_id: String,
    pub timeframe: Timeframe,
    pub series: Vec<ValuationPoint>,
    pub events: Vec<ValuationEvent>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ValuationPoint {
    pub date: NaiveDate,
    pub per: f64,
    pub pbr: f64,
    pub dividend_yield: f64,
    pub market_value_weight: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ValuationEvent {
    pub date: NaiveDate,
    pub kind: ValuationEventKind,
    pub value: f64,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum ValuationEventKind {
    PerExtremeHigh,
    PerExtremeLow,
    YieldExtremeHigh,
    PerMilestoneHigh,
    PerMilestoneLow,
}

pub struct ValuationCore;
impl ValuationCore { pub fn new() -> Self { ValuationCore } }
impl Default for ValuationCore { fn default() -> Self { ValuationCore::new() } }

impl IndicatorCore for ValuationCore {
    type Input = ValuationDailySeries;
    type Params = ValuationParams;
    type Output = ValuationOutput;

    fn name(&self) -> &'static str { "valuation_core" }
    fn version(&self) -> &'static str { "0.1.0" }

    fn compute(&self, input: &Self::Input, params: Self::Params) -> Result<Self::Output> {
        let series: Vec<ValuationPoint> = input.points.iter().map(|p| ValuationPoint {
            date: p.date,
            per: p.per.unwrap_or(0.0),
            pbr: p.pbr.unwrap_or(0.0),
            dividend_yield: p.dividend_yield.unwrap_or(0.0),
            market_value_weight: p.market_value_weight.unwrap_or(0.0),
        }).collect();

        let mut events = Vec::new();
        for (i, p) in series.iter().enumerate() {
            if p.per > 0.0 && p.per >= params.per_high_threshold {
                events.push(ValuationEvent { date: p.date, kind: ValuationEventKind::PerExtremeHigh, value: p.per,
                    metadata: json!({ "per": p.per, "threshold": params.per_high_threshold }) });
            } else if p.per > 0.0 && p.per <= params.per_low_threshold {
                events.push(ValuationEvent { date: p.date, kind: ValuationEventKind::PerExtremeLow, value: p.per,
                    metadata: json!({ "per": p.per, "threshold": params.per_low_threshold }) });
            }
            if p.dividend_yield >= params.yield_high_threshold {
                events.push(ValuationEvent { date: p.date, kind: ValuationEventKind::YieldExtremeHigh, value: p.dividend_yield,
                    metadata: json!({ "yield": p.dividend_yield, "threshold": params.yield_high_threshold }) });
            }
            if i >= MILESTONE_LOOKBACK {
                let window = &series[i - MILESTONE_LOOKBACK..i];
                let max = window.iter().map(|q| q.per).fold(f64::NEG_INFINITY, f64::max);
                let min = window.iter().filter(|q| q.per > 0.0).map(|q| q.per).fold(f64::INFINITY, f64::min);
                if p.per > max && p.per > 0.0 {
                    events.push(ValuationEvent { date: p.date, kind: ValuationEventKind::PerMilestoneHigh, value: p.per,
                        metadata: json!({ "lookback": format!("{}d", MILESTONE_LOOKBACK), "value": p.per }) });
                } else if p.per < min && p.per > 0.0 {
                    events.push(ValuationEvent { date: p.date, kind: ValuationEventKind::PerMilestoneLow, value: p.per,
                        metadata: json!({ "lookback": format!("{}d", MILESTONE_LOOKBACK), "value": p.per }) });
                }
            }
        }

        Ok(ValuationOutput { stock_id: input.stock_id.clone(), timeframe: params.timeframe, series, events })
    }

    fn produce_facts(&self, output: &Self::Output) -> Vec<Fact> {
        output.events.iter().map(|e| Fact {
            stock_id: output.stock_id.clone(),
            fact_date: e.date,
            timeframe: output.timeframe,
            source_core: "valuation_core".to_string(),
            source_version: "0.1.0".to_string(),
            params_hash: None,
            statement: format!("{:?} on {}: value={:.2}", e.kind, e.date, e.value),
            metadata: e.metadata.clone(),
        }).collect()
    }

    fn warmup_periods(&self, _: &Self::Params) -> usize { MILESTONE_LOOKBACK + 10 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fundamental_loader::ValuationDailyRaw;

    #[test]
    fn per_extreme_high_emitted() {
        let series = ValuationDailySeries {
            stock_id: "2330".to_string(),
            points: vec![ValuationDailyRaw {
                date: NaiveDate::parse_from_str("2026-04-22", "%Y-%m-%d").unwrap(),
                per: Some(35.0),
                pbr: Some(5.0),
                dividend_yield: Some(2.0),
                market_value_weight: Some(0.3),
            }],
        };
        let core = ValuationCore::new();
        let out = core.compute(&series, ValuationParams::default()).unwrap();
        assert!(out.events.iter().any(|e| e.kind == ValuationEventKind::PerExtremeHigh));
    }
}
