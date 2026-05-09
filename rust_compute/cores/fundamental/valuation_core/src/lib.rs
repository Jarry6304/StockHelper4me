// valuation_core(P2)— Fundamental Core(日頻)
//
// 對齊 m2Spec/oldm2Spec/fundamental_cores.md §四 valuation_core(spec r2)。
// Params §4.3 / Output §4.5 / EventKind 8 個 / warmup §4.4 / PER N/A §4.7。

use anyhow::Result;
use chrono::NaiveDate;
use fact_schema::{Fact, IndicatorCore, Timeframe};
use fundamental_loader::ValuationDailySeries;
use serde::Serialize;
use serde_json::json;

inventory::submit! {
    core_registry::CoreRegistration::new(
        "valuation_core", "0.1.0", core_registry::CoreKind::Fundamental, "P2",
        "Valuation Core(PER / PBR / Yield + 5 年百分位)",
    )
}

// ---------------------------------------------------------------------------
// Params(§4.3)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct ValuationParams {
    pub timeframe: Timeframe,
    pub history_lookback_years: usize, // 預設 5
    pub percentile_high: f64,          // 預設 80.0
    pub percentile_low: f64,           // 預設 20.0
    pub yield_high_threshold: f64,     // 預設 5.0
}

impl Default for ValuationParams {
    fn default() -> Self {
        Self {
            timeframe: Timeframe::Daily,
            history_lookback_years: 5,
            percentile_high: 80.0,
            percentile_low: 20.0,
            yield_high_threshold: 5.0,
        }
    }
}

// ---------------------------------------------------------------------------
// Output(§4.5)
// ---------------------------------------------------------------------------

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
    pub fact_date: NaiveDate,                // = date(日頻)
    pub per: Option<f64>,                    // 虧損 → None(§4.7)
    pub pbr: Option<f64>,
    pub dividend_yield: Option<f64>,
    pub per_percentile_5y: Option<f64>,
    pub pbr_percentile_5y: Option<f64>,
    pub yield_percentile_5y: Option<f64>,
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
    PbrExtremeHigh,
    PbrExtremeLow,
    YieldExtremeHigh,
    YieldHighThreshold,
    PerNegative,
    PbrBelowBookValue,
}

// ---------------------------------------------------------------------------
// Core
// ---------------------------------------------------------------------------

pub struct ValuationCore;
impl ValuationCore { pub fn new() -> Self { ValuationCore } }
impl Default for ValuationCore { fn default() -> Self { ValuationCore::new() } }

impl IndicatorCore for ValuationCore {
    type Input = ValuationDailySeries;
    type Params = ValuationParams;
    type Output = ValuationOutput;

    fn name(&self) -> &'static str { "valuation_core" }
    fn version(&self) -> &'static str { "0.1.0" }

    /// §4.4:`history_lookback_years * 252`
    fn warmup_periods(&self, params: &Self::Params) -> usize {
        params.history_lookback_years * 252
    }

    fn compute(&self, input: &Self::Input, params: Self::Params) -> Result<Self::Output> {
        let n = input.points.len();
        let lb = params.history_lookback_years * 252;
        // 收集 raw values 給 percentile 計算
        let pers: Vec<Option<f64>> = input.points.iter().map(|p| {
            // §4.7:虧損(per <= 0)→ None
            p.per.filter(|&v| v > 0.0)
        }).collect();
        let pbrs: Vec<Option<f64>> = input.points.iter().map(|p| p.pbr.filter(|&v| v > 0.0)).collect();
        let yields: Vec<Option<f64>> = input.points.iter().map(|p| p.dividend_yield).collect();

        let mut series = Vec::with_capacity(n);
        for i in 0..n {
            let start = i.saturating_sub(lb);
            let per_pct = percentile_at(&pers, start, i, pers[i]);
            let pbr_pct = percentile_at(&pbrs, start, i, pbrs[i]);
            let yield_pct = percentile_at(&yields, start, i, yields[i]);
            series.push(ValuationPoint {
                date: input.points[i].date,
                fact_date: input.points[i].date,
                per: pers[i],
                pbr: pbrs[i],
                dividend_yield: yields[i],
                per_percentile_5y: per_pct,
                pbr_percentile_5y: pbr_pct,
                yield_percentile_5y: yield_pct,
            });
        }

        let mut events = Vec::new();
        let mut prev_per: Option<f64> = None;
        let mut prev_pbr: Option<f64> = None;
        for p in &series {
            // PER percentile
            if let (Some(per), Some(pct)) = (p.per, p.per_percentile_5y) {
                if pct >= params.percentile_high {
                    events.push(ValuationEvent { date: p.fact_date, kind: ValuationEventKind::PerExtremeHigh, value: per,
                        metadata: json!({"per": per, "percentile_5y": pct}) });
                } else if pct <= params.percentile_low {
                    events.push(ValuationEvent { date: p.fact_date, kind: ValuationEventKind::PerExtremeLow, value: per,
                        metadata: json!({"per": per, "percentile_5y": pct}) });
                }
            }
            // PER turn negative(§4.7)— 由 prev 有值轉 None
            if prev_per.is_some() && p.per.is_none() {
                events.push(ValuationEvent { date: p.fact_date, kind: ValuationEventKind::PerNegative, value: 0.0,
                    metadata: json!({}) });
            }
            prev_per = p.per;
            // PBR percentile
            if let (Some(pbr), Some(pct)) = (p.pbr, p.pbr_percentile_5y) {
                if pct >= params.percentile_high {
                    events.push(ValuationEvent { date: p.fact_date, kind: ValuationEventKind::PbrExtremeHigh, value: pbr,
                        metadata: json!({"pbr": pbr, "percentile_5y": pct}) });
                } else if pct <= params.percentile_low {
                    events.push(ValuationEvent { date: p.fact_date, kind: ValuationEventKind::PbrExtremeLow, value: pbr,
                        metadata: json!({"pbr": pbr, "percentile_5y": pct}) });
                }
            }
            // PbrBelowBookValue(PBR < 1.0,從 >=1 轉 <1)
            if let Some(pbr) = p.pbr {
                if pbr < 1.0 && prev_pbr.map_or(true, |pp| pp >= 1.0) {
                    events.push(ValuationEvent { date: p.fact_date, kind: ValuationEventKind::PbrBelowBookValue, value: pbr,
                        metadata: json!({"pbr": pbr}) });
                }
            }
            prev_pbr = p.pbr;
            // Yield percentile + threshold
            if let (Some(y), Some(pct)) = (p.dividend_yield, p.yield_percentile_5y) {
                if pct >= params.percentile_high {
                    events.push(ValuationEvent { date: p.fact_date, kind: ValuationEventKind::YieldExtremeHigh, value: y,
                        metadata: json!({"yield": y, "percentile_5y": pct}) });
                }
            }
            if let Some(y) = p.dividend_yield {
                if y >= params.yield_high_threshold {
                    events.push(ValuationEvent { date: p.fact_date, kind: ValuationEventKind::YieldHighThreshold, value: y,
                        metadata: json!({"yield": y, "threshold": params.yield_high_threshold}) });
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
}

/// 計算 `cur_value` 在 `values[start..=end]` 中的百分位(0-100)。
/// None 值跳過,if cur_value None → 回 None。
fn percentile_at(values: &[Option<f64>], start: usize, end: usize, cur: Option<f64>) -> Option<f64> {
    let cur = cur?;
    if start > end { return None; }
    let mut sorted: Vec<f64> = values[start..=end].iter().filter_map(|&v| v).collect();
    if sorted.len() < 10 { return None; } // 樣本太少不算
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let count_below = sorted.iter().filter(|&&v| v < cur).count() as f64;
    Some(count_below / sorted.len() as f64 * 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fundamental_loader::ValuationDailyRaw;

    #[test]
    fn name_warmup() {
        let core = ValuationCore::new();
        assert_eq!(core.name(), "valuation_core");
        assert_eq!(core.warmup_periods(&ValuationParams::default()), 5 * 252);
    }

    #[test]
    fn point_has_fact_date() {
        let series = ValuationDailySeries {
            stock_id: "2330".to_string(),
            points: vec![ValuationDailyRaw {
                date: NaiveDate::parse_from_str("2026-04-22", "%Y-%m-%d").unwrap(),
                per: Some(15.0), pbr: Some(2.0), dividend_yield: Some(3.0), market_value_weight: Some(0.3),
            }],
        };
        let out = ValuationCore::new().compute(&series, ValuationParams::default()).unwrap();
        assert_eq!(out.series[0].fact_date, out.series[0].date);
        assert!(out.series[0].per_percentile_5y.is_none(), "樣本太少不算 percentile");
    }

    #[test]
    fn yield_high_threshold_emitted() {
        let series = ValuationDailySeries {
            stock_id: "2330".to_string(),
            points: vec![ValuationDailyRaw {
                date: NaiveDate::parse_from_str("2026-04-22", "%Y-%m-%d").unwrap(),
                per: Some(10.0), pbr: Some(1.5), dividend_yield: Some(6.5), market_value_weight: None,
            }],
        };
        let out = ValuationCore::new().compute(&series, ValuationParams::default()).unwrap();
        assert!(out.events.iter().any(|e| e.kind == ValuationEventKind::YieldHighThreshold));
    }

    #[test]
    fn per_negative_transition_emitted() {
        let series = ValuationDailySeries {
            stock_id: "2330".to_string(),
            points: vec![
                ValuationDailyRaw { date: NaiveDate::parse_from_str("2026-04-22", "%Y-%m-%d").unwrap(),
                    per: Some(10.0), pbr: None, dividend_yield: None, market_value_weight: None },
                ValuationDailyRaw { date: NaiveDate::parse_from_str("2026-04-23", "%Y-%m-%d").unwrap(),
                    per: Some(-5.0), pbr: None, dividend_yield: None, market_value_weight: None }, // 虧損
            ],
        };
        let out = ValuationCore::new().compute(&series, ValuationParams::default()).unwrap();
        assert!(out.events.iter().any(|e| e.kind == ValuationEventKind::PerNegative));
    }
}
