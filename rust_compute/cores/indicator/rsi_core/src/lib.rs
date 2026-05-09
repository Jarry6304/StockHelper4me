// rsi_core(P1)— Indicator Core(動量類)
// 對齊 oldm2Spec/indicator_cores_momentum.md §四(spec user m3Spec 待寫)
// Wilder RSI:Welles Wilder 1978 標準算法

use anyhow::Result;
use chrono::NaiveDate;
use fact_schema::{Fact, IndicatorCore, Timeframe};
use ohlcv_loader::OhlcvSeries;
use serde::Serialize;
use serde_json::json;

inventory::submit! {
    core_registry::CoreRegistration::new(
        "rsi_core", "0.1.0", core_registry::CoreKind::Indicator, "P1",
        "RSI Core(Wilder RSI 14-period)",
    )
}

#[derive(Debug, Clone, Serialize)]
pub struct RsiParams { pub timeframe: Timeframe, pub period: usize, pub overbought: f64, pub oversold: f64 }
impl Default for RsiParams { fn default() -> Self { Self { timeframe: Timeframe::Daily, period: 14, overbought: 70.0, oversold: 30.0 } } }

#[derive(Debug, Clone, Serialize)]
pub struct RsiOutput { pub stock_id: String, pub timeframe: Timeframe, pub series: Vec<RsiPoint>, pub events: Vec<RsiEvent> }
#[derive(Debug, Clone, Serialize)]
pub struct RsiPoint { pub date: NaiveDate, pub rsi: f64 }
#[derive(Debug, Clone, Serialize)]
pub struct RsiEvent { pub date: NaiveDate, pub kind: RsiEventKind, pub value: f64, pub metadata: serde_json::Value }
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum RsiEventKind { Overbought, Oversold }

pub struct RsiCore;
impl RsiCore { pub fn new() -> Self { RsiCore } }
impl Default for RsiCore { fn default() -> Self { RsiCore::new() } }

// Wilder RSI 抽到 indicator_kernel(本 PR 重構)
pub use indicator_kernel::wilder_rsi;

impl IndicatorCore for RsiCore {
    type Input = OhlcvSeries;
    type Params = RsiParams;
    type Output = RsiOutput;
    fn name(&self) -> &'static str { "rsi_core" }
    fn version(&self) -> &'static str { "0.1.0" }
    fn warmup_periods(&self, params: &Self::Params) -> usize { params.period * 4 }

    fn compute(&self, input: &Self::Input, params: Self::Params) -> Result<Self::Output> {
        let closes: Vec<f64> = input.bars.iter().map(|b| b.close).collect();
        let rsi = wilder_rsi(&closes, params.period);
        let series: Vec<RsiPoint> = input.bars.iter().zip(rsi.iter()).map(|(b, r)| RsiPoint { date: b.date, rsi: *r }).collect();
        let mut events = Vec::new();
        for p in &series {
            if p.rsi >= params.overbought {
                events.push(RsiEvent { date: p.date, kind: RsiEventKind::Overbought, value: p.rsi, metadata: json!({"rsi": p.rsi, "threshold": params.overbought}) });
            } else if p.rsi > 0.0 && p.rsi <= params.oversold {
                events.push(RsiEvent { date: p.date, kind: RsiEventKind::Oversold, value: p.rsi, metadata: json!({"rsi": p.rsi, "threshold": params.oversold}) });
            }
        }
        Ok(RsiOutput { stock_id: input.stock_id.clone(), timeframe: params.timeframe, series, events })
    }

    fn produce_facts(&self, output: &Self::Output) -> Vec<Fact> {
        output.events.iter().map(|e| Fact {
            stock_id: output.stock_id.clone(), fact_date: e.date, timeframe: output.timeframe,
            source_core: "rsi_core".to_string(), source_version: "0.1.0".to_string(),
            params_hash: None, statement: format!("RSI {:?} on {}: rsi={:.2}", e.kind, e.date, e.value),
            metadata: e.metadata.clone(),
        }).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn name() {
        let core = RsiCore::new();
        assert_eq!(core.name(), "rsi_core");
        assert_eq!(core.warmup_periods(&RsiParams::default()), 56);
    }
    #[test]
    fn rsi_steady_uptrend_high() {
        // 連續上漲 → loss=0 → RSI=100
        let closes: Vec<f64> = (0..30).map(|i| 100.0 + i as f64).collect();
        let rsi = wilder_rsi(&closes, 14);
        assert!((rsi[20] - 100.0).abs() < 1e-9);
    }
}
