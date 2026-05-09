// exchange_rate_core(P2)— Environment Core(匯率)
//
// 上游 Silver:exchange_rate_derived,PK 含 currency 不含 stock_id。
// stock_id 保留字 _global_(對齊 cores_overview §6.2.1)
//
// **本 PR 範圍**:單日異動 + 連續多空 streak

use anyhow::Result;
use chrono::NaiveDate;
use environment_loader::ExchangeRateSeries;
use fact_schema::{Fact, IndicatorCore, Timeframe};
use serde::Serialize;
use serde_json::json;

inventory::submit! {
    core_registry::CoreRegistration::new(
        "exchange_rate_core", "0.1.0", core_registry::CoreKind::Environment, "P2",
        "Exchange Rate Core(匯率)",
    )
}

#[derive(Debug, Clone, Serialize)]
pub struct ExchangeRateParams {
    pub timeframe: Timeframe,
    pub change_pct_threshold: f64, // 預設 0.5%
    pub streak_min_days: usize,    // 預設 3
}
impl Default for ExchangeRateParams { fn default() -> Self { Self { timeframe: Timeframe::Daily, change_pct_threshold: 0.5, streak_min_days: 3 } } }

#[derive(Debug, Clone, Serialize)]
pub struct ExchangeRateOutput { pub stock_id: String, pub currency: String, pub timeframe: Timeframe, pub events: Vec<ExchangeRateEvent> }
#[derive(Debug, Clone, Serialize)]
pub struct ExchangeRateEvent { pub date: NaiveDate, pub kind: ExchangeRateEventKind, pub value: f64, pub metadata: serde_json::Value }
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum ExchangeRateEventKind { LargeDailyMove, AppreciationStreak, DepreciationStreak }

pub struct ExchangeRateCore;
impl ExchangeRateCore { pub fn new() -> Self { ExchangeRateCore } }
impl Default for ExchangeRateCore { fn default() -> Self { ExchangeRateCore::new() } }

impl IndicatorCore for ExchangeRateCore {
    type Input = ExchangeRateSeries;
    type Params = ExchangeRateParams;
    type Output = ExchangeRateOutput;
    fn name(&self) -> &'static str { "exchange_rate_core" }
    fn version(&self) -> &'static str { "0.1.0" }
    fn warmup_periods(&self, _: &Self::Params) -> usize { 20 }

    fn compute(&self, input: &Self::Input, params: Self::Params) -> Result<Self::Output> {
        let mut events = Vec::new();
        let mut prev: Option<f64> = None;
        let mut s_pos: Option<usize> = None;
        let mut s_neg: Option<usize> = None;
        let mut prev_dates: Vec<NaiveDate> = Vec::new();
        for (i, p) in input.points.iter().enumerate() {
            let rate = p.rate.unwrap_or(0.0);
            prev_dates.push(p.date);
            let change = match prev { Some(pv) if pv > 0.0 => (rate - pv) / pv * 100.0, _ => 0.0 };
            if change.abs() >= params.change_pct_threshold {
                events.push(ExchangeRateEvent { date: p.date, kind: ExchangeRateEventKind::LargeDailyMove, value: change,
                    metadata: json!({ "currency": input.currency, "change_pct": change, "rate": rate }) });
            }
            // streak
            if change > 0.0 { if s_pos.is_none() { s_pos = Some(i); } } else if let Some(s) = s_pos.take() {
                if i - s >= params.streak_min_days { events.push(ExchangeRateEvent { date: prev_dates[i-1], kind: ExchangeRateEventKind::AppreciationStreak, value: (i-s) as f64, metadata: json!({"days": i-s}) }); }
            }
            if change < 0.0 { if s_neg.is_none() { s_neg = Some(i); } } else if let Some(s) = s_neg.take() {
                if i - s >= params.streak_min_days { events.push(ExchangeRateEvent { date: prev_dates[i-1], kind: ExchangeRateEventKind::DepreciationStreak, value: (i-s) as f64, metadata: json!({"days": i-s}) }); }
            }
            prev = Some(rate);
        }
        Ok(ExchangeRateOutput { stock_id: "_global_".to_string(), currency: input.currency.clone(), timeframe: params.timeframe, events })
    }

    fn produce_facts(&self, output: &Self::Output) -> Vec<Fact> {
        output.events.iter().map(|e| Fact {
            stock_id: output.stock_id.clone(), fact_date: e.date, timeframe: output.timeframe,
            source_core: "exchange_rate_core".to_string(), source_version: "0.1.0".to_string(),
            params_hash: None,
            statement: format!("{} {:?} on {}: value={:.4}", output.currency, e.kind, e.date, e.value),
            metadata: e.metadata.clone(),
        }).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use environment_loader::ExchangeRateRaw;

    #[test]
    fn name_and_compute_smoke() {
        let series = ExchangeRateSeries { currency: "USD".to_string(), points: vec![
            ExchangeRateRaw { date: NaiveDate::parse_from_str("2026-04-21", "%Y-%m-%d").unwrap(), rate: Some(31.5) },
            ExchangeRateRaw { date: NaiveDate::parse_from_str("2026-04-22", "%Y-%m-%d").unwrap(), rate: Some(31.7) },
        ]};
        let core = ExchangeRateCore::new();
        let out = core.compute(&series, ExchangeRateParams::default()).unwrap();
        assert_eq!(core.name(), "exchange_rate_core");
        assert_eq!(out.stock_id, "_global_");
        assert!(out.events.iter().any(|e| e.kind == ExchangeRateEventKind::LargeDailyMove));
    }
}
