// business_indicator_core(P2)— Environment Core
// 對齊 m3Spec/environment_cores.md §八 r3(2026-05-07 r3 新增)
// Params §8.4 / warmup §8.5 / Output §8.6 / EventKind 6 個 / Fact §8.7
//
// 上游 Silver:business_indicator_derived(PK 含 stock_id='_market_' sentinel,
// Bronze 2-col → Silver 3-col 升維;Cores 端 Fact 改寫保留字 `_index_business_`,
// loader 端轉換,Core 內部走 `_index_business_`)。
//
// 月頻 Core:warmup_periods 單位為「月份數」(對齊 revenue_core 月頻慣例)。
// 學術依據:無;對齊國發會景氣指標慣例(藍/黃藍/綠/黃紅/紅 燈號 9-45 分區間)。

use anyhow::Result;
use chrono::{Datelike, NaiveDate};
use environment_loader::BusinessIndicatorSeries;
use fact_schema::{Fact, IndicatorCore, Timeframe};
use serde::Serialize;
use serde_json::json;

inventory::submit! {
    core_registry::CoreRegistration::new(
        "business_indicator_core", "0.1.0", core_registry::CoreKind::Environment, "P2",
        "Business Indicator Core(景氣指標 — 領先轉折 / 燈號變化 / 連續同色)",
    )
}

const RESERVED_STOCK_ID: &str = "_index_business_";

#[derive(Debug, Clone, Serialize)]
pub struct BusinessIndicatorParams {
    /// §8.4:領先指標連續上升 / 下降月數;預設 3
    pub leading_streak_min_months: usize,
    /// §8.4:領先指標轉折變化率閾值(%);預設 0.5
    pub leading_turning_threshold: f64,
    /// §8.4:燈號連續月數;預設 3
    pub monitoring_streak_min_months: usize,
}
impl Default for BusinessIndicatorParams {
    fn default() -> Self {
        Self {
            leading_streak_min_months: 3,
            leading_turning_threshold: 0.5,
            monitoring_streak_min_months: 3,
        }
    }
}

/// §8.6:景氣對策信號燈號(國發會分數對應)
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum MonitoringColor { Blue, YellowBlue, Green, YellowRed, Red }

impl MonitoringColor {
    pub fn from_label(s: &str) -> Option<MonitoringColor> {
        match s {
            "blue" => Some(MonitoringColor::Blue),
            "yellow_blue" => Some(MonitoringColor::YellowBlue),
            "green" => Some(MonitoringColor::Green),
            "yellow_red" => Some(MonitoringColor::YellowRed),
            "red" => Some(MonitoringColor::Red),
            _ => None,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            MonitoringColor::Blue => "blue",
            MonitoringColor::YellowBlue => "yellow_blue",
            MonitoringColor::Green => "green",
            MonitoringColor::YellowRed => "yellow_red",
            MonitoringColor::Red => "red",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct BusinessIndicatorOutput {
    pub stock_id: String,
    pub timeframe: Timeframe,
    pub series: Vec<BusinessIndicatorPoint>,
    pub events: Vec<BusinessIndicatorEvent>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BusinessIndicatorPoint {
    pub period: String,           // "2026-03" 月份標籤
    pub fact_date: NaiveDate,     // 月底日(對齊 Fact schema)
    pub report_date: NaiveDate,   // §8.8:國發會實際發布日(月底+27 日左右);Silver 未存,本版 fact_date 同值占位
    pub leading_indicator: f64,
    pub coincident_indicator: f64,
    pub lagging_indicator: f64,
    pub monitoring: i32,          // 9-45 分
    pub monitoring_color: MonitoringColor,
}

#[derive(Debug, Clone, Serialize)]
pub struct BusinessIndicatorEvent {
    pub date: NaiveDate,          // 事件月底日
    pub kind: BusinessIndicatorEventKind,
    pub value: f64,               // 事件主要數值
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum BusinessIndicatorEventKind {
    LeadingTurningUp,             // 領先指標連續下降後轉折向上
    LeadingTurningDown,           // 連續上升後轉折向下
    LeadingStreakUp,              // 連續上升 N 月
    LeadingStreakDown,            // 連續下降 N 月
    MonitoringColorChange,        // 燈號變化(metadata 帶 from / to)
    MonitoringStreakInColor,      // 連續同色 N 月
}

pub struct BusinessIndicatorCore;
impl BusinessIndicatorCore { pub fn new() -> Self { BusinessIndicatorCore } }
impl Default for BusinessIndicatorCore { fn default() -> Self { BusinessIndicatorCore::new() } }

impl IndicatorCore for BusinessIndicatorCore {
    type Input = BusinessIndicatorSeries;
    type Params = BusinessIndicatorParams;
    type Output = BusinessIndicatorOutput;
    fn name(&self) -> &'static str { "business_indicator_core" }
    fn version(&self) -> &'static str { "0.1.0" }
    /// §8.5:連續事件 lookback + 緩衝(月頻,12 月便於跨年比較)
    fn warmup_periods(&self, params: &Self::Params) -> usize {
        params.leading_streak_min_months
            .max(params.monitoring_streak_min_months) + 12
    }

    fn compute(&self, input: &Self::Input, params: Self::Params) -> Result<Self::Output> {
        let series: Vec<BusinessIndicatorPoint> = input.points.iter().filter_map(|p| {
            let leading = p.leading_indicator?;
            let coincident = p.coincident_indicator?;
            let lagging = p.lagging_indicator?;
            let monitoring = p.monitoring?;
            let color_str = p.monitoring_color.as_ref()?;
            let color = MonitoringColor::from_label(color_str.as_str())?;
            let date = p.date;
            let period = format!("{:04}-{:02}", date.year(), date.month());
            Some(BusinessIndicatorPoint {
                period,
                fact_date: date,
                report_date: date, // TODO: ≈ date + 27d;Silver 未存實際發布日,留 Aggregation Layer 處理 look-ahead bias
                leading_indicator: leading,
                coincident_indicator: coincident,
                lagging_indicator: lagging,
                monitoring,
                monitoring_color: color,
            })
        }).collect();

        let mut events = Vec::new();
        let n = series.len();
        if n < 2 {
            return Ok(BusinessIndicatorOutput {
                stock_id: RESERVED_STOCK_ID.to_string(),
                timeframe: Timeframe::Monthly,
                series, events,
            });
        }

        // streak 狀態追蹤
        let mut leading_dir: i32 = 0;       // +1 up / -1 down / 0 neutral
        let mut leading_streak_len: usize = 0;
        let mut color_streak_len: usize = 1; // 首月處於自己色 1 月

        for i in 1..n {
            let prev = &series[i - 1];
            let cur = &series[i];

            // ── 領先指標方向 + 月變化率 ─────────────────────────────────
            let change_pct = if prev.leading_indicator.abs() > 1e-12 {
                (cur.leading_indicator - prev.leading_indicator) / prev.leading_indicator * 100.0
            } else { 0.0 };
            let new_dir = if change_pct > 0.0 { 1i32 }
                else if change_pct < 0.0 { -1i32 } else { 0i32 };

            // ── 轉折偵測(§8.6 LeadingTurningUp/Down):
            //   要件 1:之前是連續 N 月反向(streak >= leading_streak_min_months)
            //   要件 2:本月反向,且月變化率絕對值 >= leading_turning_threshold
            if new_dir == 1
                && leading_dir == -1
                && leading_streak_len >= params.leading_streak_min_months
                && change_pct.abs() >= params.leading_turning_threshold
            {
                events.push(BusinessIndicatorEvent {
                    date: cur.fact_date,
                    kind: BusinessIndicatorEventKind::LeadingTurningUp,
                    value: cur.leading_indicator,
                    metadata: json!({
                        "event": "leading_turning_up",
                        "streak_before": leading_streak_len,
                        "change_pct": change_pct,
                        "value": cur.leading_indicator,
                    }),
                });
            } else if new_dir == -1
                && leading_dir == 1
                && leading_streak_len >= params.leading_streak_min_months
                && change_pct.abs() >= params.leading_turning_threshold
            {
                events.push(BusinessIndicatorEvent {
                    date: cur.fact_date,
                    kind: BusinessIndicatorEventKind::LeadingTurningDown,
                    value: cur.leading_indicator,
                    metadata: json!({
                        "event": "leading_turning_down",
                        "streak_before": leading_streak_len,
                        "change_pct": change_pct,
                        "value": cur.leading_indicator,
                    }),
                });
            }

            // ── 更新 streak ──────────────────────────────────────────────
            if new_dir == leading_dir && new_dir != 0 {
                leading_streak_len += 1;
            } else {
                leading_dir = new_dir;
                leading_streak_len = if new_dir == 0 { 0 } else { 1 };
            }

            // ── Streak 觸發(達到門檻當月觸發一次,避免每月重複)──────
            if leading_dir == 1 && leading_streak_len == params.leading_streak_min_months {
                events.push(BusinessIndicatorEvent {
                    date: cur.fact_date,
                    kind: BusinessIndicatorEventKind::LeadingStreakUp,
                    value: leading_streak_len as f64,
                    metadata: json!({
                        "event": "leading_streak_up",
                        "months": leading_streak_len,
                        "current_value": cur.leading_indicator,
                    }),
                });
            } else if leading_dir == -1 && leading_streak_len == params.leading_streak_min_months {
                events.push(BusinessIndicatorEvent {
                    date: cur.fact_date,
                    kind: BusinessIndicatorEventKind::LeadingStreakDown,
                    value: leading_streak_len as f64,
                    metadata: json!({
                        "event": "leading_streak_down",
                        "months": leading_streak_len,
                        "current_value": cur.leading_indicator,
                    }),
                });
            }

            // ── 燈號變化 / 連續同色 ────────────────────────────────────
            if cur.monitoring_color != prev.monitoring_color {
                events.push(BusinessIndicatorEvent {
                    date: cur.fact_date,
                    kind: BusinessIndicatorEventKind::MonitoringColorChange,
                    value: cur.monitoring as f64,
                    metadata: json!({
                        "event": "color_change",
                        "from": prev.monitoring_color.label(),
                        "to": cur.monitoring_color.label(),
                        "score": cur.monitoring,
                    }),
                });
                color_streak_len = 1;
            } else {
                color_streak_len += 1;
                if color_streak_len == params.monitoring_streak_min_months {
                    events.push(BusinessIndicatorEvent {
                        date: cur.fact_date,
                        kind: BusinessIndicatorEventKind::MonitoringStreakInColor,
                        value: color_streak_len as f64,
                        metadata: json!({
                            "event": "color_streak",
                            "color": cur.monitoring_color.label(),
                            "months": color_streak_len,
                        }),
                    });
                }
            }
        }

        Ok(BusinessIndicatorOutput {
            stock_id: RESERVED_STOCK_ID.to_string(),
            timeframe: Timeframe::Monthly,
            series, events,
        })
    }

    fn produce_facts(&self, output: &Self::Output) -> Vec<Fact> {
        output.events.iter().map(|e| Fact {
            stock_id: output.stock_id.clone(),
            fact_date: e.date,
            timeframe: output.timeframe,
            source_core: "business_indicator_core".to_string(),
            source_version: "0.1.0".to_string(),
            params_hash: None,
            statement: format!("Business {:?} at {}: value={:.2}", e.kind, e.date, e.value),
            metadata: e.metadata.clone(),
        }).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use environment_loader::BusinessIndicatorRaw;

    fn mk_raw(yyyy: i32, mm: u32, leading: f64, coincident: f64, lagging: f64, mon: i32, color: &str) -> BusinessIndicatorRaw {
        BusinessIndicatorRaw {
            date: NaiveDate::from_ymd_opt(yyyy, mm, 28).unwrap(),
            leading_indicator: Some(leading),
            coincident_indicator: Some(coincident),
            lagging_indicator: Some(lagging),
            monitoring: Some(mon),
            monitoring_color: Some(color.to_string()),
        }
    }

    #[test]
    fn name_warmup_reserved_id() {
        let core = BusinessIndicatorCore::new();
        assert_eq!(core.name(), "business_indicator_core");
        assert_eq!(core.warmup_periods(&BusinessIndicatorParams::default()), 15); // max(3,3)+12
        let input = BusinessIndicatorSeries { points: vec![] };
        let out = core.compute(&input, BusinessIndicatorParams::default()).unwrap();
        assert_eq!(out.stock_id, "_index_business_");
        assert_eq!(out.timeframe, Timeframe::Monthly);
    }

    #[test]
    fn monitoring_color_from_label() {
        assert_eq!(MonitoringColor::from_label("blue"), Some(MonitoringColor::Blue));
        assert_eq!(MonitoringColor::from_label("yellow_blue"), Some(MonitoringColor::YellowBlue));
        assert_eq!(MonitoringColor::from_label("green"), Some(MonitoringColor::Green));
        assert_eq!(MonitoringColor::from_label("yellow_red"), Some(MonitoringColor::YellowRed));
        assert_eq!(MonitoringColor::from_label("red"), Some(MonitoringColor::Red));
        assert_eq!(MonitoringColor::from_label("unknown"), None);
    }

    #[test]
    fn leading_streak_up_fires_at_threshold() {
        // 連續 3 月上升 → LeadingStreakUp fire 1 次
        let points = vec![
            mk_raw(2025, 1, 100.0, 100.0, 100.0, 25, "green"),
            mk_raw(2025, 2, 101.0, 100.5, 100.5, 25, "green"),
            mk_raw(2025, 3, 102.0, 101.0, 101.0, 25, "green"),
            mk_raw(2025, 4, 103.0, 101.5, 101.5, 25, "green"),
        ];
        let out = BusinessIndicatorCore::new()
            .compute(&BusinessIndicatorSeries { points }, BusinessIndicatorParams::default()).unwrap();
        let n_streak_up = out.events.iter()
            .filter(|e| e.kind == BusinessIndicatorEventKind::LeadingStreakUp).count();
        assert_eq!(n_streak_up, 1, "3-month up streak fires exactly once");
    }

    #[test]
    fn leading_turning_up_fires_after_down_streak() {
        // 3 月下降 → 第 4 月上升 0.5%+ → LeadingTurningUp fire
        let points = vec![
            mk_raw(2025, 1, 100.0, 100.0, 100.0, 22, "yellow_blue"),
            mk_raw(2025, 2, 99.0,  99.5,  99.5, 22, "yellow_blue"),
            mk_raw(2025, 3, 98.0,  99.0,  99.0, 22, "yellow_blue"),
            mk_raw(2025, 4, 97.0,  98.5,  98.5, 22, "yellow_blue"),
            mk_raw(2025, 5, 98.5,  99.0,  99.0, 22, "yellow_blue"), // +1.5% turn up
        ];
        let out = BusinessIndicatorCore::new()
            .compute(&BusinessIndicatorSeries { points }, BusinessIndicatorParams::default()).unwrap();
        let n_turn_up = out.events.iter()
            .filter(|e| e.kind == BusinessIndicatorEventKind::LeadingTurningUp).count();
        assert_eq!(n_turn_up, 1, "turning up after down streak fires once");
    }

    #[test]
    fn monitoring_color_change_fires_on_transition() {
        let points = vec![
            mk_raw(2025, 1, 100.0, 100.0, 100.0, 22, "yellow_blue"),
            mk_raw(2025, 2, 100.5, 100.5, 100.5, 17, "blue"),       // yellow_blue → blue
        ];
        let out = BusinessIndicatorCore::new()
            .compute(&BusinessIndicatorSeries { points }, BusinessIndicatorParams::default()).unwrap();
        let n_change = out.events.iter()
            .filter(|e| e.kind == BusinessIndicatorEventKind::MonitoringColorChange).count();
        assert_eq!(n_change, 1);
        // metadata 含 from/to
        let evt = out.events.iter().find(|e| e.kind == BusinessIndicatorEventKind::MonitoringColorChange).unwrap();
        assert_eq!(evt.metadata["from"], "yellow_blue");
        assert_eq!(evt.metadata["to"], "blue");
    }

    #[test]
    fn monitoring_streak_in_color_fires_at_threshold() {
        // 3 月同色 green → MonitoringStreakInColor fire at month 3(達 streak_min_months=3)
        let points = vec![
            mk_raw(2025, 1, 100.0, 100.0, 100.0, 25, "green"),
            mk_raw(2025, 2, 100.0, 100.0, 100.0, 25, "green"),
            mk_raw(2025, 3, 100.0, 100.0, 100.0, 25, "green"),
            mk_raw(2025, 4, 100.0, 100.0, 100.0, 28, "green"),  // 仍 green
        ];
        let out = BusinessIndicatorCore::new()
            .compute(&BusinessIndicatorSeries { points }, BusinessIndicatorParams::default()).unwrap();
        let n_streak = out.events.iter()
            .filter(|e| e.kind == BusinessIndicatorEventKind::MonitoringStreakInColor).count();
        assert_eq!(n_streak, 1, "color streak fires once at threshold");
    }

    #[test]
    fn produce_facts_uses_reserved_stock_id() {
        let points = vec![
            mk_raw(2025, 1, 100.0, 100.0, 100.0, 25, "green"),
            mk_raw(2025, 2, 101.0, 100.0, 100.0, 22, "yellow_blue"), // color change
        ];
        let core = BusinessIndicatorCore::new();
        let out = core.compute(&BusinessIndicatorSeries { points }, BusinessIndicatorParams::default()).unwrap();
        let facts = core.produce_facts(&out);
        assert!(!facts.is_empty());
        for f in &facts {
            assert_eq!(f.stock_id, "_index_business_");
            assert_eq!(f.source_core, "business_indicator_core");
        }
    }
}
