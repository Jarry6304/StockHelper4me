// fear_greed_core(P2)— 對齊 m3Spec/environment_cores.md §六 r3
// Params §6.3(extreme_fear/fear/greed/extreme_greed/streak_min_days)/
// Output §6.5(zone enum)/ EventKind 5 個(Entered/Exited/StreakInZone)
// 已知架構例外:暫直讀 Bronze fear_greed_index(§6.2)

use anyhow::Result;
use chrono::NaiveDate;
use environment_loader::FearGreedIndexSeries;
use fact_schema::{Fact, IndicatorCore, Timeframe};
use serde::Serialize;
use serde_json::json;

inventory::submit! {
    core_registry::CoreRegistration::new(
        "fear_greed_core", "0.1.0", core_registry::CoreKind::Environment, "P2",
        "Fear Greed Core(zone state + streak)",
    )
}

const RESERVED_STOCK_ID: &str = "_global_";

#[derive(Debug, Clone, Serialize)]
pub struct FearGreedParams {
    pub timeframe: Timeframe,
    pub extreme_fear_threshold: f64,
    pub fear_threshold: f64,
    pub greed_threshold: f64,
    pub extreme_greed_threshold: f64,
    pub streak_min_days: usize,
}
impl Default for FearGreedParams {
    fn default() -> Self {
        Self { timeframe: Timeframe::Daily,
            extreme_fear_threshold: 25.0, fear_threshold: 45.0,
            greed_threshold: 55.0, extreme_greed_threshold: 75.0,
            streak_min_days: 5 }
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum FearGreedZone { ExtremeFear, Fear, Neutral, Greed, ExtremeGreed }

#[derive(Debug, Clone, Serialize)]
pub struct FearGreedOutput {
    pub stock_id: String, pub timeframe: Timeframe,
    pub series: Vec<FearGreedPoint>,
    pub events: Vec<FearGreedEvent>,
}
#[derive(Debug, Clone, Serialize)]
pub struct FearGreedPoint { pub date: NaiveDate, pub value: f64, pub zone: FearGreedZone }
#[derive(Debug, Clone, Serialize)]
pub struct FearGreedEvent { pub date: NaiveDate, pub kind: FearGreedEventKind, pub value: f64, pub metadata: serde_json::Value }
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum FearGreedEventKind { EnteredExtremeFear, ExitedExtremeFear, EnteredExtremeGreed, ExitedExtremeGreed, StreakInZone }

pub struct FearGreedCore;
impl FearGreedCore { pub fn new() -> Self { FearGreedCore } }
impl Default for FearGreedCore { fn default() -> Self { FearGreedCore::new() } }

fn classify(v: f64, p: &FearGreedParams) -> FearGreedZone {
    if v <= p.extreme_fear_threshold { FearGreedZone::ExtremeFear }
    else if v < p.fear_threshold { FearGreedZone::Fear }
    else if v <= p.greed_threshold { FearGreedZone::Neutral }
    else if v < p.extreme_greed_threshold { FearGreedZone::Greed }
    else { FearGreedZone::ExtremeGreed }
}

impl IndicatorCore for FearGreedCore {
    type Input = FearGreedIndexSeries;
    type Params = FearGreedParams;
    type Output = FearGreedOutput;
    fn name(&self) -> &'static str { "fear_greed_core" }
    fn version(&self) -> &'static str { "0.1.0" }
    /// §6.4:`streak_min_days + 10`
    fn warmup_periods(&self, params: &Self::Params) -> usize { params.streak_min_days + 10 }

    fn compute(&self, input: &Self::Input, params: Self::Params) -> Result<Self::Output> {
        let series: Vec<FearGreedPoint> = input.points.iter().filter_map(|p| {
            let v = p.value?;
            Some(FearGreedPoint { date: p.date, value: v, zone: classify(v, &params) })
        }).collect();
        let mut events = Vec::new();
        let mut prev_zone: Option<FearGreedZone> = None;
        let mut streak_zone: Option<FearGreedZone> = None;
        let mut streak_start = 0;
        for (i, p) in series.iter().enumerate() {
            // Entered/Exited transitions
            if let Some(prev) = prev_zone {
                if prev != FearGreedZone::ExtremeFear && p.zone == FearGreedZone::ExtremeFear {
                    events.push(FearGreedEvent { date: p.date, kind: FearGreedEventKind::EnteredExtremeFear, value: p.value,
                        metadata: json!({"value": p.value, "threshold": params.extreme_fear_threshold}) });
                }
                if prev == FearGreedZone::ExtremeFear && p.zone != FearGreedZone::ExtremeFear {
                    events.push(FearGreedEvent { date: p.date, kind: FearGreedEventKind::ExitedExtremeFear, value: p.value,
                        metadata: json!({"value": p.value, "threshold": params.extreme_fear_threshold}) });
                }
                if prev != FearGreedZone::ExtremeGreed && p.zone == FearGreedZone::ExtremeGreed {
                    events.push(FearGreedEvent { date: p.date, kind: FearGreedEventKind::EnteredExtremeGreed, value: p.value,
                        metadata: json!({"value": p.value, "threshold": params.extreme_greed_threshold}) });
                }
                if prev == FearGreedZone::ExtremeGreed && p.zone != FearGreedZone::ExtremeGreed {
                    events.push(FearGreedEvent { date: p.date, kind: FearGreedEventKind::ExitedExtremeGreed, value: p.value,
                        metadata: json!({"value": p.value, "threshold": params.extreme_greed_threshold}) });
                }
            }
            // Streak detection
            match streak_zone {
                Some(z) if z == p.zone => {
                    let len = i - streak_start + 1;
                    if len == params.streak_min_days {
                        events.push(FearGreedEvent { date: p.date, kind: FearGreedEventKind::StreakInZone, value: len as f64,
                            metadata: json!({"zone": format!("{:?}", p.zone), "days": len}) });
                    }
                }
                _ => { streak_zone = Some(p.zone); streak_start = i; }
            }
            prev_zone = Some(p.zone);
        }
        Ok(FearGreedOutput { stock_id: RESERVED_STOCK_ID.to_string(), timeframe: params.timeframe, series, events })
    }

    fn produce_facts(&self, output: &Self::Output) -> Vec<Fact> {
        output.events.iter().map(|e| Fact {
            stock_id: output.stock_id.clone(), fact_date: e.date, timeframe: output.timeframe,
            source_core: "fear_greed_core".to_string(), source_version: "0.1.0".to_string(),
            params_hash: None, statement: format!("Fear&Greed {:?} on {}: value={:.1}", e.kind, e.date, e.value),
            metadata: fact_schema::with_event_kind(e.metadata.clone(), &e.kind),
        }).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn name_warmup_reserved_id() {
        let core = FearGreedCore::new();
        assert_eq!(core.name(), "fear_greed_core");
        assert_eq!(core.warmup_periods(&FearGreedParams::default()), 15);
        let input = FearGreedIndexSeries { points: vec![] };
        let out = core.compute(&input, FearGreedParams::default()).unwrap();
        assert_eq!(out.stock_id, "_global_");
    }
}
