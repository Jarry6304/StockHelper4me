// margin_core(P2)— Chip Core
//
// 對齊 m3Spec/chip_cores.md §四 margin_core(個股級融資融券)。
// 命名注意:本 Core 個股級;市場整體融資維持率為獨立 Core market_margin_core(Environment)
//
// **本 PR 範圍**:
//   - MarginParams + 7 個 EventKind(完整 §4.5)
//   - compute():逐筆組 series + day-over-day change_pct + short_to_margin_ratio
//   - detect:MarginSurge / Crash / ShortSqueeze / ShortBuildUp(threshold-based)
//   - ShortRatioExtremeHigh / Low(threshold-based)
//   - MaintenanceLow(只在 margin_maintenance 有值時觸發)
//
// TODO(後續討論):
//   - "historical high" 標籤(spec §4.6 範例「reached 32% on 2026-04-20(historical high)」)
//     需要 lookback 比較,目前 threshold-based 簡化處理
//   - MaintenanceLow 閾值寫死 145(實務常見預警線)— 可外部化但 spec 沒列

use anyhow::Result;
use chip_loader::{MarginDailyRaw, MarginDailySeries};
use chrono::NaiveDate;
use fact_schema::{Fact, IndicatorCore, Timeframe};
use serde::Serialize;
use serde_json::json;

inventory::submit! {
    core_registry::CoreRegistration::new(
        "margin_core",
        "0.1.0",
        core_registry::CoreKind::Chip,
        "P2",
        "Margin Core(個股級融資融券事實萃取)",
    )
}

#[derive(Debug, Clone, Serialize)]
pub struct MarginParams {
    pub timeframe: Timeframe,
    pub margin_change_pct_threshold: f64,
    pub short_change_pct_threshold: f64,
    pub short_to_margin_ratio_high: f64,
    pub short_to_margin_ratio_low: f64,
}

impl Default for MarginParams {
    fn default() -> Self {
        Self {
            timeframe: Timeframe::Daily,
            margin_change_pct_threshold: 5.0,
            short_change_pct_threshold: 10.0,
            short_to_margin_ratio_high: 30.0,
            short_to_margin_ratio_low: 5.0,
        }
    }
}

/// MaintenanceLow 警戒閾值(spec §4.5 EventKind 列出 MaintenanceLow,但 §4.3 Params 未列;寫死 const)
/// 145% 為融資維持率實務預警線(對齊 market_margin_core 同 const)
const MAINTENANCE_LOW_THRESHOLD: f64 = 145.0;

#[derive(Debug, Clone, Serialize)]
pub struct MarginOutput {
    pub stock_id: String,
    pub timeframe: Timeframe,
    pub series: Vec<MarginPoint>,
    pub events: Vec<MarginEvent>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MarginPoint {
    pub date: NaiveDate,
    pub margin_balance: i64,
    pub short_balance: i64,
    pub margin_change_pct: f64,
    pub short_change_pct: f64,
    pub short_to_margin_ratio: f64,
    pub margin_maintenance: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct MarginEvent {
    pub date: NaiveDate,
    pub kind: MarginEventKind,
    pub value: f64,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum MarginEventKind {
    MarginSurge,
    MarginCrash,
    ShortSqueeze,
    ShortBuildUp,
    ShortRatioExtremeHigh,
    ShortRatioExtremeLow,
    MaintenanceLow,
}

pub struct MarginCore;

impl MarginCore {
    pub fn new() -> Self { MarginCore }
}
impl Default for MarginCore { fn default() -> Self { MarginCore::new() } }

impl IndicatorCore for MarginCore {
    type Input = MarginDailySeries;
    type Params = MarginParams;
    type Output = MarginOutput;

    fn name(&self) -> &'static str { "margin_core" }
    fn version(&self) -> &'static str { "0.1.0" }

    fn compute(&self, input: &Self::Input, params: Self::Params) -> Result<Self::Output> {
        let mut series = Vec::with_capacity(input.points.len());
        let mut prev_margin: Option<i64> = None;
        let mut prev_short: Option<i64> = None;
        for p in &input.points {
            let mb = p.margin_balance.unwrap_or(0);
            let sb = p.short_balance.unwrap_or(0);
            let m_pct = pct_change(prev_margin, mb);
            let s_pct = pct_change(prev_short, sb);
            let ratio = if mb > 0 { sb as f64 / mb as f64 * 100.0 } else { 0.0 };
            series.push(MarginPoint {
                date: p.date,
                margin_balance: mb,
                short_balance: sb,
                margin_change_pct: m_pct,
                short_change_pct: s_pct,
                short_to_margin_ratio: ratio,
                margin_maintenance: p.margin_maintenance.unwrap_or(0.0),
            });
            prev_margin = Some(mb);
            prev_short = Some(sb);
        }
        let events = detect_events(&series, &params, &input.points);
        Ok(MarginOutput {
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

fn pct_change(prev: Option<i64>, cur: i64) -> f64 {
    match prev {
        Some(p) if p > 0 => (cur - p) as f64 / p as f64 * 100.0,
        _ => 0.0,
    }
}

fn detect_events(series: &[MarginPoint], params: &MarginParams, raw: &[MarginDailyRaw]) -> Vec<MarginEvent> {
    let mut events = Vec::new();
    for p in series {
        if p.margin_change_pct >= params.margin_change_pct_threshold {
            events.push(MarginEvent {
                date: p.date,
                kind: MarginEventKind::MarginSurge,
                value: p.margin_change_pct,
                metadata: json!({ "change_pct": p.margin_change_pct, "balance": p.margin_balance }),
            });
        } else if p.margin_change_pct <= -params.margin_change_pct_threshold {
            events.push(MarginEvent {
                date: p.date,
                kind: MarginEventKind::MarginCrash,
                value: p.margin_change_pct,
                metadata: json!({ "change_pct": p.margin_change_pct, "balance": p.margin_balance }),
            });
        }
        if p.short_change_pct <= -params.short_change_pct_threshold {
            events.push(MarginEvent {
                date: p.date,
                kind: MarginEventKind::ShortSqueeze,
                value: p.short_change_pct,
                metadata: json!({ "change_pct": p.short_change_pct, "balance": p.short_balance }),
            });
        } else if p.short_change_pct >= params.short_change_pct_threshold {
            events.push(MarginEvent {
                date: p.date,
                kind: MarginEventKind::ShortBuildUp,
                value: p.short_change_pct,
                metadata: json!({ "change_pct": p.short_change_pct, "balance": p.short_balance }),
            });
        }
        if p.short_to_margin_ratio >= params.short_to_margin_ratio_high {
            events.push(MarginEvent {
                date: p.date,
                kind: MarginEventKind::ShortRatioExtremeHigh,
                value: p.short_to_margin_ratio,
                metadata: json!({ "ratio": p.short_to_margin_ratio, "lookback": "60d" }), // TODO P0 後接真實 lookback
            });
        } else if p.short_to_margin_ratio > 0.0 && p.short_to_margin_ratio <= params.short_to_margin_ratio_low {
            events.push(MarginEvent {
                date: p.date,
                kind: MarginEventKind::ShortRatioExtremeLow,
                value: p.short_to_margin_ratio,
                metadata: json!({ "ratio": p.short_to_margin_ratio }),
            });
        }
    }
    // MaintenanceLow — 只對有 margin_maintenance 值的 points
    for (i, p) in series.iter().enumerate() {
        if raw[i].margin_maintenance.is_some()
            && p.margin_maintenance > 0.0
            && p.margin_maintenance < MAINTENANCE_LOW_THRESHOLD
        {
            events.push(MarginEvent {
                date: p.date,
                kind: MarginEventKind::MaintenanceLow,
                value: p.margin_maintenance,
                metadata: json!({ "maintenance": p.margin_maintenance }),
            });
        }
    }
    events
}

fn event_to_fact(output: &MarginOutput, e: &MarginEvent) -> Fact {
    let statement = match e.kind {
        MarginEventKind::MarginSurge => format!("Margin balance up {:.1}% on {}", e.value, e.date),
        MarginEventKind::MarginCrash => format!("Margin balance down {:.1}% on {}", e.value.abs(), e.date),
        MarginEventKind::ShortSqueeze => format!("Short balance down {:.1}% on {}(short squeeze)", e.value.abs(), e.date),
        MarginEventKind::ShortBuildUp => format!("Short balance up {:.1}% on {}(short build-up)", e.value, e.date),
        MarginEventKind::ShortRatioExtremeHigh => format!("Short-to-margin ratio reached {:.1}% on {}", e.value, e.date),
        MarginEventKind::ShortRatioExtremeLow => format!("Short-to-margin ratio dropped to {:.1}% on {}", e.value, e.date),
        MarginEventKind::MaintenanceLow => format!("Margin maintenance dropped to {:.1}% on {}", e.value, e.date),
    };
    Fact {
        stock_id: output.stock_id.clone(),
        fact_date: e.date,
        timeframe: output.timeframe,
        source_core: "margin_core".to_string(),
        source_version: "0.1.0".to_string(),
        params_hash: None,
        statement,
        metadata: e.metadata.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(d: &str, mb: i64, sb: i64) -> MarginDailyRaw {
        MarginDailyRaw {
            date: NaiveDate::parse_from_str(d, "%Y-%m-%d").unwrap(),
            margin_purchase: Some(0),
            margin_sell: Some(0),
            margin_balance: Some(mb),
            short_sale: Some(0),
            short_cover: Some(0),
            short_balance: Some(sb),
            margin_maintenance: None,
        }
    }

    #[test]
    fn margin_surge_emitted() {
        let series = MarginDailySeries {
            stock_id: "2330".to_string(),
            points: vec![raw("2026-04-21", 10_000, 100), raw("2026-04-22", 11_200, 100)],
        };
        let core = MarginCore::new();
        let out = core.compute(&series, MarginParams::default()).unwrap();
        assert!(out.events.iter().any(|e| e.kind == MarginEventKind::MarginSurge));
    }

    #[test]
    fn short_squeeze_emitted() {
        let series = MarginDailySeries {
            stock_id: "2330".to_string(),
            points: vec![raw("2026-04-21", 10_000, 5_000), raw("2026-04-22", 10_000, 3_200)],
        };
        let core = MarginCore::new();
        let out = core.compute(&series, MarginParams::default()).unwrap();
        assert!(out.events.iter().any(|e| e.kind == MarginEventKind::ShortSqueeze));
    }

    #[test]
    fn warmup_is_20() {
        assert_eq!(MarginCore::new().warmup_periods(&MarginParams::default()), 20);
    }
}
