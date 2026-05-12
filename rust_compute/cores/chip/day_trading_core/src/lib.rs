// day_trading_core(P2)— Chip Core
//
// 對齊 m3Spec/chip_cores.md §七 day_trading_core(完整 spec,user 已在 m3Spec/ 寫定)。
//
// 定位(§7.1):當沖統計、當沖比率、當沖力道。
// 上游 Silver(§7.2):day_trading_derived(market, stock_id, date)
//   - day_trading_buy / day_trading_sell / day_trading_ratio
//
// **M3 PR-CC1 階段**(本 PR 完整實作):
//   - DayTradingParams + warmup_periods 對齊 §7.3-7.4
//   - DayTradingOutput { series, events } 對齊 §7.5
//   - 4 個 EventKind:RatioExtremeHigh / Low / RatioStreakHigh / Low
//   - produce_facts() 對齊 §7.6 範例
//   - momentum 計算:用 ratio 的 SMA 對前 momentum_lookback 天比較(自定義版,
//     對齊 §7.5 註「momentum 可自定義」)

use anyhow::Result;
use chrono::NaiveDate;
use chip_loader::DayTradingSeries;
use fact_schema::{Fact, IndicatorCore, Timeframe};
use serde::Serialize;
use serde_json::json;

// inventory 註冊(對齊 cores_overview §五)
inventory::submit! {
    core_registry::CoreRegistration::new(
        "day_trading_core",
        "0.1.0",
        core_registry::CoreKind::Chip,
        "P2",
        "Day Trading Core(當沖統計 / 比率 / 力道)",
    )
}

// ---------------------------------------------------------------------------
// Params
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct DayTradingParams {
    pub timeframe: Timeframe,
    /// 當沖比率高閾值(%),預設 30.0
    pub ratio_high_threshold: f64,
    /// 當沖比率低閾值(%),預設 5.0
    pub ratio_low_threshold: f64,
    /// 當沖力道回看,預設 5
    pub momentum_lookback: usize,
}

/// 連續高/低當沖比的最小天數(對齊 spec §7.5 streak EventKind;§7.3 Params 未列,寫死 const)
const STREAK_MIN_DAYS: usize = 3;

impl Default for DayTradingParams {
    fn default() -> Self {
        Self {
            timeframe: Timeframe::Daily,
            ratio_high_threshold: 30.0,
            ratio_low_threshold: 5.0,
            momentum_lookback: 5,
        }
    }
}

// ---------------------------------------------------------------------------
// Output
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct DayTradingOutput {
    pub stock_id: String,
    pub timeframe: Timeframe,
    pub series: Vec<DayTradingPoint>,
    pub events: Vec<DayTradingEvent>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DayTradingPoint {
    pub date: NaiveDate,
    /// 當沖股數(對齊 spec §7.5)。
    /// Silver day_trading_derived 沒直接欄位,best-guess:= day_trading_buy(實務上 buy ≈ sell)
    pub day_trade_volume: i64,
    /// 全日總成交股數(對齊 spec §7.5)。
    /// Silver day_trading_derived 沒直接欄位,從 ratio 反推:= day_trade_volume × 100 / ratio(if ratio > 0)
    pub total_volume: i64,
    pub day_trade_ratio: f64, // %
    pub day_trade_buy: i64,   // 當沖買進(原 Silver day_trading_buy)
    pub day_trade_sell: i64,  // 當沖賣出
    pub momentum: f64,        // 當沖力道:ratio diff vs SMA(N) 前 N 期
}

#[derive(Debug, Clone, Serialize)]
pub struct DayTradingEvent {
    pub date: NaiveDate,
    pub kind: DayTradingEventKind,
    pub value: f64,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum DayTradingEventKind {
    RatioExtremeHigh,
    RatioExtremeLow,
    RatioStreakHigh,
    RatioStreakLow,
}

// ---------------------------------------------------------------------------
// Core
// ---------------------------------------------------------------------------

pub struct DayTradingCore;

impl DayTradingCore {
    pub fn new() -> Self {
        DayTradingCore
    }
}

impl Default for DayTradingCore {
    fn default() -> Self {
        DayTradingCore::new()
    }
}

impl IndicatorCore for DayTradingCore {
    type Input = DayTradingSeries;
    type Params = DayTradingParams;
    type Output = DayTradingOutput;

    fn name(&self) -> &'static str {
        "day_trading_core"
    }

    fn version(&self) -> &'static str {
        "0.1.0"
    }

    fn compute(&self, input: &Self::Input, params: Self::Params) -> Result<Self::Output> {
        let mut series: Vec<DayTradingPoint> = Vec::with_capacity(input.points.len());

        // Pass 1:逐筆組 series,momentum 用「當前 ratio - 前 lookback 期 ratio 平均」
        for (i, p) in input.points.iter().enumerate() {
            let ratio = p.day_trading_ratio.unwrap_or(0.0);
            let momentum = compute_momentum(&input.points, i, params.momentum_lookback);
            let buy = p.day_trading_buy.unwrap_or(0);
            let sell = p.day_trading_sell.unwrap_or(0);
            // day_trade_volume:當沖股數;Silver 沒直接欄位,best-guess 取 buy(buy ≈ sell)
            let day_trade_volume = buy;
            // total_volume:從 ratio 反推 — ratio = volume × 100 / total → total = volume × 100 / ratio
            let total_volume = if ratio > 0.0 {
                ((day_trade_volume as f64) * 100.0 / ratio).round() as i64
            } else {
                0
            };
            series.push(DayTradingPoint {
                date: p.date,
                day_trade_volume,
                total_volume,
                day_trade_ratio: ratio,
                day_trade_buy: buy,
                day_trade_sell: sell,
                momentum,
            });
        }

        // Pass 2:event 偵測
        let events = detect_events(&series, &params);

        Ok(DayTradingOutput {
            stock_id: input.stock_id.clone(),
            timeframe: params.timeframe,
            series,
            events,
        })
    }

    fn produce_facts(&self, output: &Self::Output) -> Vec<Fact> {
        output
            .events
            .iter()
            .map(|e| event_to_fact(output, e))
            .collect()
    }

    fn warmup_periods(&self, _params: &Self::Params) -> usize {
        // §7.4:固定 20 個交易日(連續 N 日高當沖比偵測所需的最小窗口)
        20
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn compute_momentum(points: &[chip_loader::DayTradingRaw], i: usize, lookback: usize) -> f64 {
    if i < lookback || lookback == 0 {
        return 0.0;
    }
    let cur = points[i].day_trading_ratio.unwrap_or(0.0);
    let sum: f64 = points[i - lookback..i]
        .iter()
        .map(|p| p.day_trading_ratio.unwrap_or(0.0))
        .sum();
    let avg = sum / lookback as f64;
    cur - avg
}

fn detect_events(series: &[DayTradingPoint], params: &DayTradingParams) -> Vec<DayTradingEvent> {
    let mut events = Vec::new();

    // RatioExtremeHigh / Low — edge trigger: fire only on zone entry, not on every bar in zone.
    // Brown & Warner (1985): 事件 = 狀態轉換(crossing into zone), 連續停留不算新事件。
    let mut was_extreme_high = false;
    let mut was_extreme_low = false;
    for (i, p) in series.iter().enumerate() {
        let is_extreme_high = p.day_trade_ratio >= params.ratio_high_threshold;
        let is_extreme_low = p.day_trade_ratio > 0.0 && p.day_trade_ratio <= params.ratio_low_threshold;

        if is_extreme_high && !was_extreme_high {
            let historical_high = series[..i]
                .iter()
                .all(|prev| p.day_trade_ratio > prev.day_trade_ratio);
            events.push(DayTradingEvent {
                date: p.date,
                kind: DayTradingEventKind::RatioExtremeHigh,
                value: p.day_trade_ratio,
                metadata: json!({
                    "ratio": p.day_trade_ratio,
                    "threshold": params.ratio_high_threshold,
                    "historical_high": historical_high,
                }),
            });
        } else if is_extreme_low && !was_extreme_low {
            // ratio == 0 多半是缺資料 / 無交易,不算 extreme low
            events.push(DayTradingEvent {
                date: p.date,
                kind: DayTradingEventKind::RatioExtremeLow,
                value: p.day_trade_ratio,
                metadata: json!({
                    "ratio": p.day_trade_ratio,
                    "threshold": params.ratio_low_threshold,
                }),
            });
        }
        was_extreme_high = is_extreme_high;
        was_extreme_low = is_extreme_low;
    }

    // RatioStreakHigh / Low(連續 N 天高 / 低)
    detect_streak(
        series,
        STREAK_MIN_DAYS,
        |p| p.day_trade_ratio >= params.ratio_high_threshold,
        DayTradingEventKind::RatioStreakHigh,
        params.ratio_high_threshold,
        &mut events,
    );
    detect_streak(
        series,
        STREAK_MIN_DAYS,
        |p| p.day_trade_ratio > 0.0 && p.day_trade_ratio <= params.ratio_low_threshold,
        DayTradingEventKind::RatioStreakLow,
        params.ratio_low_threshold,
        &mut events,
    );

    events
}

fn detect_streak(
    series: &[DayTradingPoint],
    min_days: usize,
    predicate: impl Fn(&DayTradingPoint) -> bool,
    kind: DayTradingEventKind,
    threshold: f64,
    out: &mut Vec<DayTradingEvent>,
) {
    let mut streak_start: Option<usize> = None;
    for (i, p) in series.iter().enumerate() {
        if predicate(p) {
            if streak_start.is_none() {
                streak_start = Some(i);
            }
        } else if let Some(start) = streak_start.take() {
            let days = i - start;
            if days >= min_days {
                let end_p = &series[i - 1];
                out.push(DayTradingEvent {
                    date: end_p.date,
                    kind,
                    value: days as f64,
                    metadata: json!({
                        "days": days,
                        "threshold": threshold,
                        "start_date": series[start].date,
                        "end_date": end_p.date,
                    }),
                });
            }
        }
    }
    // 序列尾端仍在 streak 中
    if let Some(start) = streak_start {
        let days = series.len() - start;
        if days >= min_days {
            let end_p = series.last().unwrap();
            out.push(DayTradingEvent {
                date: end_p.date,
                kind,
                value: days as f64,
                metadata: json!({
                    "days": days,
                    "threshold": threshold,
                    "start_date": series[start].date,
                    "end_date": end_p.date,
                }),
            });
        }
    }
}

fn event_to_fact(output: &DayTradingOutput, event: &DayTradingEvent) -> Fact {
    let statement = match event.kind {
        DayTradingEventKind::RatioExtremeHigh => format!(
            "Day trade ratio reached {:.1}% on {}(extreme high)",
            event.value, event.date
        ),
        DayTradingEventKind::RatioExtremeLow => format!(
            "Day trade ratio dropped to {:.1}% on {}(extreme low)",
            event.value, event.date
        ),
        DayTradingEventKind::RatioStreakHigh => format!(
            "Day trade ratio above threshold for {} consecutive days ending on {}",
            event.value as i64, event.date
        ),
        DayTradingEventKind::RatioStreakLow => format!(
            "Day trade ratio below threshold for {} consecutive days ending on {}",
            event.value as i64, event.date
        ),
    };

    Fact {
        stock_id: output.stock_id.clone(),
        fact_date: event.date,
        timeframe: output.timeframe,
        source_core: "day_trading_core".to_string(),
        source_version: "0.1.0".to_string(),
        params_hash: None,
        statement,
        metadata: event.metadata.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chip_loader::DayTradingRaw;

    fn point(d: &str, ratio: f64) -> DayTradingRaw {
        DayTradingRaw {
            date: NaiveDate::parse_from_str(d, "%Y-%m-%d").unwrap(),
            day_trading_buy: Some(1_000_000),
            day_trading_sell: Some(900_000),
            day_trading_ratio: Some(ratio),
        }
    }

    fn make_series(ratios: &[(&str, f64)]) -> DayTradingSeries {
        DayTradingSeries {
            stock_id: "2330".to_string(),
            points: ratios.iter().map(|(d, r)| point(d, *r)).collect(),
        }
    }

    #[test]
    fn extreme_high_event_emitted() {
        let series = make_series(&[
            ("2026-04-22", 38.0),
            ("2026-04-23", 25.0),
        ]);
        let core = DayTradingCore::new();
        let out = core.compute(&series, DayTradingParams::default()).unwrap();
        assert_eq!(out.events.len(), 1);
        assert_eq!(out.events[0].kind, DayTradingEventKind::RatioExtremeHigh);
        assert!((out.events[0].value - 38.0).abs() < 1e-9);
    }

    #[test]
    fn extreme_low_event_emitted() {
        let series = make_series(&[
            ("2026-04-28", 4.2),
        ]);
        let core = DayTradingCore::new();
        let out = core.compute(&series, DayTradingParams::default()).unwrap();
        assert_eq!(out.events.len(), 1);
        assert_eq!(out.events[0].kind, DayTradingEventKind::RatioExtremeLow);
    }

    #[test]
    fn streak_high_emitted_after_3_days() {
        // 5 連續高 ratio + 1 normal day → 1 個 RatioStreakHigh + 1 個 RatioExtremeHigh(edge entry)
        let series = make_series(&[
            ("2026-04-22", 35.0),
            ("2026-04-23", 36.0),
            ("2026-04-24", 33.0),
            ("2026-04-25", 31.0),
            ("2026-04-26", 32.0),
            ("2026-04-27", 25.0),
        ]);
        let core = DayTradingCore::new();
        let out = core.compute(&series, DayTradingParams::default()).unwrap();
        let streaks: Vec<_> = out
            .events
            .iter()
            .filter(|e| e.kind == DayTradingEventKind::RatioStreakHigh)
            .collect();
        assert_eq!(streaks.len(), 1);
        assert!((streaks[0].value - 5.0).abs() < 1e-9, "5 連續高");
    }

    #[test]
    fn momentum_uses_lookback() {
        let series = make_series(&[
            ("2026-04-21", 10.0),
            ("2026-04-22", 10.0),
            ("2026-04-23", 10.0),
            ("2026-04-24", 10.0),
            ("2026-04-25", 10.0),
            ("2026-04-26", 20.0), // momentum = 20 - 10 = 10
        ]);
        let core = DayTradingCore::new();
        let out = core.compute(&series, DayTradingParams::default()).unwrap();
        assert!((out.series[5].momentum - 10.0).abs() < 1e-9);
        // 前 5 個 momentum = 0(lookback 不足)
        for i in 0..5 {
            assert_eq!(out.series[i].momentum, 0.0);
        }
    }

    #[test]
    fn produce_facts_returns_one_per_event() {
        let series = make_series(&[
            ("2026-04-22", 38.0),
            ("2026-04-28", 4.2),
        ]);
        let core = DayTradingCore::new();
        let out = core.compute(&series, DayTradingParams::default()).unwrap();
        let facts = core.produce_facts(&out);
        assert_eq!(facts.len(), 2);
        assert!(facts[0].statement.contains("38.0%"));
        assert!(facts[0].statement.contains("extreme high"));
        assert!(facts[1].statement.contains("4.2%"));
        assert!(facts[1].statement.contains("extreme low"));
        assert!(facts.iter().all(|f| f.source_core == "day_trading_core"));
        assert!(facts.iter().all(|f| f.stock_id == "2330"));
    }

    #[test]
    fn warmup_periods_is_20() {
        let core = DayTradingCore::new();
        assert_eq!(core.warmup_periods(&DayTradingParams::default()), 20);
    }

    #[test]
    fn empty_series_yields_no_events() {
        let series = DayTradingSeries {
            stock_id: "2330".to_string(),
            points: Vec::new(),
        };
        let core = DayTradingCore::new();
        let out = core.compute(&series, DayTradingParams::default()).unwrap();
        assert!(out.series.is_empty());
        assert!(out.events.is_empty());
    }

    #[test]
    fn extreme_high_edge_trigger_fires_only_on_zone_entry() {
        // 3 days in extreme zone → 1 event on entry; exit then re-enter → 2nd event
        let series = make_series(&[
            ("2026-04-22", 35.0), // entry → fires
            ("2026-04-23", 36.0), // stays in zone → no fire
            ("2026-04-24", 33.0), // stays in zone → no fire
            ("2026-04-25", 20.0), // exits zone
            ("2026-04-26", 32.0), // re-entry → fires again
        ]);
        let core = DayTradingCore::new();
        let out = core.compute(&series, DayTradingParams::default()).unwrap();
        let extremes: Vec<_> = out
            .events
            .iter()
            .filter(|e| e.kind == DayTradingEventKind::RatioExtremeHigh)
            .collect();
        assert_eq!(extremes.len(), 2, "entry + re-entry = 2 fires, not 5");
    }

    #[test]
    fn name_and_version_stable() {
        let core = DayTradingCore::new();
        assert_eq!(core.name(), "day_trading_core");
        assert_eq!(core.version(), "0.1.0");
    }
}
