// rsi_core(P1)— 對齊 m2Spec/oldm2Spec/indicator_cores_momentum.md §四 r2
// Output §4.4(僅 series.value)+ Fact §4.5(5 種)+ Failure Swing §4.6

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

const STREAK_MIN_DAYS: usize = 3;

#[derive(Debug, Clone, Serialize)]
pub struct RsiParams { pub period: usize, pub overbought: f64, pub oversold: f64, pub timeframe: Timeframe }
impl Default for RsiParams { fn default() -> Self { Self { period: 14, overbought: 70.0, oversold: 30.0, timeframe: Timeframe::Daily } } }

#[derive(Debug, Clone, Serialize)]
pub struct RsiOutput {
    pub stock_id: String, pub timeframe: Timeframe,
    pub series: Vec<RsiPoint>,
    #[serde(skip)]
    pub events: Vec<RsiEvent>,
}
#[derive(Debug, Clone, Serialize)]
pub struct RsiPoint { pub date: NaiveDate, pub value: f64 }
#[derive(Debug, Clone, Serialize)]
pub struct RsiEvent { pub date: NaiveDate, pub kind: RsiEventKind, pub value: f64, pub metadata: serde_json::Value }
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum RsiEventKind { OverboughtStreak, OversoldStreak, OverboughtExit, OversoldExit, BearishDivergence, BullishDivergence, FailureSwing }

pub struct RsiCore;
impl RsiCore { pub fn new() -> Self { RsiCore } }
impl Default for RsiCore { fn default() -> Self { RsiCore::new() } }

pub fn wilder_rsi(closes: &[f64], period: usize) -> Vec<f64> {
    let n = closes.len();
    if n < 2 || period == 0 { return vec![0.0; n]; }
    let mut gains = vec![0.0_f64; n]; let mut losses = vec![0.0_f64; n];
    for i in 1..n { let d = closes[i] - closes[i - 1]; if d > 0.0 { gains[i] = d; } else { losses[i] = -d; } }
    let mut ag = vec![0.0; n]; let mut al = vec![0.0; n];
    let warmup = period.min(n - 1);
    let (mut sg, mut sl) = (0.0, 0.0);
    for i in 1..=warmup { sg += gains[i]; sl += losses[i]; }
    let p = warmup as f64;
    ag[warmup] = sg / p; al[warmup] = sl / p;
    for i in (warmup + 1)..n {
        ag[i] = ((period as f64 - 1.0) * ag[i - 1] + gains[i]) / period as f64;
        al[i] = ((period as f64 - 1.0) * al[i - 1] + losses[i]) / period as f64;
    }
    let mut rsi = vec![0.0; n];
    for i in warmup..n {
        rsi[i] = if al[i] < 1e-12 { 100.0 } else { let rs = ag[i] / al[i]; 100.0 - 100.0 / (1.0 + rs) };
    }
    rsi
}

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
        let series: Vec<RsiPoint> = input.bars.iter().zip(rsi.iter()).map(|(b, r)| RsiPoint { date: b.date, value: *r }).collect();
        let mut events = Vec::new();

        // Overbought / Oversold streak
        streak(&series, STREAK_MIN_DAYS, |p| p.value >= params.overbought, RsiEventKind::OverboughtStreak, &mut events);
        streak(&series, STREAK_MIN_DAYS, |p| p.value > 0.0 && p.value <= params.oversold, RsiEventKind::OversoldStreak, &mut events);

        // Exit events
        for i in 1..series.len() {
            if series[i - 1].value >= params.overbought && series[i].value < params.overbought {
                events.push(RsiEvent { date: series[i].date, kind: RsiEventKind::OverboughtExit, value: series[i].value,
                    metadata: json!({"event": "overbought_exit", "date": series[i].date}) });
            }
            if series[i - 1].value <= params.oversold && series[i].value > params.oversold {
                events.push(RsiEvent { date: series[i].date, kind: RsiEventKind::OversoldExit, value: series[i].value,
                    metadata: json!({"event": "oversold_exit", "date": series[i].date}) });
            }
        }

        // Divergence(同 macd 嚴格規則)
        const DIV_MIN_BARS: usize = 20;
        if series.len() > DIV_MIN_BARS {
            for i in DIV_MIN_BARS..series.len() {
                let pi = i - DIV_MIN_BARS;
                if closes[i] > closes[pi] && series[i].value < series[pi].value {
                    events.push(RsiEvent { date: series[i].date, kind: RsiEventKind::BearishDivergence, value: series[i].value,
                        metadata: json!({"event": "bearish_divergence"}) });
                } else if closes[i] < closes[pi] && series[i].value > series[pi].value {
                    events.push(RsiEvent { date: series[i].date, kind: RsiEventKind::BullishDivergence, value: series[i].value,
                        metadata: json!({"event": "bullish_divergence"}) });
                }
            }
        }
        // Failure Swing 留 PR-future(spec §4.6 四步全成立才產出)— TODO

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

fn streak(series: &[RsiPoint], min_days: usize, pred: impl Fn(&RsiPoint) -> bool, kind: RsiEventKind, out: &mut Vec<RsiEvent>) {
    let mut start: Option<usize> = None;
    for (i, p) in series.iter().enumerate() {
        if pred(p) { if start.is_none() { start = Some(i); } }
        else if let Some(s) = start.take() {
            let days = i - s;
            if days >= min_days {
                out.push(RsiEvent { date: series[i - 1].date, kind, value: series[i - 1].value,
                    metadata: json!({"event": format!("{:?}", kind), "days": days, "value": series[i - 1].value}) });
            }
        }
    }
    if let Some(s) = start {
        let days = series.len() - s;
        if days >= min_days && !series.is_empty() {
            let last = series.last().unwrap();
            out.push(RsiEvent { date: last.date, kind, value: last.value,
                metadata: json!({"event": format!("{:?}", kind), "days": days, "value": last.value}) });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn name_warmup() {
        let core = RsiCore::new();
        assert_eq!(core.name(), "rsi_core");
        assert_eq!(core.warmup_periods(&RsiParams::default()), 56);
    }
    #[test]
    fn wilder_steady_uptrend_100() {
        let closes: Vec<f64> = (0..30).map(|i| 100.0 + i as f64).collect();
        assert!((wilder_rsi(&closes, 14)[20] - 100.0).abs() < 1e-9);
    }
}
