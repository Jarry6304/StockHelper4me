// market_margin_core(P2)— 對齊 m3Spec/environment_cores.md §七 r3
// Params §7.4(maintenance_warning/danger / significant_change)/
// Output §7.6(maintenance_rate / change_pct / MarginZone)
// EventKind 4 個(EnteredWarning/Danger / Exited / SignificantSingleDayDrop)
// stock_id 保留字 _market_
//
// **Reference(2026-05-10 加)**:
//   maintenance_warning=145 / danger=130:**證交所《有價證券借貸辦法》§39** —
//                                          130% 追繳線 + 145% 預警(監管文件依據)
//   significant_change=5.0%:無學術,業界經驗值「市場融資維持率單日 5% 變化」為大事件

use anyhow::Result;
use chrono::NaiveDate;
use environment_loader::MarketMarginMaintenanceSeries;
use fact_schema::{Fact, IndicatorCore, Timeframe};
use serde::Serialize;
use serde_json::json;

inventory::submit! {
    core_registry::CoreRegistration::new(
        "market_margin_core", "0.1.0", core_registry::CoreKind::Environment, "P2",
        "Market Margin Core(整體融資維持率 zone state)",
    )
}

const RESERVED_STOCK_ID: &str = "_market_";

#[derive(Debug, Clone, Serialize)]
pub struct MarketMarginParams {
    pub timeframe: Timeframe,
    pub maintenance_warning_threshold: f64,
    pub maintenance_danger_threshold: f64,
    pub significant_change_threshold: f64,
}
impl Default for MarketMarginParams {
    fn default() -> Self { Self { timeframe: Timeframe::Daily, maintenance_warning_threshold: 145.0, maintenance_danger_threshold: 130.0, significant_change_threshold: 5.0 } }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum MarginZone { Safe, Warning, Danger }

#[derive(Debug, Clone, Serialize)]
pub struct MarketMarginOutput {
    pub stock_id: String, pub timeframe: Timeframe,
    pub series: Vec<MarketMarginPoint>,
    pub events: Vec<MarketMarginEvent>,
}
#[derive(Debug, Clone, Serialize)]
pub struct MarketMarginPoint {
    pub date: NaiveDate,
    pub maintenance_rate: f64,
    pub change_pct: f64,
    pub zone: MarginZone,
}
#[derive(Debug, Clone, Serialize)]
pub struct MarketMarginEvent { pub date: NaiveDate, pub kind: MarketMarginEventKind, pub value: f64, pub metadata: serde_json::Value }
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum MarketMarginEventKind { EnteredWarningZone, EnteredDangerZone, ExitedDangerZone, SignificantSingleDayDrop }

pub struct MarketMarginCore;
impl MarketMarginCore { pub fn new() -> Self { MarketMarginCore } }
impl Default for MarketMarginCore { fn default() -> Self { MarketMarginCore::new() } }

fn classify(v: f64, p: &MarketMarginParams) -> MarginZone {
    if v <= p.maintenance_danger_threshold { MarginZone::Danger }
    else if v <= p.maintenance_warning_threshold { MarginZone::Warning }
    else { MarginZone::Safe }
}

impl IndicatorCore for MarketMarginCore {
    type Input = MarketMarginMaintenanceSeries;
    type Params = MarketMarginParams;
    type Output = MarketMarginOutput;
    fn name(&self) -> &'static str { "market_margin_core" }
    fn version(&self) -> &'static str { "0.1.0" }
    /// §7.5:寫死 20(對齊偏離理由)
    fn warmup_periods(&self, _: &Self::Params) -> usize { 20 }

    fn compute(&self, input: &Self::Input, params: Self::Params) -> Result<Self::Output> {
        let mut series = Vec::with_capacity(input.points.len());
        let mut prev: Option<f64> = None;
        for p in &input.points {
            let r = p.ratio.unwrap_or(0.0);
            if r <= 0.0 { continue; }
            let change = match prev { Some(pv) if pv > 0.0 => (r - pv) / pv * 100.0, _ => 0.0 };
            series.push(MarketMarginPoint { date: p.date, maintenance_rate: r, change_pct: change, zone: classify(r, &params) });
            prev = Some(r);
        }
        let mut events = Vec::new();
        let mut prev_zone: Option<MarginZone> = None;
        for p in &series {
            if let Some(pz) = prev_zone {
                if pz == MarginZone::Safe && p.zone == MarginZone::Warning {
                    events.push(MarketMarginEvent { date: p.date, kind: MarketMarginEventKind::EnteredWarningZone, value: p.maintenance_rate,
                        metadata: json!({"rate": p.maintenance_rate, "threshold": params.maintenance_warning_threshold}) });
                }
                if pz != MarginZone::Danger && p.zone == MarginZone::Danger {
                    events.push(MarketMarginEvent { date: p.date, kind: MarketMarginEventKind::EnteredDangerZone, value: p.maintenance_rate,
                        metadata: json!({"rate": p.maintenance_rate, "threshold": params.maintenance_danger_threshold}) });
                }
                if pz == MarginZone::Danger && p.zone != MarginZone::Danger {
                    events.push(MarketMarginEvent { date: p.date, kind: MarketMarginEventKind::ExitedDangerZone, value: p.maintenance_rate,
                        metadata: json!({"rate": p.maintenance_rate}) });
                }
            }
            if p.change_pct <= -params.significant_change_threshold {
                events.push(MarketMarginEvent { date: p.date, kind: MarketMarginEventKind::SignificantSingleDayDrop, value: p.change_pct,
                    metadata: json!({"rate": p.maintenance_rate, "change_pct": p.change_pct}) });
            }
            prev_zone = Some(p.zone);
        }
        Ok(MarketMarginOutput { stock_id: RESERVED_STOCK_ID.to_string(), timeframe: params.timeframe, series, events })
    }

    fn produce_facts(&self, output: &Self::Output) -> Vec<Fact> {
        output.events.iter().map(|e| Fact {
            stock_id: output.stock_id.clone(), fact_date: e.date, timeframe: output.timeframe,
            source_core: "market_margin_core".to_string(), source_version: "0.1.0".to_string(),
            params_hash: None, statement: format!("Market margin {:?} on {}: ratio={:.1}", e.kind, e.date, e.value),
            metadata: e.metadata.clone(),
        }).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use environment_loader::MarketMarginRaw;
    #[test]
    fn name_warmup_reserved_id() {
        let core = MarketMarginCore::new();
        assert_eq!(core.name(), "market_margin_core");
        assert_eq!(core.warmup_periods(&MarketMarginParams::default()), 20);
    }
    #[test]
    fn entered_danger_emitted() {
        let series = MarketMarginMaintenanceSeries { points: vec![
            MarketMarginRaw { date: NaiveDate::parse_from_str("2026-04-21", "%Y-%m-%d").unwrap(), ratio: Some(150.0), total_margin_purchase_balance: None, total_short_sale_balance: None },
            MarketMarginRaw { date: NaiveDate::parse_from_str("2026-04-22", "%Y-%m-%d").unwrap(), ratio: Some(125.0), total_margin_purchase_balance: None, total_short_sale_balance: None },
        ]};
        let out = MarketMarginCore::new().compute(&series, MarketMarginParams::default()).unwrap();
        assert_eq!(out.stock_id, "_market_");
        assert!(out.events.iter().any(|e| e.kind == MarketMarginEventKind::EnteredDangerZone));
    }
}
