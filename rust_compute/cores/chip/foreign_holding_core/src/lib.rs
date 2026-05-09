// foreign_holding_core(P2)— Chip Core
//
// 對齊 m3Spec/chip_cores.md §五 foreign_holding_core(外資持股比率)。
//
// **本 PR 範圍**:
//   - 完整 ForeignHoldingParams + Output + 4 EventKind(對齊 §5.5)
//   - compute():逐筆組 series + day-over-day change_pct
//   - LimitNearAlert / SignificantSingleDayChange threshold-based
//   - HoldingMilestoneHigh / Low(N 期最高 / 最低,使用 series 內 lookback)
//
// TODO:
//   - foreign_limit_pct 目前 NULL placeholder(Silver 沒 stored col),
//     LimitNearAlert 在 limit 為 0 時不觸發 — 留 user 確認 Silver 是否要 expose
//   - HoldingMilestoneHigh/Low 的 lookback 期數寫死 60 期(spec §5.6 範例「6-month high」)

use anyhow::Result;
use chip_loader::ForeignHoldingSeries;
use chrono::NaiveDate;
use fact_schema::{Fact, IndicatorCore, Timeframe};
use serde::Serialize;
use serde_json::json;

inventory::submit! {
    core_registry::CoreRegistration::new(
        "foreign_holding_core",
        "0.1.0",
        core_registry::CoreKind::Chip,
        "P2",
        "Foreign Holding Core(外資持股比率變化 / 接近上限警訊)",
    )
}

const MILESTONE_LOOKBACK: usize = 60;

#[derive(Debug, Clone, Serialize)]
pub struct ForeignHoldingParams {
    pub timeframe: Timeframe,
    pub change_threshold_pct: f64,
    pub limit_alert_remaining: f64,
}

impl Default for ForeignHoldingParams {
    fn default() -> Self {
        Self {
            timeframe: Timeframe::Daily,
            change_threshold_pct: 0.5,
            limit_alert_remaining: 5.0,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ForeignHoldingOutput {
    pub stock_id: String,
    pub timeframe: Timeframe,
    pub series: Vec<ForeignHoldingPoint>,
    pub events: Vec<ForeignHoldingEvent>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ForeignHoldingPoint {
    pub date: NaiveDate,
    pub foreign_holding_pct: f64,
    pub foreign_limit_pct: f64,
    pub remaining_pct: f64,
    pub change_pct: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ForeignHoldingEvent {
    pub date: NaiveDate,
    pub kind: ForeignHoldingEventKind,
    pub value: f64,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum ForeignHoldingEventKind {
    HoldingMilestoneHigh,
    HoldingMilestoneLow,
    LimitNearAlert,
    SignificantSingleDayChange,
}

pub struct ForeignHoldingCore;

impl ForeignHoldingCore { pub fn new() -> Self { ForeignHoldingCore } }
impl Default for ForeignHoldingCore { fn default() -> Self { ForeignHoldingCore::new() } }

impl IndicatorCore for ForeignHoldingCore {
    type Input = ForeignHoldingSeries;
    type Params = ForeignHoldingParams;
    type Output = ForeignHoldingOutput;

    fn name(&self) -> &'static str { "foreign_holding_core" }
    fn version(&self) -> &'static str { "0.1.0" }

    fn compute(&self, input: &Self::Input, params: Self::Params) -> Result<Self::Output> {
        let mut series = Vec::with_capacity(input.points.len());
        let mut prev_pct: Option<f64> = None;
        for p in &input.points {
            let holding_pct = p.foreign_holding_ratio.unwrap_or(0.0);
            let limit_pct = p.foreign_limit_pct.unwrap_or(0.0);
            let remaining = (limit_pct - holding_pct).max(0.0);
            let change = match prev_pct {
                Some(pp) => holding_pct - pp,
                None => 0.0,
            };
            series.push(ForeignHoldingPoint {
                date: p.date,
                foreign_holding_pct: holding_pct,
                foreign_limit_pct: limit_pct,
                remaining_pct: remaining,
                change_pct: change,
            });
            prev_pct = Some(holding_pct);
        }
        let events = detect_events(&series, &params);
        Ok(ForeignHoldingOutput {
            stock_id: input.stock_id.clone(),
            timeframe: params.timeframe,
            series,
            events,
        })
    }

    fn produce_facts(&self, output: &Self::Output) -> Vec<Fact> {
        output.events.iter().map(|e| event_to_fact(output, e)).collect()
    }

    fn warmup_periods(&self, _: &Self::Params) -> usize { 20 }
}

fn detect_events(series: &[ForeignHoldingPoint], params: &ForeignHoldingParams) -> Vec<ForeignHoldingEvent> {
    let mut events = Vec::new();
    for (i, p) in series.iter().enumerate() {
        // SignificantSingleDayChange
        if p.change_pct.abs() >= params.change_threshold_pct {
            events.push(ForeignHoldingEvent {
                date: p.date,
                kind: ForeignHoldingEventKind::SignificantSingleDayChange,
                value: p.change_pct,
                metadata: json!({ "change": p.change_pct, "lookback": "1d" }),
            });
        }
        // LimitNearAlert(只在 limit > 0 時觸發,避免 placeholder 0 誤觸)
        if p.foreign_limit_pct > 0.0 && p.remaining_pct > 0.0 && p.remaining_pct <= params.limit_alert_remaining {
            events.push(ForeignHoldingEvent {
                date: p.date,
                kind: ForeignHoldingEventKind::LimitNearAlert,
                value: p.foreign_holding_pct,
                metadata: json!({
                    "holding": p.foreign_holding_pct,
                    "limit": p.foreign_limit_pct,
                    "remaining": p.remaining_pct,
                }),
            });
        }
        // Milestone high / low(N 期 lookback)
        if i >= MILESTONE_LOOKBACK {
            let window = &series[i - MILESTONE_LOOKBACK..i];
            let max_prev = window.iter().map(|q| q.foreign_holding_pct).fold(f64::NEG_INFINITY, f64::max);
            let min_prev = window.iter().map(|q| q.foreign_holding_pct).fold(f64::INFINITY, f64::min);
            if p.foreign_holding_pct > max_prev {
                events.push(ForeignHoldingEvent {
                    date: p.date,
                    kind: ForeignHoldingEventKind::HoldingMilestoneHigh,
                    value: p.foreign_holding_pct,
                    metadata: json!({ "lookback": format!("{}d", MILESTONE_LOOKBACK), "value": p.foreign_holding_pct }),
                });
            } else if p.foreign_holding_pct < min_prev {
                events.push(ForeignHoldingEvent {
                    date: p.date,
                    kind: ForeignHoldingEventKind::HoldingMilestoneLow,
                    value: p.foreign_holding_pct,
                    metadata: json!({ "lookback": format!("{}d", MILESTONE_LOOKBACK), "value": p.foreign_holding_pct }),
                });
            }
        }
    }
    events
}

fn event_to_fact(output: &ForeignHoldingOutput, e: &ForeignHoldingEvent) -> Fact {
    let statement = match e.kind {
        ForeignHoldingEventKind::HoldingMilestoneHigh => format!(
            "Foreign holding {} high at {:.2}% on {}", e.metadata["lookback"], e.value, e.date
        ),
        ForeignHoldingEventKind::HoldingMilestoneLow => format!(
            "Foreign holding {} low at {:.2}% on {}", e.metadata["lookback"], e.value, e.date
        ),
        ForeignHoldingEventKind::LimitNearAlert => format!(
            "Foreign holding reached {:.2}% on {}, near {:.2}% limit",
            e.metadata["holding"].as_f64().unwrap_or(0.0),
            e.date,
            e.metadata["limit"].as_f64().unwrap_or(0.0)
        ),
        ForeignHoldingEventKind::SignificantSingleDayChange => format!(
            "Foreign holding {} {:.2}% in single day on {}",
            if e.value >= 0.0 { "rose" } else { "dropped" },
            e.value.abs(),
            e.date
        ),
    };
    Fact {
        stock_id: output.stock_id.clone(),
        fact_date: e.date,
        timeframe: output.timeframe,
        source_core: "foreign_holding_core".to_string(),
        source_version: "0.1.0".to_string(),
        params_hash: None,
        statement,
        metadata: e.metadata.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chip_loader::ForeignHoldingRaw;

    fn raw(d: &str, ratio: f64) -> ForeignHoldingRaw {
        ForeignHoldingRaw {
            date: NaiveDate::parse_from_str(d, "%Y-%m-%d").unwrap(),
            foreign_holding_shares: Some(1_000_000),
            foreign_holding_ratio: Some(ratio),
            foreign_limit_pct: None,
        }
    }

    #[test]
    fn significant_change_triggered() {
        let series = ForeignHoldingSeries {
            stock_id: "2330".to_string(),
            points: vec![raw("2026-04-21", 65.0), raw("2026-04-22", 65.8)],
        };
        let core = ForeignHoldingCore::new();
        let out = core.compute(&series, ForeignHoldingParams::default()).unwrap();
        assert!(out.events.iter().any(|e| e.kind == ForeignHoldingEventKind::SignificantSingleDayChange));
    }

    #[test]
    fn name_version() {
        let core = ForeignHoldingCore::new();
        assert_eq!(core.name(), "foreign_holding_core");
        assert_eq!(core.version(), "0.1.0");
    }
}
