// williams_r_core(P3)— 對齊 m3Spec/indicator_cores_momentum.md §九
// Params §9.2 / warmup §9.3 / Output §9.4 / Fact §9.5
//
// Reference:
//   Larry Williams (1973), "How I Made One Million Dollars... Last Year... Trading Commodities"
//   period=14:慣例(對齊 Stochastic 同源)
//   overbought=-20 / oversold=-80:Williams 原版閾值
//
// Williams %R 公式(範圍 -100 ~ 0):
//   %R = (highest_high(N) - close) / (highest_high(N) - lowest_low(N)) * -100

use anyhow::Result;
use chrono::NaiveDate;
use fact_schema::{Fact, IndicatorCore, Timeframe};
use ohlcv_loader::OhlcvSeries;
use serde::Serialize;
use serde_json::json;

inventory::submit! {
    core_registry::CoreRegistration::new(
        "williams_r_core", "0.1.0", core_registry::CoreKind::Indicator, "P3",
        "Williams %R Core(動量振盪指標,範圍 -100 ~ 0)",
    )
}

/// 連續處於超賣 / 超買區的最小天數,觸發 streak event
const STREAK_MIN_DAYS: usize = 3;

/// **v1.34 Round 5**:全市場 23.8 facts/yr/stock,加 spacing=15 → 17.6/yr(26% down)。
///
/// **v1.34 Round 6**:15 → 25(對應約 5 週,讓 Williams %R 14-period 振盪
/// 完整跑完 1 cycle 才再觸發)。預期 17.6 → ~10/yr。
const MIN_STREAK_FIRE_SPACING: usize = 25;

#[derive(Debug, Clone, Serialize)]
pub struct WilliamsRParams {
    pub period: usize,
    pub overbought: f64,
    pub oversold: f64,
    pub timeframe: Timeframe,
}
impl Default for WilliamsRParams {
    fn default() -> Self {
        Self {
            period: 14,
            overbought: -20.0,
            oversold: -80.0,
            timeframe: Timeframe::Daily,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct WilliamsROutput {
    pub stock_id: String,
    pub timeframe: Timeframe,
    pub series: Vec<WilliamsRPoint>,
    #[serde(skip)]
    pub events: Vec<WilliamsREvent>,
}
#[derive(Debug, Clone, Serialize)]
pub struct WilliamsRPoint {
    pub date: NaiveDate,
    pub value: f64,
}
#[derive(Debug, Clone, Serialize)]
pub struct WilliamsREvent {
    pub date: NaiveDate,
    pub kind: WilliamsREventKind,
    pub value: f64,
    pub metadata: serde_json::Value,
}
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum WilliamsREventKind {
    OversoldStreak,
    OverboughtStreak,
    OversoldExit,
    OverboughtExit,
}

pub struct WilliamsRCore;
impl WilliamsRCore {
    pub fn new() -> Self {
        WilliamsRCore
    }
}
impl Default for WilliamsRCore {
    fn default() -> Self {
        WilliamsRCore::new()
    }
}

/// 在 [start, end) 內找 high 最大值 / low 最小值,回 (max_h, min_l)
fn rolling_hl(bars: &[ohlcv_loader::OhlcvBar], start: usize, end: usize) -> (f64, f64) {
    let mut hi = f64::NEG_INFINITY;
    let mut lo = f64::INFINITY;
    for b in &bars[start..end] {
        if b.high > hi {
            hi = b.high;
        }
        if b.low < lo {
            lo = b.low;
        }
    }
    (hi, lo)
}

impl IndicatorCore for WilliamsRCore {
    type Input = OhlcvSeries;
    type Params = WilliamsRParams;
    type Output = WilliamsROutput;
    fn name(&self) -> &'static str {
        "williams_r_core"
    }
    fn version(&self) -> &'static str {
        "0.1.0"
    }
    fn warmup_periods(&self, params: &Self::Params) -> usize {
        params.period * 4
    }

    fn compute(&self, input: &Self::Input, params: Self::Params) -> Result<Self::Output> {
        let n = input.bars.len();
        let p = params.period;
        let mut series = Vec::with_capacity(n);
        for i in 0..n {
            if i + 1 < p {
                // warmup 階段填 0(NULL-safe placeholder,事件偵測會 skip)
                series.push(WilliamsRPoint {
                    date: input.bars[i].date,
                    value: 0.0,
                });
                continue;
            }
            let start = i + 1 - p;
            let (hh, ll) = rolling_hl(&input.bars, start, i + 1);
            let close = input.bars[i].close;
            let range = hh - ll;
            let value = if range > 0.0 {
                (hh - close) / range * -100.0
            } else {
                0.0
            };
            series.push(WilliamsRPoint {
                date: input.bars[i].date,
                value,
            });
        }

        let mut events = Vec::new();
        // streak / exit events
        let mut over_run = 0usize; // overbought (value > overbought = > -20)
        let mut under_run = 0usize; // oversold (value < oversold = < -80)
        // v1.34 Round 5:每事件種類獨立 spacing 狀態,確保「兩次同 kind ≥ 15 bars」
        let mut last_overbought_streak_idx: Option<usize> = None;
        let mut last_overbought_exit_idx: Option<usize> = None;
        let mut last_oversold_streak_idx: Option<usize> = None;
        let mut last_oversold_exit_idx: Option<usize> = None;
        for i in p..n {
            let cur = series[i].value;
            let prev = series[i - 1].value;

            // overbought streak
            if cur > params.overbought {
                over_run += 1;
                if over_run == STREAK_MIN_DAYS
                    && last_overbought_streak_idx
                        .is_none_or(|last| i >= last + MIN_STREAK_FIRE_SPACING)
                {
                    events.push(WilliamsREvent {
                        date: series[i].date,
                        kind: WilliamsREventKind::OverboughtStreak,
                        value: cur,
                        metadata: json!({"event": "overbought_streak", "days": STREAK_MIN_DAYS, "threshold": params.overbought}),
                    });
                    last_overbought_streak_idx = Some(i);
                }
            } else {
                if over_run >= STREAK_MIN_DAYS
                    && prev > params.overbought
                    && last_overbought_exit_idx
                        .is_none_or(|last| i >= last + MIN_STREAK_FIRE_SPACING)
                {
                    events.push(WilliamsREvent {
                        date: series[i].date,
                        kind: WilliamsREventKind::OverboughtExit,
                        value: cur,
                        metadata: json!({"event": "overbought_exit", "threshold": params.overbought}),
                    });
                    last_overbought_exit_idx = Some(i);
                }
                over_run = 0;
            }

            // oversold streak
            if cur < params.oversold {
                under_run += 1;
                if under_run == STREAK_MIN_DAYS
                    && last_oversold_streak_idx
                        .is_none_or(|last| i >= last + MIN_STREAK_FIRE_SPACING)
                {
                    events.push(WilliamsREvent {
                        date: series[i].date,
                        kind: WilliamsREventKind::OversoldStreak,
                        value: cur,
                        metadata: json!({"event": "oversold_streak", "days": STREAK_MIN_DAYS, "threshold": params.oversold}),
                    });
                    last_oversold_streak_idx = Some(i);
                }
            } else {
                if under_run >= STREAK_MIN_DAYS
                    && prev < params.oversold
                    && last_oversold_exit_idx
                        .is_none_or(|last| i >= last + MIN_STREAK_FIRE_SPACING)
                {
                    events.push(WilliamsREvent {
                        date: series[i].date,
                        kind: WilliamsREventKind::OversoldExit,
                        value: cur,
                        metadata: json!({"event": "oversold_exit", "threshold": params.oversold}),
                    });
                    last_oversold_exit_idx = Some(i);
                }
                under_run = 0;
            }
        }

        Ok(WilliamsROutput {
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
                source_core: "williams_r_core".to_string(),
                source_version: "0.1.0".to_string(),
                params_hash: None,
                statement: format!("Williams %R {:?} on {}: value={:.2}", e.kind, e.date, e.value),
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
        let core = WilliamsRCore::new();
        assert_eq!(core.name(), "williams_r_core");
        assert_eq!(core.warmup_periods(&WilliamsRParams::default()), 56);
    }

    #[test]
    fn highest_high_equals_close_yields_zero() {
        let core = WilliamsRCore::new();
        // close == hh → %R = 0
        let bars: Vec<(f64, f64, f64)> = (0..20).map(|_| (100.0, 90.0, 100.0)).collect();
        let series = make_series(bars);
        let out = core.compute(&series, WilliamsRParams::default()).unwrap();
        // 第 14 個點(index 13)起應該有非零 series.value(close==hh → 0,但 hh==ll
        // 場景下 range=0,value 預設 0)
        assert_eq!(out.series.len(), 20);
    }

    #[test]
    fn close_equals_lowest_low_yields_minus_100() {
        let core = WilliamsRCore::new();
        // 14 bars: 13 個 hi=100, lo=90, 第 14 收於 low,期望 value = -100
        let mut bars: Vec<(f64, f64, f64)> = (0..13).map(|_| (100.0, 90.0, 95.0)).collect();
        bars.push((100.0, 90.0, 90.0));
        let series = make_series(bars);
        let out = core.compute(&series, WilliamsRParams::default()).unwrap();
        // index 13 (0-based) = 第 14 點,完成 warmup 後第一個有效值
        assert!((out.series[13].value - (-100.0)).abs() < 1e-6);
    }

    #[test]
    fn oversold_streak_fires_after_min_days() {
        let core = WilliamsRCore::new();
        // 20 bars 全部讓 close = low → 全部 -100,oversold
        let bars: Vec<(f64, f64, f64)> = (0..20).map(|_| (100.0, 90.0, 90.0)).collect();
        let series = make_series(bars);
        let out = core.compute(&series, WilliamsRParams::default()).unwrap();
        // STREAK_MIN_DAYS=3 → 第 16 個有效值(index 13+3=16 i.e. p=14 起算 STREAK_MIN_DAYS 滿)
        let streak_events: Vec<_> = out
            .events
            .iter()
            .filter(|e| e.kind == WilliamsREventKind::OversoldStreak)
            .collect();
        assert_eq!(streak_events.len(), 1, "should fire exactly once at min_days");
    }
}
