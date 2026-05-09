// macd_core(P1)— Indicator Core(動量類)
// 對齊 oldm2Spec/indicator_cores_momentum.md §三

use anyhow::Result;
use chrono::NaiveDate;
use fact_schema::{Fact, IndicatorCore, Timeframe};
use ohlcv_loader::OhlcvSeries;
use serde::Serialize;
use serde_json::json;

inventory::submit! {
    core_registry::CoreRegistration::new(
        "macd_core", "0.1.0", core_registry::CoreKind::Indicator, "P1",
        "MACD Core(12/26/9 — golden/death cross)",
    )
}

#[derive(Debug, Clone, Serialize)]
pub struct MacdParams { pub timeframe: Timeframe, pub fast: usize, pub slow: usize, pub signal: usize }
impl Default for MacdParams { fn default() -> Self { Self { timeframe: Timeframe::Daily, fast: 12, slow: 26, signal: 9 } } }

#[derive(Debug, Clone, Serialize)]
pub struct MacdOutput { pub stock_id: String, pub timeframe: Timeframe, pub series: Vec<MacdPoint>, pub events: Vec<MacdEvent> }
#[derive(Debug, Clone, Serialize)]
pub struct MacdPoint { pub date: NaiveDate, pub macd: f64, pub signal: f64, pub histogram: f64 }
#[derive(Debug, Clone, Serialize)]
pub struct MacdEvent { pub date: NaiveDate, pub kind: MacdEventKind, pub value: f64, pub metadata: serde_json::Value }
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum MacdEventKind { GoldenCross, DeathCross }

pub struct MacdCore;
impl MacdCore { pub fn new() -> Self { MacdCore } }
impl Default for MacdCore { fn default() -> Self { MacdCore::new() } }

// EMA 抽到 indicator_kernel(本 PR 重構)
use indicator_kernel::ema;

impl IndicatorCore for MacdCore {
    type Input = OhlcvSeries;
    type Params = MacdParams;
    type Output = MacdOutput;
    fn name(&self) -> &'static str { "macd_core" }
    fn version(&self) -> &'static str { "0.1.0" }
    fn warmup_periods(&self, params: &Self::Params) -> usize { params.slow * 6 }

    fn compute(&self, input: &Self::Input, params: Self::Params) -> Result<Self::Output> {
        let closes: Vec<f64> = input.bars.iter().map(|b| b.close).collect();
        let ema_fast = ema(&closes, params.fast);
        let ema_slow = ema(&closes, params.slow);
        let macd_line: Vec<f64> = (0..closes.len()).map(|i| ema_fast[i] - ema_slow[i]).collect();
        let signal_line = ema(&macd_line, params.signal);
        let series: Vec<MacdPoint> = (0..closes.len()).map(|i| MacdPoint {
            date: input.bars[i].date,
            macd: macd_line[i],
            signal: signal_line[i],
            histogram: macd_line[i] - signal_line[i],
        }).collect();
        let mut events = Vec::new();
        for i in 1..series.len() {
            let prev_above = series[i - 1].macd > series[i - 1].signal;
            let cur_above = series[i].macd > series[i].signal;
            if !prev_above && cur_above {
                events.push(MacdEvent { date: series[i].date, kind: MacdEventKind::GoldenCross, value: series[i].macd,
                    metadata: json!({"macd": series[i].macd, "signal": series[i].signal}) });
            } else if prev_above && !cur_above {
                events.push(MacdEvent { date: series[i].date, kind: MacdEventKind::DeathCross, value: series[i].macd,
                    metadata: json!({"macd": series[i].macd, "signal": series[i].signal}) });
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
        assert_eq!(core.warmup_periods(&MacdParams::default()), 26 * 6);
    }
}
