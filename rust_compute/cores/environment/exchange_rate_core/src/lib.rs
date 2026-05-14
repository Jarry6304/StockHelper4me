// exchange_rate_core(P2)— 對齊 m3Spec/environment_cores.md §五 r3
// Params §5.4(currency_pairs / ma_period / key_levels / significant_change)/
// Output §5.6(rate / change_pct / ma_value / TrendState)/ EventKind 4 個

use anyhow::Result;
use chrono::NaiveDate;
use environment_loader::ExchangeRateSeries;
use fact_schema::{Fact, IndicatorCore, Timeframe};
use serde::Serialize;
use serde_json::json;

inventory::submit! {
    core_registry::CoreRegistration::new(
        "exchange_rate_core", "0.1.0", core_registry::CoreKind::Environment, "P2",
        "Exchange Rate Core(MA cross + key level breakout)",
    )
}

const RESERVED_STOCK_ID: &str = "_global_";

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum TrendState { BullishMa, BearishMa, Neutral }

#[derive(Debug, Clone, Serialize)]
pub struct ExchangeRateParams {
    pub timeframe: Timeframe,
    pub currency_pairs: Vec<String>,
    pub ma_period: usize,
    pub key_levels: Vec<f64>,
    pub significant_change_threshold: f64,
}
impl Default for ExchangeRateParams {
    fn default() -> Self {
        Self { timeframe: Timeframe::Daily, currency_pairs: vec!["USD/TWD".to_string()],
            ma_period: 20, key_levels: vec![30.0, 31.0, 32.0], significant_change_threshold: 0.5 }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ExchangeRateOutput {
    pub stock_id: String, pub timeframe: Timeframe,
    pub series: Vec<ExchangeRatePoint>,
    pub events: Vec<ExchangeRateEvent>,
}
#[derive(Debug, Clone, Serialize)]
pub struct ExchangeRatePoint {
    pub date: NaiveDate,
    pub currency_pair: String,
    pub rate: f64,
    pub change_pct: f64,
    pub ma_value: f64,
    pub trend_state: TrendState,
}
#[derive(Debug, Clone, Serialize)]
pub struct ExchangeRateEvent { pub date: NaiveDate, pub kind: ExchangeRateEventKind, pub value: f64, pub metadata: serde_json::Value }
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum ExchangeRateEventKind {
    KeyLevelBreakout, KeyLevelBreakdown, SignificantSingleDayMove, MaCross,
}

pub struct ExchangeRateCore;
impl ExchangeRateCore { pub fn new() -> Self { ExchangeRateCore } }
impl Default for ExchangeRateCore { fn default() -> Self { ExchangeRateCore::new() } }

fn sma(v: &[f64], p: usize) -> Vec<f64> {
    let mut out = vec![0.0; v.len()];
    if v.is_empty() || p == 0 { return out; }
    let mut s = 0.0;
    for i in 0..v.len() { s += v[i]; if i >= p { s -= v[i - p]; } out[i] = s / (i + 1).min(p) as f64; }
    out
}

impl IndicatorCore for ExchangeRateCore {
    type Input = ExchangeRateSeries;
    type Params = ExchangeRateParams;
    type Output = ExchangeRateOutput;
    fn name(&self) -> &'static str { "exchange_rate_core" }
    fn version(&self) -> &'static str { "0.1.0" }
    /// §5.5:`ma_period + 10`
    fn warmup_periods(&self, params: &Self::Params) -> usize { params.ma_period + 10 }

    fn compute(&self, input: &Self::Input, params: Self::Params) -> Result<Self::Output> {
        let n = input.points.len();
        let pair = format!("{}/TWD", input.currency); // best-guess(input.currency = "USD" 等)
        let rates: Vec<f64> = input.points.iter().map(|p| p.rate.unwrap_or(0.0)).collect();
        let mas = sma(&rates, params.ma_period);
        let mut series = Vec::with_capacity(n);
        let mut prev: Option<f64> = None;
        for i in 0..n {
            let rate = rates[i];
            let change = match prev { Some(p) if p > 0.0 => (rate - p) / p * 100.0, _ => 0.0 };
            let trend = if rate > mas[i] && mas[i] > 0.0 { TrendState::BullishMa }
                else if rate < mas[i] && mas[i] > 0.0 { TrendState::BearishMa }
                else { TrendState::Neutral };
            series.push(ExchangeRatePoint {
                date: input.points[i].date, currency_pair: pair.clone(),
                rate, change_pct: change, ma_value: mas[i], trend_state: trend,
            });
            prev = Some(rate);
        }
        let mut events = Vec::new();
        for i in 1..series.len() {
            let prev_p = &series[i - 1]; let cur = &series[i];
            // Key level breakout / breakdown
            for &level in &params.key_levels {
                if prev_p.rate < level && cur.rate >= level {
                    events.push(ExchangeRateEvent { date: cur.date, kind: ExchangeRateEventKind::KeyLevelBreakout, value: cur.rate,
                        metadata: json!({"pair": pair, "level": level, "rate": cur.rate}) });
                } else if prev_p.rate > level && cur.rate <= level {
                    events.push(ExchangeRateEvent { date: cur.date, kind: ExchangeRateEventKind::KeyLevelBreakdown, value: cur.rate,
                        metadata: json!({"pair": pair, "level": level, "rate": cur.rate}) });
                }
            }
            // Significant single-day move
            if cur.change_pct.abs() >= params.significant_change_threshold {
                events.push(ExchangeRateEvent { date: cur.date, kind: ExchangeRateEventKind::SignificantSingleDayMove, value: cur.change_pct,
                    metadata: json!({"pair": pair, "change": cur.change_pct}) });
            }
            // MA cross(rate cross MA)
            let prev_above = prev_p.rate > prev_p.ma_value && prev_p.ma_value > 0.0;
            let cur_above = cur.rate > cur.ma_value && cur.ma_value > 0.0;
            if prev_above != cur_above {
                let dir = if cur_above { "above" } else { "below" };
                events.push(ExchangeRateEvent { date: cur.date, kind: ExchangeRateEventKind::MaCross, value: cur.rate,
                    metadata: json!({"pair": pair, "direction": dir, "ma_period": params.ma_period}) });
            }
        }
        Ok(ExchangeRateOutput { stock_id: RESERVED_STOCK_ID.to_string(), timeframe: params.timeframe, series, events })
    }

    fn produce_facts(&self, output: &Self::Output) -> Vec<Fact> {
        output.events.iter().map(|e| Fact {
            stock_id: output.stock_id.clone(), fact_date: e.date, timeframe: output.timeframe,
            source_core: "exchange_rate_core".to_string(), source_version: "0.1.0".to_string(),
            params_hash: None, statement: format!("FX {:?} on {}: value={:.4}", e.kind, e.date, e.value),
            metadata: e.metadata.clone(),
        }).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn name_warmup_reserved_id() {
        let core = ExchangeRateCore::new();
        assert_eq!(core.name(), "exchange_rate_core");
        assert_eq!(core.warmup_periods(&ExchangeRateParams::default()), 30);
        let input = ExchangeRateSeries { currency: "USD".to_string(), points: vec![] };
        let out = core.compute(&input, ExchangeRateParams::default()).unwrap();
        assert_eq!(out.stock_id, "_global_");
    }
}
