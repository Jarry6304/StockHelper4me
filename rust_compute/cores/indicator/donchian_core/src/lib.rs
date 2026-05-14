#![allow(clippy::needless_range_loop)]
// donchian_core(P3)— 對齊 m3Spec/indicator_cores_volatility.md §六
// Params §6.2 / warmup §6.3 / Output §6.4 / Fact §6.5
//
// Reference:
//   Richard Donchian (1960s) — 通道指標原作
//   Curtis Faith (2003), "Way of the Turtle" Ch.5:海龜系統 Donchian(20) 進場 + Donchian(55) 過濾
//   period=20:海龜系統短期入場
//
// Donchian Channel:
//   upper  = max(high) in last N bars
//   lower  = min(low) in last N bars
//   middle = (upper + lower) / 2

use anyhow::Result;
use chrono::NaiveDate;
use fact_schema::{Fact, IndicatorCore, Timeframe};
use ohlcv_loader::OhlcvSeries;
use serde::Serialize;
use serde_json::json;

inventory::submit! {
    core_registry::CoreRegistration::new(
        "donchian_core", "0.1.0", core_registry::CoreKind::Indicator, "P3",
        "Donchian Channel Core(N 日高低點通道,海龜系統)",
    )
}

#[derive(Debug, Clone, Serialize)]
pub struct DonchianParams {
    pub period: usize,
    pub timeframe: Timeframe,
}
impl Default for DonchianParams {
    fn default() -> Self {
        Self {
            period: 20,
            timeframe: Timeframe::Daily,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct DonchianOutput {
    pub stock_id: String,
    pub timeframe: Timeframe,
    pub series: Vec<DonchianPoint>,
    #[serde(skip)]
    pub events: Vec<DonchianEvent>,
}
#[derive(Debug, Clone, Serialize)]
pub struct DonchianPoint {
    pub date: NaiveDate,
    pub upper_band: f64,
    pub middle_band: f64,
    pub lower_band: f64,
}
#[derive(Debug, Clone, Serialize)]
pub struct DonchianEvent {
    pub date: NaiveDate,
    pub kind: DonchianEventKind,
    pub metadata: serde_json::Value,
}
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum DonchianEventKind {
    BreakoutUp,
    Breakdown,
}

pub struct DonchianCore;
impl DonchianCore {
    pub fn new() -> Self {
        DonchianCore
    }
}
impl Default for DonchianCore {
    fn default() -> Self {
        DonchianCore::new()
    }
}

impl IndicatorCore for DonchianCore {
    type Input = OhlcvSeries;
    type Params = DonchianParams;
    type Output = DonchianOutput;
    fn name(&self) -> &'static str {
        "donchian_core"
    }
    fn version(&self) -> &'static str {
        "0.1.0"
    }
    fn warmup_periods(&self, params: &Self::Params) -> usize {
        params.period + 5
    }

    fn compute(&self, input: &Self::Input, params: Self::Params) -> Result<Self::Output> {
        let n = input.bars.len();
        let p = params.period;
        let mut series = Vec::with_capacity(n);
        for i in 0..n {
            if i + 1 < p {
                series.push(DonchianPoint {
                    date: input.bars[i].date,
                    upper_band: input.bars[i].high,
                    middle_band: input.bars[i].close,
                    lower_band: input.bars[i].low,
                });
                continue;
            }
            let start = i + 1 - p;
            // 注意:Donchian 慣例是「過去 N 天」不含當天,但我們用「含當天 N 天」這個版本
            // 較為穩健且與多數實作一致(避免單純 lookback 取到未來 0 bar 的邊界 case)
            let hi = input.bars[start..=i]
                .iter()
                .map(|b| b.high)
                .fold(f64::NEG_INFINITY, f64::max);
            let lo = input.bars[start..=i]
                .iter()
                .map(|b| b.low)
                .fold(f64::INFINITY, f64::min);
            series.push(DonchianPoint {
                date: input.bars[i].date,
                upper_band: hi,
                middle_band: (hi + lo) / 2.0,
                lower_band: lo,
            });
        }

        let mut events = Vec::new();
        // 突破:close > 過去 N-1 天的最高 high(不含當天)— 對齊海龜系統「突破前 N 天高點」語意
        for i in p..n {
            let prev_window = &input.bars[i - p + 1..i]; // 過去 p-1 天(不含當天)
            let prev_high = prev_window.iter().map(|b| b.high).fold(f64::NEG_INFINITY, f64::max);
            let prev_low = prev_window.iter().map(|b| b.low).fold(f64::INFINITY, f64::min);
            let close = input.bars[i].close;
            if close > prev_high {
                events.push(DonchianEvent {
                    date: series[i].date,
                    kind: DonchianEventKind::BreakoutUp,
                    metadata: json!({"event": "donchian_breakout_up", "period": p, "close": close, "prev_high": prev_high}),
                });
            } else if close < prev_low {
                events.push(DonchianEvent {
                    date: series[i].date,
                    kind: DonchianEventKind::Breakdown,
                    metadata: json!({"event": "donchian_breakdown", "period": p, "close": close, "prev_low": prev_low}),
                });
            }
        }

        Ok(DonchianOutput {
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
            .map(|e| Fact {
                stock_id: output.stock_id.clone(),
                fact_date: e.date,
                timeframe: output.timeframe,
                source_core: "donchian_core".to_string(),
                source_version: "0.1.0".to_string(),
                params_hash: None,
                statement: format!("Donchian {:?} on {}", e.kind, e.date),
                metadata: e.metadata.clone(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use fact_schema::Timeframe;
    use ohlcv_loader::{OhlcvBar, OhlcvSeries};

    fn make_series(bars: Vec<(f64, f64, f64)>) -> OhlcvSeries {
        let bars = bars
            .into_iter()
            .enumerate()
            .map(|(i, (h, l, c))| OhlcvBar {
                date: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap()
                    + chrono::Duration::days(i as i64),
                open: c,
                high: h,
                low: l,
                close: c,
                volume: Some(1000),
            })
            .collect();
        OhlcvSeries {
            stock_id: "TEST".to_string(),
            timeframe: Timeframe::Daily,
            bars,
        }
    }

    #[test]
    fn name_and_warmup() {
        let core = DonchianCore::new();
        assert_eq!(core.name(), "donchian_core");
        assert_eq!(core.warmup_periods(&DonchianParams::default()), 25);
    }

    #[test]
    fn breakout_fires_when_close_exceeds_prev_high() {
        let core = DonchianCore::new();
        // 25 bars 都是 high=100, low=90, close=95;第 26 bar 跳到 close=105 突破
        let mut bars: Vec<(f64, f64, f64)> = (0..25).map(|_| (100.0, 90.0, 95.0)).collect();
        bars.push((110.0, 100.0, 105.0));
        let series = make_series(bars);
        let out = core.compute(&series, DonchianParams::default()).unwrap();
        let breakouts: Vec<_> = out
            .events
            .iter()
            .filter(|e| e.kind == DonchianEventKind::BreakoutUp)
            .collect();
        assert!(!breakouts.is_empty(), "should detect breakout up");
    }
}
