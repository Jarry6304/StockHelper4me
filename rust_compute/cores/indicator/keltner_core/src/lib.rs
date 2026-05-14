// keltner_core(P3)— 對齊 m3Spec/indicator_cores_volatility.md §五
// Params §5.2 / warmup §5.3 / Output §5.4 / Fact §5.5
//
// Reference:
//   Chester W. Keltner (1960), "How to Make Money in Commodities"
//   Linda Bradford Raschke (現代版):用 EMA 取代 SMA,ATR 取代 high-low range
//   atr_multiplier=2.0 / ema_period=20 / atr_period=10:Raschke 慣例
//
// Keltner Channel:
//   middle = EMA(close, ema_period)
//   upper  = middle + atr_multiplier × ATR(atr_period)
//   lower  = middle - atr_multiplier × ATR(atr_period)
//
// 內部 inline wilder_atr + ema(對齊 v1.28 indicator kernel revert,零耦合)

use anyhow::Result;
use chrono::NaiveDate;
use fact_schema::{Fact, IndicatorCore, Timeframe};
use ohlcv_loader::{OhlcvBar, OhlcvSeries};
use serde::Serialize;
use serde_json::json;

inventory::submit! {
    core_registry::CoreRegistration::new(
        "keltner_core", "0.1.0", core_registry::CoreKind::Indicator, "P3",
        "Keltner Channel Core(EMA ±k×ATR 通道)",
    )
}

const ABOVE_MIDDLE_STREAK_MIN: usize = 30;

#[derive(Debug, Clone, Serialize)]
pub struct KeltnerParams {
    pub ema_period: usize,
    pub atr_period: usize,
    pub atr_multiplier: f64,
    pub timeframe: Timeframe,
}
impl Default for KeltnerParams {
    fn default() -> Self {
        Self {
            ema_period: 20,
            atr_period: 10,
            atr_multiplier: 2.0,
            timeframe: Timeframe::Daily,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct KeltnerOutput {
    pub stock_id: String,
    pub timeframe: Timeframe,
    pub series: Vec<KeltnerPoint>,
    #[serde(skip)]
    pub events: Vec<KeltnerEvent>,
}
#[derive(Debug, Clone, Serialize)]
pub struct KeltnerPoint {
    pub date: NaiveDate,
    pub upper_band: f64,
    pub middle_band: f64,
    pub lower_band: f64,
}
#[derive(Debug, Clone, Serialize)]
pub struct KeltnerEvent {
    pub date: NaiveDate,
    pub kind: KeltnerEventKind,
    pub metadata: serde_json::Value,
}
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum KeltnerEventKind {
    UpperBreakout,
    LowerBreakout,
    AboveMiddleStreak,
}

pub struct KeltnerCore;
impl KeltnerCore {
    pub fn new() -> Self {
        KeltnerCore
    }
}
impl Default for KeltnerCore {
    fn default() -> Self {
        KeltnerCore::new()
    }
}

fn ema(values: &[f64], period: usize) -> Vec<f64> {
    let n = values.len();
    let mut out = vec![0.0; n];
    if n == 0 || period == 0 {
        return out;
    }
    let alpha = 2.0 / (period as f64 + 1.0);
    out[0] = values[0];
    for i in 1..n {
        out[i] = alpha * values[i] + (1.0 - alpha) * out[i - 1];
    }
    out
}

fn wilder_atr(bars: &[OhlcvBar], period: usize) -> Vec<f64> {
    let n = bars.len();
    if n == 0 || period == 0 {
        return vec![0.0; n];
    }
    let mut tr = Vec::with_capacity(n);
    tr.push(bars[0].high - bars[0].low);
    for i in 1..n {
        let prev = bars[i - 1].close;
        let h = bars[i].high;
        let l = bars[i].low;
        let cands = [(h - l).abs(), (h - prev).abs(), (l - prev).abs()];
        tr.push(cands.iter().cloned().fold(0.0_f64, f64::max));
    }
    let mut atr = vec![0.0; n];
    let warmup = period.min(n);
    let mut sum = 0.0;
    for i in 0..warmup {
        sum += tr[i];
        atr[i] = sum / (i + 1) as f64;
    }
    for i in warmup..n {
        atr[i] = ((period as f64 - 1.0) * atr[i - 1] + tr[i]) / period as f64;
    }
    atr
}

impl IndicatorCore for KeltnerCore {
    type Input = OhlcvSeries;
    type Params = KeltnerParams;
    type Output = KeltnerOutput;
    fn name(&self) -> &'static str {
        "keltner_core"
    }
    fn version(&self) -> &'static str {
        "0.1.0"
    }
    fn warmup_periods(&self, params: &Self::Params) -> usize {
        (params.ema_period * 4).max(params.atr_period * 4) + 5
    }

    fn compute(&self, input: &Self::Input, params: Self::Params) -> Result<Self::Output> {
        let n = input.bars.len();
        let closes: Vec<f64> = input.bars.iter().map(|b| b.close).collect();
        let ema_close = ema(&closes, params.ema_period);
        let atrs = wilder_atr(&input.bars, params.atr_period);

        let mut series = Vec::with_capacity(n);
        for i in 0..n {
            let middle = ema_close[i];
            let band = params.atr_multiplier * atrs[i];
            series.push(KeltnerPoint {
                date: input.bars[i].date,
                upper_band: middle + band,
                middle_band: middle,
                lower_band: middle - band,
            });
        }

        let warmup = self.warmup_periods(&params).min(n);
        let mut events = Vec::new();
        let mut above_run = 0usize;
        for i in warmup..n {
            let close = input.bars[i].close;
            let prev_close = input.bars[i - 1].close;

            // upper / lower breakout — edge trigger
            if close > series[i].upper_band && prev_close <= series[i - 1].upper_band {
                events.push(KeltnerEvent {
                    date: series[i].date,
                    kind: KeltnerEventKind::UpperBreakout,
                    metadata: json!({"event": "keltner_upper_breakout", "close": close, "upper": series[i].upper_band}),
                });
            } else if close < series[i].lower_band && prev_close >= series[i - 1].lower_band {
                events.push(KeltnerEvent {
                    date: series[i].date,
                    kind: KeltnerEventKind::LowerBreakout,
                    metadata: json!({"event": "keltner_lower_breakout", "close": close, "lower": series[i].lower_band}),
                });
            }

            // above middle streak
            if close > series[i].middle_band {
                above_run += 1;
                if above_run == ABOVE_MIDDLE_STREAK_MIN {
                    events.push(KeltnerEvent {
                        date: series[i].date,
                        kind: KeltnerEventKind::AboveMiddleStreak,
                        metadata: json!({"event": "above_middle_streak", "days": ABOVE_MIDDLE_STREAK_MIN}),
                    });
                }
            } else {
                above_run = 0;
            }
        }

        Ok(KeltnerOutput {
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
                source_core: "keltner_core".to_string(),
                source_version: "0.1.0".to_string(),
                params_hash: None,
                statement: format!("Keltner {:?} on {}", e.kind, e.date),
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
        let core = KeltnerCore::new();
        assert_eq!(core.name(), "keltner_core");
        // ema_period=20, atr_period=10 → max(80, 40) + 5 = 85
        assert_eq!(core.warmup_periods(&KeltnerParams::default()), 85);
    }

    #[test]
    fn flat_input_yields_band_equals_middle() {
        let core = KeltnerCore::new();
        let series = make_series((0..100).map(|_| (100.0, 100.0, 100.0)).collect());
        let out = core.compute(&series, KeltnerParams::default()).unwrap();
        // flat input → ATR=0 → upper == middle == lower
        for p in &out.series[20..] {
            assert!((p.upper_band - p.middle_band).abs() < 1e-6);
            assert!((p.lower_band - p.middle_band).abs() < 1e-6);
        }
    }
}
