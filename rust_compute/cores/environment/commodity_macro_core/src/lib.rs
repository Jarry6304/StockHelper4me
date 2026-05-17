// commodity_macro_core(P2)— 對齊 m3Spec/environment_cores.md §十(v3.21 拍版)
//
// Params §10.3 / Output §10.5(4 EventKind:Spike / MomentumUp / MomentumDown / RegimeBreak)
// 初版 commodity = "GOLD";Bronze PK 已含 commodity 維度,future 擴 SILVER/OIL 等。
//
// Reference:
// - streak_min_days = 5(macro 長於個股 3):Brock, Lakonishok & LeBaron (1992)
//   "Simple Technical Trading Rules and the Stochastic Properties of Stock Returns"
//   *Journal of Finance* 47(5):1731-1764
// - regime_break_window = 10(MomentumUp ↔ Down alternation):Hamilton (1989)
//   "A New Approach to the Economic Analysis of Nonstationary Time Series and the
//   Business Cycle" *Econometrica* 57(2):357-384

use anyhow::Result;
use chrono::NaiveDate;
use environment_loader::CommodityMacroSeries;
use fact_schema::{Fact, IndicatorCore, Timeframe};
use serde::Serialize;
use serde_json::json;

inventory::submit! {
    core_registry::CoreRegistration::new(
        "commodity_macro_core", "0.1.0", core_registry::CoreKind::Environment, "P2",
        "Commodity Macro Core(初版 GOLD;Spike / Momentum / RegimeBreak)",
    )
}

const RESERVED_STOCK_ID: &str = "_global_";

#[derive(Debug, Clone, Serialize)]
pub struct CommodityMacroParams {
    pub timeframe: Timeframe,
    pub commodities: Vec<String>,         // 初版 ["GOLD"]
    pub momentum_lookback: usize,         // 預設 60
    pub z_score_threshold: f64,           // 預設 2.0
    pub streak_min_days: usize,           // 預設 5(macro 長於個股 3,Brock 1992)
    pub regime_break_window: usize,       // 預設 10(Hamilton 1989)
}

impl Default for CommodityMacroParams {
    fn default() -> Self {
        Self {
            timeframe: Timeframe::Daily,
            commodities: vec!["GOLD".to_string()],
            momentum_lookback: 60,
            z_score_threshold: 2.0,
            streak_min_days: 5,
            regime_break_window: 10,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CommodityMacroOutput {
    pub stock_id: String,
    pub timeframe: Timeframe,
    pub series: Vec<CommodityMacroPoint>,
    pub events: Vec<CommodityMacroEvent>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CommodityMacroPoint {
    pub date: NaiveDate,
    pub commodity: String,
    pub price: f64,
    pub return_pct: f64,
    pub return_z_score: f64,
    pub momentum_state: String,           // 'up' | 'down' | 'neutral'
    pub streak_days: i32,
}

#[derive(Debug, Clone, Serialize)]
pub struct CommodityMacroEvent {
    pub date: NaiveDate,
    pub kind: CommodityMacroEventKind,
    pub value: f64,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum CommodityMacroEventKind {
    CommoditySpike,
    CommodityMomentumUp,
    CommodityMomentumDown,
    CommodityRegimeBreak,
}

pub struct CommodityMacroCore;
impl CommodityMacroCore { pub fn new() -> Self { CommodityMacroCore } }
impl Default for CommodityMacroCore { fn default() -> Self { CommodityMacroCore::new() } }

impl IndicatorCore for CommodityMacroCore {
    type Input = CommodityMacroSeries;
    type Params = CommodityMacroParams;
    type Output = CommodityMacroOutput;

    fn name(&self) -> &'static str { "commodity_macro_core" }
    fn version(&self) -> &'static str { "0.1.0" }

    fn warmup_periods(&self, params: &Self::Params) -> usize {
        params.momentum_lookback + 10
    }

    fn compute(&self, input: &Self::Input, params: Self::Params) -> Result<Self::Output> {
        let commodity = input.commodity.clone();
        let n = input.points.len();

        // Silver derived 已算好 return_pct / return_z_score / momentum_state / streak_days,
        // 本 Core 直接讀 Silver 結果 + 應用 EventKind 觸發規則(對齊 cores §3.5 重用 Silver)
        let mut series = Vec::with_capacity(n);
        for p in &input.points {
            series.push(CommodityMacroPoint {
                date: p.date,
                commodity: commodity.clone(),
                price: p.price.unwrap_or(0.0),
                return_pct: p.return_pct.unwrap_or(0.0),
                return_z_score: p.return_z_score.unwrap_or(0.0),
                momentum_state: p.momentum_state.clone().unwrap_or_else(|| "neutral".to_string()),
                streak_days: p.streak_days.unwrap_or(0),
            });
        }

        let mut events = Vec::new();
        let streak_min = params.streak_min_days as i32;

        // streak fire 對齊 edge trigger:streak_days 達閾值「當日」fire,後續 持續 streak 不重 fire
        // 用前一日 streak_days < min,當日 >= min 判 edge
        let mut prev_streak: i32 = 0;
        let mut last_momentum_event: Option<(NaiveDate, CommodityMacroEventKind)> = None;

        for (i, point) in series.iter().enumerate() {
            // 1. CommoditySpike(z-score 跨閾值 edge)
            if i > 0 {
                let prev_abs = series[i - 1].return_z_score.abs();
                let cur_abs = point.return_z_score.abs();
                if cur_abs >= params.z_score_threshold && prev_abs < params.z_score_threshold {
                    events.push(CommodityMacroEvent {
                        date: point.date,
                        kind: CommodityMacroEventKind::CommoditySpike,
                        value: point.return_z_score,
                        metadata: json!({
                            "commodity": commodity,
                            "return_pct": point.return_pct,
                            "z_score": point.return_z_score,
                            "lookback_days": params.momentum_lookback,
                        }),
                    });
                }
            }

            // 2/3. MomentumUp / MomentumDown(streak edge trigger)
            if point.streak_days == streak_min && prev_streak < streak_min {
                let kind = if point.momentum_state == "up" {
                    Some(CommodityMacroEventKind::CommodityMomentumUp)
                } else if point.momentum_state == "down" {
                    Some(CommodityMacroEventKind::CommodityMomentumDown)
                } else {
                    None
                };
                if let Some(k) = kind {
                    events.push(CommodityMacroEvent {
                        date: point.date,
                        kind: k,
                        value: point.streak_days as f64,
                        metadata: json!({
                            "commodity": commodity,
                            "days": point.streak_days,
                            "state": point.momentum_state,
                        }),
                    });
                    // 4. RegimeBreak(MomentumUp ↔ Down 在 regime_break_window 內 alternation)
                    if let Some((prev_date, prev_kind)) = last_momentum_event {
                        let opposite = matches!(
                            (k, prev_kind),
                            (CommodityMacroEventKind::CommodityMomentumUp, CommodityMacroEventKind::CommodityMomentumDown) |
                            (CommodityMacroEventKind::CommodityMomentumDown, CommodityMacroEventKind::CommodityMomentumUp)
                        );
                        let gap = (point.date - prev_date).num_days() as usize;
                        if opposite && gap <= params.regime_break_window {
                            let prev_str = match prev_kind {
                                CommodityMacroEventKind::CommodityMomentumUp => "MomentumUp",
                                _ => "MomentumDown",
                            };
                            events.push(CommodityMacroEvent {
                                date: point.date,
                                kind: CommodityMacroEventKind::CommodityRegimeBreak,
                                value: gap as f64,
                                metadata: json!({
                                    "commodity": commodity,
                                    "prev_streak_kind": prev_str,
                                    "prev_streak_end_date": prev_date.format("%Y-%m-%d").to_string(),
                                    "days_since_prev_streak": gap,
                                }),
                            });
                        }
                    }
                    last_momentum_event = Some((point.date, k));
                }
            }
            prev_streak = point.streak_days;
        }

        Ok(CommodityMacroOutput {
            stock_id: RESERVED_STOCK_ID.to_string(),
            timeframe: params.timeframe,
            series,
            events,
        })
    }

    fn produce_facts(&self, output: &Self::Output) -> Vec<Fact> {
        output.events.iter().map(|e| Fact {
            stock_id: output.stock_id.clone(),
            fact_date: e.date,
            timeframe: output.timeframe,
            source_core: "commodity_macro_core".to_string(),
            source_version: "0.1.0".to_string(),
            params_hash: None,
            statement: format!("Commodity {:?} on {}: value={:.4}", e.kind, e.date, e.value),
            metadata: fact_schema::with_event_kind(e.metadata.clone(), &e.kind),
        }).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use environment_loader::CommodityMacroRaw;

    fn pt(d: &str, price: f64, ret_pct: f64, z: f64, state: &str, streak: i32) -> CommodityMacroRaw {
        CommodityMacroRaw {
            date: NaiveDate::parse_from_str(d, "%Y-%m-%d").unwrap(),
            price: Some(price),
            return_pct: Some(ret_pct),
            return_z_score: Some(z),
            momentum_state: Some(state.to_string()),
            streak_days: Some(streak),
        }
    }

    #[test]
    fn name_warmup_reserved() {
        let core = CommodityMacroCore::new();
        assert_eq!(core.name(), "commodity_macro_core");
        assert_eq!(core.warmup_periods(&CommodityMacroParams::default()), 70);
        let input = CommodityMacroSeries { commodity: "GOLD".to_string(), points: vec![] };
        let out = core.compute(&input, CommodityMacroParams::default()).unwrap();
        assert_eq!(out.stock_id, "_global_");
        assert!(out.events.is_empty());
    }

    #[test]
    fn spike_edge_trigger() {
        let core = CommodityMacroCore::new();
        let input = CommodityMacroSeries {
            commodity: "GOLD".to_string(),
            points: vec![
                pt("2025-01-02", 2620.0, 0.5, 1.0, "neutral", 0),
                pt("2025-01-03", 2700.0, 3.0, 2.5, "up", 1), // z 跨 2.0 → fire Spike
                pt("2025-01-04", 2710.0, 0.4, 2.3, "up", 2), // 仍 >2,不 re-fire(edge)
            ],
        };
        let out = core.compute(&input, CommodityMacroParams::default()).unwrap();
        let spikes: Vec<_> = out.events.iter()
            .filter(|e| e.kind == CommodityMacroEventKind::CommoditySpike).collect();
        assert_eq!(spikes.len(), 1, "edge trigger should fire once");
        assert_eq!(spikes[0].date.format("%Y-%m-%d").to_string(), "2025-01-03");
    }

    #[test]
    fn momentum_up_streak_edge() {
        let core = CommodityMacroCore::new();
        // streak_min_days=5;edge 在 streak_days 等於 5 當日 — 用 1,2,3,4,5 確保 hit
        let mut points = Vec::new();
        for i in 0..5 {
            let d = format!("2025-01-{:02}", i + 1);
            points.push(pt(&d, 2620.0 + i as f64, 0.3, 0.5, "up", (i + 1) as i32));
        }
        let input = CommodityMacroSeries { commodity: "GOLD".to_string(), points };
        let out = core.compute(&input, CommodityMacroParams::default()).unwrap();
        let momenta: Vec<_> = out.events.iter()
            .filter(|e| e.kind == CommodityMacroEventKind::CommodityMomentumUp).collect();
        assert_eq!(momenta.len(), 1, "edge trigger at streak=5");
    }

    #[test]
    fn regime_break_within_10_days() {
        let core = CommodityMacroCore::new();
        let mut points = Vec::new();
        // Day 0-4(Jan 1-5):streak 1-5 up → MomentumUp at Jan 5
        for i in 0..5 {
            let d = format!("2025-01-{:02}", i + 1);
            points.push(pt(&d, 2620.0, 0.3, 0.5, "up", (i + 1) as i32));
        }
        // gap neutral Jan 6-10
        for i in 5..10 {
            let d = format!("2025-01-{:02}", i + 1);
            points.push(pt(&d, 2620.0, -0.1, -0.3, "neutral", 0));
        }
        // Day 10-14(Jan 11-15):streak 1-5 down → MomentumDown at Jan 15
        // gap Jan 5 → Jan 15 = 10 days,在 regime_break_window=10 內
        for i in 10..15 {
            let d = format!("2025-01-{:02}", i + 1);
            points.push(pt(&d, 2620.0, -0.3, -0.5, "down", (i - 9) as i32));
        }
        let input = CommodityMacroSeries { commodity: "GOLD".to_string(), points };
        let out = core.compute(&input, CommodityMacroParams::default()).unwrap();
        let breaks: Vec<_> = out.events.iter()
            .filter(|e| e.kind == CommodityMacroEventKind::CommodityRegimeBreak).collect();
        assert_eq!(breaks.len(), 1, "regime break should fire");
    }
}
