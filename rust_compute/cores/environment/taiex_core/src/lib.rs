// taiex_core(P2)— Environment Core(stock_id 保留字 _index_taiex_)
//
// 對齊 oldm2Spec/environment_cores.md(spec user m3Spec 待寫)+ cores_overview §6.2.1。
//
// **本 PR 範圍**:單日異動 / 連續多空(threshold + streak)
// TODO:技術指標(MACD/RSI 對指數)、跳空 gap 偵測、52 週新高/低 — 留 PR-future

use anyhow::Result;
use chrono::NaiveDate;
use environment_loader::MarketIndexTwSeries;
use fact_schema::{Fact, IndicatorCore, Timeframe};
use serde::Serialize;
use serde_json::json;

inventory::submit! {
    core_registry::CoreRegistration::new(
        "taiex_core", "0.1.0", core_registry::CoreKind::Environment, "P2",
        "TAIEX Core(加權指數)",
    )
}

const RESERVED_STOCK_ID: &str = "_index_taiex_";

#[derive(Debug, Clone, Serialize)]
pub struct TaiexParams {
    pub timeframe: Timeframe,
    pub change_pct_threshold: f64, // 單日漲跌 %,預設 2.0
    pub streak_min_days: usize,    // 連續同向最小天數,預設 3
}
impl Default for TaiexParams { fn default() -> Self { Self { timeframe: Timeframe::Daily, change_pct_threshold: 2.0, streak_min_days: 3 } } }

#[derive(Debug, Clone, Serialize)]
pub struct TaiexOutput { pub stock_id: String, pub timeframe: Timeframe, pub series: Vec<TaiexPoint>, pub events: Vec<TaiexEvent> }
#[derive(Debug, Clone, Serialize)]
pub struct TaiexPoint { pub date: NaiveDate, pub close: f64, pub change_pct: f64 }
#[derive(Debug, Clone, Serialize)]
pub struct TaiexEvent { pub date: NaiveDate, pub kind: TaiexEventKind, pub value: f64, pub metadata: serde_json::Value }
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum TaiexEventKind { LargeDailyMove, BullStreak, BearStreak }

pub struct TaiexCore;
impl TaiexCore { pub fn new() -> Self { TaiexCore } }
impl Default for TaiexCore { fn default() -> Self { TaiexCore::new() } }

impl IndicatorCore for TaiexCore {
    type Input = MarketIndexTwSeries;
    type Params = TaiexParams;
    type Output = TaiexOutput;
    fn name(&self) -> &'static str { "taiex_core" }
    fn version(&self) -> &'static str { "0.1.0" }
    fn warmup_periods(&self, _: &Self::Params) -> usize { 20 }

    fn compute(&self, input: &Self::Input, params: Self::Params) -> Result<Self::Output> {
        let mut series = Vec::with_capacity(input.points.len());
        let mut prev: Option<f64> = None;
        for p in &input.points {
            let close = p.close.unwrap_or(0.0);
            let change_pct = match prev {
                Some(c) if c > 0.0 => (close - c) / c * 100.0,
                _ => 0.0,
            };
            series.push(TaiexPoint { date: p.date, close, change_pct });
            prev = Some(close);
        }
        let mut events = Vec::new();
        for p in &series {
            if p.change_pct.abs() >= params.change_pct_threshold {
                events.push(TaiexEvent { date: p.date, kind: TaiexEventKind::LargeDailyMove, value: p.change_pct,
                    metadata: json!({ "change_pct": p.change_pct, "close": p.close }) });
            }
        }
        // streak
        let mut s_pos: Option<usize> = None;
        let mut s_neg: Option<usize> = None;
        for (i, p) in series.iter().enumerate() {
            if p.change_pct > 0.0 { if s_pos.is_none() { s_pos = Some(i); } } else if let Some(s) = s_pos.take() {
                if i - s >= params.streak_min_days { events.push(TaiexEvent { date: series[i-1].date, kind: TaiexEventKind::BullStreak, value: (i-s) as f64, metadata: json!({"days": i-s, "start_date": series[s].date}) }); }
            }
            if p.change_pct < 0.0 { if s_neg.is_none() { s_neg = Some(i); } } else if let Some(s) = s_neg.take() {
                if i - s >= params.streak_min_days { events.push(TaiexEvent { date: series[i-1].date, kind: TaiexEventKind::BearStreak, value: (i-s) as f64, metadata: json!({"days": i-s, "start_date": series[s].date}) }); }
            }
        }
        Ok(TaiexOutput { stock_id: RESERVED_STOCK_ID.to_string(), timeframe: params.timeframe, series, events })
    }

    fn produce_facts(&self, output: &Self::Output) -> Vec<Fact> {
        output.events.iter().map(|e| Fact {
            stock_id: output.stock_id.clone(), fact_date: e.date, timeframe: output.timeframe,
            source_core: "taiex_core".to_string(), source_version: "0.1.0".to_string(),
            params_hash: None,
            statement: format!("TAIEX {:?} on {}: value={:.2}", e.kind, e.date, e.value),
            metadata: e.metadata.clone(),
        }).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use environment_loader::MarketIndexTwRaw;

    #[test]
    fn name_and_reserved_stock_id() {
        let core = TaiexCore::new();
        assert_eq!(core.name(), "taiex_core");
        let series = MarketIndexTwSeries { points: vec![
            MarketIndexTwRaw { date: NaiveDate::parse_from_str("2026-04-21", "%Y-%m-%d").unwrap(), open: None, high: None, low: None, close: Some(22000.0), volume: None },
            MarketIndexTwRaw { date: NaiveDate::parse_from_str("2026-04-22", "%Y-%m-%d").unwrap(), open: None, high: None, low: None, close: Some(22500.0), volume: None },
        ]};
        let out = core.compute(&series, TaiexParams::default()).unwrap();
        assert_eq!(out.stock_id, "_index_taiex_");
        assert!(out.events.iter().any(|e| e.kind == TaiexEventKind::LargeDailyMove));
    }
}
