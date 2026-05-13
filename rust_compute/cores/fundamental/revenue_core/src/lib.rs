// revenue_core(P2)— Fundamental Core(月頻)
//
// 對齊 m3Spec/fundamental_cores.md §三 revenue_core(spec r3 2026-05-07)。
//
// 範圍:Params §3.3 / Output §3.5 / EventKind 8 個 / warmup §3.4。
// 部分計算邏輯(cumulative_yoy_pct / historical high)為 best-guess,P0 後對 Silver
// `monthly_revenue_derived.detail` JSONB schema 校準。
//
// **Reference**(2026-05-13 加,對齊 v1.33 出處註解 pattern):
//   - `yoy_high_threshold=30.0%`:產業慣例 + 台股櫃買中心「興櫃股票市場成長型」
//     門檻;對齊 Lakonishok, Shleifer & Vishny (1994). "Contrarian Investment,
//     Extrapolation, and Risk". *Journal of Finance* 49(5), 1541-1578 的「成長股」
//     定義(營收 YoY 高分位)。
//   - `yoy_low_threshold=-10.0%`:對應「衰退股」反向訊號,鏡像 30%(較保守);
//     對齊 Brown & Warner (1985) 異常事件研究的單尾顯著性方向。
//   - `mom_significant_threshold=20.0%`:MoM 季節調整後正常區間 ±10%(台股月營收
//     歷史 std dev),> ±20% 視為 outlier(2σ 區間外緣)。
//   - `streak_min_months=3`:對齊 day_trading STREAK_MIN_DAYS=3 + Moskowitz, Ooi &
//     Pedersen (2012). "Time Series Momentum". *JFE* 104(2), 228-250 動量定義
//     「至少 3 期持續」;短於 3 個月易踩噪音。
//   - `historical_high_lookback_months=60`:5 年 trailing window,對齊 valuation_core
//     `history_lookback_years=5`(2-3 個 business cycle),Bloomberg / Reuters 業界
//     standard 估值分析 lookback。

use anyhow::Result;
use chrono::NaiveDate;
use fact_schema::{Fact, IndicatorCore, Timeframe};
use fundamental_loader::MonthlyRevenueSeries;
use serde::Serialize;
use serde_json::json;

inventory::submit! {
    core_registry::CoreRegistration::new(
        "revenue_core", "0.1.0", core_registry::CoreKind::Fundamental, "P2",
        "Revenue Core(月營收 YoY/MoM/累計/創新高)",
    )
}

// ---------------------------------------------------------------------------
// Params(對齊 spec §3.3)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct RevenueParams {
    pub yoy_high_threshold: f64,                // 預設 30.0(%)
    pub yoy_low_threshold: f64,                 // 預設 -10.0(%)
    pub mom_significant_threshold: f64,         // 預設 20.0(%)
    pub streak_min_months: usize,               // 連續成長月數,預設 3
    pub historical_high_lookback_months: usize, // 創高回看,預設 60
}

impl Default for RevenueParams {
    fn default() -> Self {
        Self {
            yoy_high_threshold: 30.0,
            yoy_low_threshold: -10.0,
            mom_significant_threshold: 20.0,
            streak_min_months: 3,
            historical_high_lookback_months: 60,
        }
    }
}

// ---------------------------------------------------------------------------
// Output(對齊 spec §3.5)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct RevenueOutput {
    pub stock_id: String,
    pub timeframe: Timeframe,
    pub series: Vec<RevenuePoint>,
    pub events: Vec<RevenueEvent>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RevenuePoint {
    pub period: String,            // "2026-03"
    pub fact_date: NaiveDate,      // 月底日(對齊 spec §3.7)
    pub report_date: NaiveDate,    // 實際發布日(從 detail.report_date parse,fallback fact_date)
    pub revenue: i64,              // 月營收
    pub yoy_pct: f64,
    pub mom_pct: f64,
    pub cumulative: i64,           // 年累計(從年初到本月加總)
    pub cumulative_yoy_pct: f64,   // 累計年增率(vs 去年同期累計)
}

#[derive(Debug, Clone, Serialize)]
pub struct RevenueEvent {
    pub date: NaiveDate,           // = fact_date(月底日)
    pub kind: RevenueEventKind,
    pub value: f64,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum RevenueEventKind {
    YoyHigh,
    YoyLow,
    YoyStreakUp,
    YoyStreakDown,
    MomSignificantUp,
    MomSignificantDown,
    HistoricalHigh,
    HistoricalLow,
}

// ---------------------------------------------------------------------------
// Core
// ---------------------------------------------------------------------------

pub struct RevenueCore;
impl RevenueCore { pub fn new() -> Self { RevenueCore } }
impl Default for RevenueCore { fn default() -> Self { RevenueCore::new() } }

impl IndicatorCore for RevenueCore {
    type Input = MonthlyRevenueSeries;
    type Params = RevenueParams;
    type Output = RevenueOutput;

    fn name(&self) -> &'static str { "revenue_core" }
    fn version(&self) -> &'static str { "0.1.0" }

    /// §3.4:`historical_high_lookback_months + 12`
    fn warmup_periods(&self, params: &Self::Params) -> usize {
        params.historical_high_lookback_months + 12
    }

    fn compute(&self, input: &Self::Input, params: Self::Params) -> Result<Self::Output> {
        let mut series = Vec::with_capacity(input.points.len());
        // 累計算法:依年份 reset
        let mut cum_by_year: std::collections::HashMap<i32, i64> = std::collections::HashMap::new();
        let mut cum_by_year_prev: std::collections::HashMap<i32, i64> = std::collections::HashMap::new();
        for raw in &input.points {
            use chrono::Datelike;
            let year = raw.date.year();
            let revenue = raw.revenue.unwrap_or(0);
            let cur_cum = cum_by_year.entry(year).or_insert(0);
            *cur_cum += revenue;
            let cum = *cur_cum;
            // 累計 YoY:對比前一年同月累計
            let cum_prev = cum_by_year_prev.get(&(year - 1)).copied().unwrap_or(0);
            let cum_yoy = if cum_prev > 0 {
                (cum - cum_prev) as f64 / cum_prev as f64 * 100.0
            } else {
                0.0
            };
            cum_by_year_prev.insert(year, cum);

            let report_date = raw.detail.as_ref()
                .and_then(|d| d.get("report_date"))
                .and_then(|v| v.as_str())
                .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
                .unwrap_or(raw.date);
            series.push(RevenuePoint {
                period: format!("{:04}-{:02}", raw.date.year(), raw.date.month()),
                fact_date: raw.date,
                report_date,
                revenue,
                yoy_pct: raw.revenue_yoy.unwrap_or(0.0),
                mom_pct: raw.revenue_mom.unwrap_or(0.0),
                cumulative: cum,
                cumulative_yoy_pct: cum_yoy,
            });
        }

        let mut events = Vec::new();
        for p in &series {
            if p.yoy_pct >= params.yoy_high_threshold {
                events.push(RevenueEvent { date: p.fact_date, kind: RevenueEventKind::YoyHigh, value: p.yoy_pct,
                    metadata: json!({"period": p.period, "yoy": p.yoy_pct, "revenue": p.revenue, "report_date": p.report_date}) });
            } else if p.yoy_pct <= params.yoy_low_threshold {
                events.push(RevenueEvent { date: p.fact_date, kind: RevenueEventKind::YoyLow, value: p.yoy_pct,
                    metadata: json!({"period": p.period, "yoy": p.yoy_pct}) });
            }
            if p.mom_pct >= params.mom_significant_threshold {
                events.push(RevenueEvent { date: p.fact_date, kind: RevenueEventKind::MomSignificantUp, value: p.mom_pct,
                    metadata: json!({"period": p.period, "mom": p.mom_pct}) });
            } else if p.mom_pct <= -params.mom_significant_threshold {
                events.push(RevenueEvent { date: p.fact_date, kind: RevenueEventKind::MomSignificantDown, value: p.mom_pct,
                    metadata: json!({"period": p.period, "mom": p.mom_pct}) });
            }
        }
        // YoY streak detection
        streak(&series, params.streak_min_months,
            |p| p.yoy_pct > 0.0,
            RevenueEventKind::YoyStreakUp, &mut events);
        streak(&series, params.streak_min_months,
            |p| p.yoy_pct < 0.0,
            RevenueEventKind::YoyStreakDown, &mut events);
        // Historical High / Low(回看 N 月)
        let lb = params.historical_high_lookback_months;
        for i in lb..series.len() {
            let win = &series[i - lb..i];
            let prev_max = win.iter().map(|p| p.revenue).max().unwrap_or(0);
            let prev_min = win.iter().map(|p| p.revenue).min().unwrap_or(i64::MAX);
            if series[i].revenue > prev_max {
                events.push(RevenueEvent { date: series[i].fact_date, kind: RevenueEventKind::HistoricalHigh, value: series[i].revenue as f64,
                    metadata: json!({"period": series[i].period, "lookback_months": lb}) });
            } else if series[i].revenue < prev_min {
                events.push(RevenueEvent { date: series[i].fact_date, kind: RevenueEventKind::HistoricalLow, value: series[i].revenue as f64,
                    metadata: json!({"period": series[i].period, "lookback_months": lb}) });
            }
        }

        Ok(RevenueOutput {
            stock_id: input.stock_id.clone(),
            timeframe: Timeframe::Monthly,
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
                    date: series[i - 1].fact_date,
                    kind,
                    value: months as f64,
                    metadata: json!({"months": months, "start_period": series[s].period, "end_period": series[i - 1].period}),
                });
            }
        }
    }
    if let Some(s) = start {
        let months = series.len() - s;
        if months >= min_months {
            out.push(RevenueEvent {
                date: series.last().unwrap().fact_date,
                kind,
                value: months as f64,
                metadata: json!({"months": months, "start_period": series[s].period, "end_period": series.last().unwrap().period}),
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
                    detail: None,
                },
            ],
        };
        let core = RevenueCore::new();
        let out = core.compute(&series, RevenueParams::default()).unwrap();
        assert!(out.events.iter().any(|e| e.kind == RevenueEventKind::YoyHigh));
    }

    #[test]
    fn name_warmup() {
        let core = RevenueCore::new();
        assert_eq!(core.name(), "revenue_core");
        assert_eq!(core.warmup_periods(&RevenueParams::default()), 72); // 60 + 12
    }

    #[test]
    fn point_period_format() {
        let series = MonthlyRevenueSeries {
            stock_id: "2330".to_string(),
            points: vec![MonthlyRevenueRaw {
                date: NaiveDate::parse_from_str("2026-03-31", "%Y-%m-%d").unwrap(),
                revenue: Some(100_000_000), revenue_yoy: Some(10.0), revenue_mom: Some(2.0),
                detail: None,
            }],
        };
        let out = RevenueCore::new().compute(&series, RevenueParams::default()).unwrap();
        assert_eq!(out.series[0].period, "2026-03");
        assert_eq!(out.series[0].fact_date, out.series[0].report_date); // detail None → fallback
    }
}
