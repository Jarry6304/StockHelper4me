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
    /// Fusion Layer P1.2:EnterPanic 門檻(深度恐慌,低於 extreme_fear)。
    pub panic_threshold: f64,
}
impl Default for FearGreedParams {
    fn default() -> Self {
        Self { timeframe: Timeframe::Daily,
            extreme_fear_threshold: 25.0, fear_threshold: 45.0,
            greed_threshold: 55.0, extreme_greed_threshold: 75.0,
            streak_min_days: 5, panic_threshold: 10.0 }
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
pub struct FearGreedPoint {
    pub date: NaiveDate,
    pub value: f64,
    pub zone: FearGreedZone,
    /// Fusion Layer P1.2b:value 在尾段 252 個資料點內的百分位(0.0-1.0)。
    pub percentile_252: f64,
}
#[derive(Debug, Clone, Serialize)]
pub struct FearGreedEvent { pub date: NaiveDate, pub kind: FearGreedEventKind, pub value: f64, pub metadata: serde_json::Value }
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum FearGreedEventKind {
    EnteredExtremeFear, ExitedExtremeFear, EnteredExtremeGreed, ExitedExtremeGreed, StreakInZone,
    // Fusion Layer P1.2:供 Fusion market_events 用
    EnterPanic, Drop30In5d,
}

impl FearGreedEventKind {
    /// Fact 嚴重度 — 本 core 自行映射(對齊 fusion_layer §9 #6)。
    fn severity(self) -> fact_schema::Severity {
        use fact_schema::Severity::*;
        use FearGreedEventKind::*;
        match self {
            EnterPanic => Critical,
            EnteredExtremeFear | EnteredExtremeGreed | Drop30In5d => Warning,
            ExitedExtremeFear | ExitedExtremeGreed | StreakInZone => Notable,
        }
    }
}

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

/// Fusion Layer P1.2b:回 `values[i]` 在尾段 `window` 個資料點(含自己)內的百分位(0.0-1.0)。
fn percentile_trailing(values: &[f64], i: usize, window: usize) -> f64 {
    if values.is_empty() || i >= values.len() || window == 0 {
        return 0.0;
    }
    let lo = i.saturating_sub(window - 1);
    let win = &values[lo..=i];
    let le = win.iter().filter(|&&v| v <= values[i]).count();
    le as f64 / win.len() as f64
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
        let valid: Vec<(NaiveDate, f64)> =
            input.points.iter().filter_map(|p| Some((p.date, p.value?))).collect();
        let values: Vec<f64> = valid.iter().map(|&(_, v)| v).collect();
        let series: Vec<FearGreedPoint> = valid
            .iter()
            .enumerate()
            .map(|(i, &(date, v))| FearGreedPoint {
                date,
                value: v,
                zone: classify(v, &params),
                percentile_252: percentile_trailing(&values, i, 252),
            })
            .collect();
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
            // Fusion Layer P1.2:EnterPanic(深度恐慌 — value 跌破 panic 門檻當日,edge)
            if i > 0
                && series[i - 1].value > params.panic_threshold
                && p.value <= params.panic_threshold
            {
                events.push(FearGreedEvent { date: p.date, kind: FearGreedEventKind::EnterPanic, value: p.value,
                    metadata: json!({"value": p.value, "threshold": params.panic_threshold}) });
            }
            // Fusion Layer P1.2:Drop30In5d(5 日內驟跌 ≥ 30 點,edge — 跨越當日 fire 一次)
            if i >= 5 {
                let drop_now = p.value - series[i - 5].value;
                let drop_prev = if i >= 6 { series[i - 1].value - series[i - 6].value } else { 0.0 };
                if drop_now <= -30.0 && drop_prev > -30.0 {
                    events.push(FearGreedEvent { date: p.date, kind: FearGreedEventKind::Drop30In5d, value: drop_now,
                        metadata: json!({"drop": drop_now, "from": series[i - 5].value, "to": p.value}) });
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
        output.events.iter().map(|e| Fact { severity: e.kind.severity(),
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

    #[test]
    fn severity_and_percentile() {
        use fact_schema::Severity;
        assert_eq!(FearGreedEventKind::EnterPanic.severity(), Severity::Critical);
        assert_eq!(FearGreedEventKind::Drop30In5d.severity(), Severity::Warning);
        assert_eq!(FearGreedEventKind::StreakInZone.severity(), Severity::Notable);
        let v = vec![5.0, 50.0, 95.0];
        assert_eq!(percentile_trailing(&v, 2, 252), 1.0);
        assert!((percentile_trailing(&v, 0, 252) - 1.0).abs() < 1e-9);
    }
}
