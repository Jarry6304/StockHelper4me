// ma_core(P1)— Indicator Core
//
// 對齊 oldm2Spec/indicator_cores_momentum.md §7(SMA / EMA / WMA 同族,以 enum 區分)
//
// **本 PR 範圍**:Vec<MaSpec> 多均線 + cross above/below close 事件
// TODO:多均線 cross(短均上穿長均)golden cross / death cross — 留 PR-future

use anyhow::Result;
use chrono::NaiveDate;
use fact_schema::{Fact, IndicatorCore, Timeframe};
use ohlcv_loader::OhlcvSeries;
use serde::Serialize;
use serde_json::json;

inventory::submit! {
    core_registry::CoreRegistration::new(
        "ma_core", "0.1.0", core_registry::CoreKind::Indicator, "P1",
        "MA Core(SMA / EMA / WMA 同族 by enum)",
    )
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum MaKind { Sma, Ema, Wma }

#[derive(Debug, Clone, Serialize)]
pub struct MaSpec { pub kind: MaKind, pub period: usize }

#[derive(Debug, Clone, Serialize)]
pub struct MaParams { pub timeframe: Timeframe, pub specs: Vec<MaSpec> }
impl Default for MaParams {
    fn default() -> Self {
        Self { timeframe: Timeframe::Daily, specs: vec![
            MaSpec { kind: MaKind::Sma, period: 5 },
            MaSpec { kind: MaKind::Sma, period: 20 },
            MaSpec { kind: MaKind::Sma, period: 60 },
        ]}
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct MaOutput { pub stock_id: String, pub timeframe: Timeframe, pub events: Vec<MaEvent> }
#[derive(Debug, Clone, Serialize)]
pub struct MaEvent { pub date: NaiveDate, pub kind: MaEventKind, pub value: f64, pub metadata: serde_json::Value }
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum MaEventKind { CrossAbove, CrossBelow }

pub struct MaCore;
impl MaCore { pub fn new() -> Self { MaCore } }
impl Default for MaCore { fn default() -> Self { MaCore::new() } }

// SMA / EMA / WMA 計算抽到 indicator_kernel(本 PR 重構 — 各 indicator core 不再自己重新實作)。
// Re-export 給後續 ma_core 內部 + Tests 用,signature 對齊 indicator_kernel。
pub use indicator_kernel::{ema, sma, wma};

pub fn compute_ma(values: &[f64], spec: &MaSpec) -> Vec<f64> {
    match spec.kind {
        MaKind::Sma => sma(values, spec.period),
        MaKind::Ema => ema(values, spec.period),
        MaKind::Wma => wma(values, spec.period),
    }
}

impl IndicatorCore for MaCore {
    type Input = OhlcvSeries;
    type Params = MaParams;
    type Output = MaOutput;
    fn name(&self) -> &'static str { "ma_core" }
    fn version(&self) -> &'static str { "0.1.0" }
    fn warmup_periods(&self, params: &Self::Params) -> usize {
        params.specs.iter().map(|s| match s.kind {
            MaKind::Sma => s.period + 5,
            MaKind::Ema => s.period * 4,
            MaKind::Wma => s.period + 5,
        }).max().unwrap_or(20)
    }

    fn compute(&self, input: &Self::Input, params: Self::Params) -> Result<Self::Output> {
        let closes: Vec<f64> = input.bars.iter().map(|b| b.close).collect();
        let mut events = Vec::new();
        for spec in &params.specs {
            let ma = compute_ma(&closes, spec);
            for i in 1..closes.len() {
                let prev_above = closes[i - 1] > ma[i - 1];
                let cur_above = closes[i] > ma[i];
                if !prev_above && cur_above {
                    events.push(MaEvent { date: input.bars[i].date, kind: MaEventKind::CrossAbove, value: ma[i],
                        metadata: json!({ "kind": format!("{:?}", spec.kind), "period": spec.period, "close": closes[i], "ma": ma[i] }) });
                } else if prev_above && !cur_above {
                    events.push(MaEvent { date: input.bars[i].date, kind: MaEventKind::CrossBelow, value: ma[i],
                        metadata: json!({ "kind": format!("{:?}", spec.kind), "period": spec.period, "close": closes[i], "ma": ma[i] }) });
                }
            }
        }
        Ok(MaOutput { stock_id: input.stock_id.clone(), timeframe: params.timeframe, events })
    }

    fn produce_facts(&self, output: &Self::Output) -> Vec<Fact> {
        output.events.iter().map(|e| Fact {
            stock_id: output.stock_id.clone(), fact_date: e.date, timeframe: output.timeframe,
            source_core: "ma_core".to_string(), source_version: "0.1.0".to_string(),
            params_hash: None,
            statement: format!("Close {:?} MA on {}: ma={:.2}", e.kind, e.date, e.value),
            metadata: e.metadata.clone(),
        }).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sma_simple() {
        let v = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let r = sma(&v, 3);
        assert!((r[2] - 2.0).abs() < 1e-9); // (1+2+3)/3
        assert!((r[4] - 4.0).abs() < 1e-9); // (3+4+5)/3
    }

    #[test]
    fn name_and_warmup() {
        let core = MaCore::new();
        assert_eq!(core.name(), "ma_core");
        assert!(core.warmup_periods(&MaParams::default()) >= 60); // 60-period SMA
    }
}
