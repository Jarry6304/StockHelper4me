// revenue_core(P2)— Fundamental Core(月頻)
//
// 對齊 oldm2Spec/fundamental_cores.md(spec user m3Spec 還沒寫,暫 ref oldm2)。
//
// **本 PR 範圍**(極限推進):
//   - RevenueParams + Output + 4 EventKind(YoY 高 / 低 / 連續正成長 / 連續負成長)
//   - compute():逐筆組 series + 連續 streak detection
//   - produce_facts():對 YoY 異常 / streak 出 Fact
//
// TODO(後續討論):
//   - 預設 thresholds 由 best-guess 設(YoY ±20% / streak ≥ 3 月);user m3Spec 校準
//   - 月頻資料時間對齊:每月 10 號前發布,batch 排程留 user 設定

use anyhow::Result;
use chrono::NaiveDate;
use fact_schema::{Fact, IndicatorCore, Timeframe};
use fundamental_loader::MonthlyRevenueSeries;
use serde::Serialize;
use serde_json::json;

inventory::submit! {
    core_registry::CoreRegistration::new(
        "revenue_core",
        "0.1.0",
        core_registry::CoreKind::Fundamental,
        "P2",
        "Revenue Core(月營收 YoY/MoM + streak)",
    )
}

#[derive(Debug, Clone, Serialize)]
pub struct RevenueParams {
    pub timeframe: Timeframe,
    pub yoy_high_threshold: f64,    // 預設 20.0(%)
    pub yoy_low_threshold: f64,     // 預設 -20.0(%)
    pub streak_min_months: usize,   // 預設 3
}

impl Default for RevenueParams {
    fn default() -> Self {
        Self {
            timeframe: Timeframe::Monthly,
            yoy_high_threshold: 20.0,
            yoy_low_threshold: -20.0,
            streak_min_months: 3,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct RevenueOutput {
    pub stock_id: String,
    pub timeframe: Timeframe,
    pub series: Vec<RevenuePoint>,
    pub events: Vec<RevenueEvent>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RevenuePoint {
    pub date: NaiveDate,
    pub revenue: i64,
    pub revenue_yoy: f64,
    pub revenue_mom: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct RevenueEvent {
    pub date: NaiveDate,
    pub kind: RevenueEventKind,
    pub value: f64,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum RevenueEventKind {
    YoYExtremeHigh,
    YoYExtremeLow,
    PositiveYoYStreak,
    NegativeYoYStreak,
}

pub struct RevenueCore;
impl RevenueCore { pub fn new() -> Self { RevenueCore } }
impl Default for RevenueCore { fn default() -> Self { RevenueCore::new() } }

impl IndicatorCore for RevenueCore {
    type Input = MonthlyRevenueSeries;
    type Params = RevenueParams;
    type Output = RevenueOutput;

    fn name(&self) -> &'static str { "revenue_core" }
    fn version(&self) -> &'static str { "0.1.0" }

    fn compute(&self, input: &Self::Input, params: Self::Params) -> Result<Self::Output> {
        let series: Vec<RevenuePoint> = input.points.iter().map(|p| RevenuePoint {
            date: p.date,
            revenue: p.revenue.unwrap_or(0),
            revenue_yoy: p.revenue_yoy.unwrap_or(0.0),
            revenue_mom: p.revenue_mom.unwrap_or(0.0),
        }).collect();

        let mut events = Vec::new();
        for p in &series {
            if p.revenue_yoy >= params.yoy_high_threshold {
                events.push(RevenueEvent {
                    date: p.date,
                    kind: RevenueEventKind::YoYExtremeHigh,
                    value: p.revenue_yoy,
                    metadata: json!({ "yoy": p.revenue_yoy, "threshold": params.yoy_high_threshold }),
                });
            } else if p.revenue_yoy <= params.yoy_low_threshold {
                events.push(RevenueEvent {
                    date: p.date,
                    kind: RevenueEventKind::YoYExtremeLow,
                    value: p.revenue_yoy,
                    metadata: json!({ "yoy": p.revenue_yoy, "threshold": params.yoy_low_threshold }),
                });
            }
        }

        // YoY streak detection
        streak(&series, params.streak_min_months,
            |p| p.revenue_yoy > 0.0,
            RevenueEventKind::PositiveYoYStreak, &mut events);
        streak(&series, params.streak_min_months,
            |p| p.revenue_yoy < 0.0,
            RevenueEventKind::NegativeYoYStreak, &mut events);

        Ok(RevenueOutput {
            stock_id: input.stock_id.clone(),
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
            source_core: "revenue_core".to_string(),
            source_version: "0.1.0".to_string(),
            params_hash: None,
            statement: format!("{:?} on {}: value={:.2}", e.kind, e.date, e.value),
            metadata: e.metadata.clone(),
        }).collect()
    }

    fn warmup_periods(&self, _: &Self::Params) -> usize { 12 } // 12 個月,YoY 比較需上一年
}

fn streak(
    series: &[RevenuePoint],
    min_months: usize,
    pred: impl Fn(&RevenuePoint) -> bool,
    kind: RevenueEventKind,
    out: &mut Vec<RevenueEvent>,
) {
    let mut start: Option<usize> = None;
    for (i, p) in series.iter().enumerate() {
        if pred(p) {
            if start.is_none() { start = Some(i); }
        } else if let Some(s) = start.take() {
            let months = i - s;
            if months >= min_months {
                out.push(RevenueEvent {
                    date: series[i - 1].date,
                    kind,
                    value: months as f64,
                    metadata: json!({ "months": months, "start_date": series[s].date, "end_date": series[i - 1].date }),
                });
            }
        }
    }
    if let Some(s) = start {
        let months = series.len() - s;
        if months >= min_months {
            out.push(RevenueEvent {
                date: series.last().unwrap().date,
                kind,
                value: months as f64,
                metadata: json!({ "months": months, "start_date": series[s].date, "end_date": series.last().unwrap().date }),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fundamental_loader::MonthlyRevenueRaw;

    #[test]
    fn yoy_high_emitted() {
        let series = MonthlyRevenueSeries {
            stock_id: "2330".to_string(),
            points: vec![
                MonthlyRevenueRaw {
                    date: NaiveDate::parse_from_str("2026-04-30", "%Y-%m-%d").unwrap(),
                    revenue: Some(100_000_000),
                    revenue_yoy: Some(35.0),
                    revenue_mom: Some(5.0),
                },
            ],
        };
        let core = RevenueCore::new();
        let out = core.compute(&series, RevenueParams::default()).unwrap();
        assert!(out.events.iter().any(|e| e.kind == RevenueEventKind::YoYExtremeHigh));
    }

    #[test]
    fn name_version() {
        let core = RevenueCore::new();
        assert_eq!(core.name(), "revenue_core");
        assert_eq!(core.warmup_periods(&RevenueParams::default()), 12);
    }
}
