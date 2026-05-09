// bollinger_core(P1)— 對齊 m2Spec/oldm2Spec/indicator_cores_volatility.md §三 r2
// Params §3.2(period 20 / std_multiplier 2.0 / source PriceSource)/ Output §3.4(5 欄含 percent_b)

use anyhow::Result;
use chrono::NaiveDate;
use fact_schema::{Fact, IndicatorCore, Timeframe};
use ohlcv_loader::{OhlcvBar, OhlcvSeries};
use serde::Serialize;
use serde_json::json;

inventory::submit! {
    core_registry::CoreRegistration::new(
        "bollinger_core", "0.1.0", core_registry::CoreKind::Indicator, "P1",
        "Bollinger Core(SMA ± std_multiplier × stdev + percent_b)",
    )
}

const SQUEEZE_STREAK_MIN: usize = 5;
const WALK_BAND_NEAR_THRESHOLD: f64 = 0.95; // %B >= 0.95 視為 walking upper

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum PriceSource { Close, Open, High, Low, Hl2, Hlc3, Ohlc4 }

#[derive(Debug, Clone, Serialize)]
pub struct BollingerParams {
    pub period: usize,
    pub std_multiplier: f64,
    pub source: PriceSource,
    pub timeframe: Timeframe,
}
impl Default for BollingerParams {
    fn default() -> Self { Self { period: 20, std_multiplier: 2.0, source: PriceSource::Close, timeframe: Timeframe::Daily } }
}

#[derive(Debug, Clone, Serialize)]
pub struct BollingerOutput {
    pub stock_id: String, pub timeframe: Timeframe,
    pub series: Vec<BollingerPoint>,
    #[serde(skip)]
    pub events: Vec<BollingerEvent>,
}
#[derive(Debug, Clone, Serialize)]
pub struct BollingerPoint {
    pub date: NaiveDate,
    pub upper_band: f64,
    pub middle_band: f64,
    pub lower_band: f64,
    pub bandwidth: f64,
    pub percent_b: f64,
}
#[derive(Debug, Clone, Serialize)]
pub struct BollingerEvent { pub date: NaiveDate, pub kind: BollingerEventKind, pub value: f64, pub metadata: serde_json::Value }
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum BollingerEventKind { BandwidthExtremeLow, UpperBandTouch, LowerBandTouch, AboveUpperBand, BelowLowerBand, SqueezeStreak, WalkingUpperBand, WalkingLowerBand }

pub struct BollingerCore;
impl BollingerCore { pub fn new() -> Self { BollingerCore } }
impl Default for BollingerCore { fn default() -> Self { BollingerCore::new() } }

fn pick_source(bars: &[OhlcvBar], src: PriceSource) -> Vec<f64> {
    bars.iter().map(|b| match src {
        PriceSource::Close => b.close, PriceSource::Open => b.open,
        PriceSource::High => b.high, PriceSource::Low => b.low,
        PriceSource::Hl2 => (b.high + b.low) / 2.0,
        PriceSource::Hlc3 => (b.high + b.low + b.close) / 3.0,
        PriceSource::Ohlc4 => (b.open + b.high + b.low + b.close) / 4.0,
    }).collect()
}

impl IndicatorCore for BollingerCore {
    type Input = OhlcvSeries;
    type Params = BollingerParams;
    type Output = BollingerOutput;
    fn name(&self) -> &'static str { "bollinger_core" }
    fn version(&self) -> &'static str { "0.1.0" }
    /// §3.3:`period + 5`
    fn warmup_periods(&self, params: &Self::Params) -> usize { params.period + 5 }

    fn compute(&self, input: &Self::Input, params: Self::Params) -> Result<Self::Output> {
        let n = input.bars.len();
        let src_values = pick_source(&input.bars, params.source);
        let mut series = Vec::with_capacity(n);
        for i in 0..n {
            let p = params.period.min(i + 1);
            let start = i + 1 - p;
            let win = &src_values[start..=i];
            let mean: f64 = win.iter().sum::<f64>() / p as f64;
            let var: f64 = win.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / p as f64;
            let std = var.sqrt();
            let upper = mean + params.std_multiplier * std;
            let lower = mean - params.std_multiplier * std;
            let bandwidth = if mean > 0.0 { (upper - lower) / mean } else { 0.0 };
            let close = input.bars[i].close;
            let percent_b = if upper - lower > 1e-12 { (close - lower) / (upper - lower) } else { 0.5 };
            series.push(BollingerPoint { date: input.bars[i].date, upper_band: upper, middle_band: mean, lower_band: lower, bandwidth, percent_b });
        }
        let mut events = Vec::new();
        // Touches + above/below
        for (i, p) in series.iter().enumerate() {
            let close = input.bars[i].close;
            if close >= p.upper_band {
                events.push(BollingerEvent { date: p.date, kind: BollingerEventKind::UpperBandTouch, value: close,
                    metadata: json!({"event": "upper_band_touch", "close": close, "upper": p.upper_band}) });
            }
            if close <= p.lower_band {
                events.push(BollingerEvent { date: p.date, kind: BollingerEventKind::LowerBandTouch, value: close,
                    metadata: json!({"event": "lower_band_touch", "close": close, "lower": p.lower_band}) });
            }
            if p.percent_b > 1.0 {
                events.push(BollingerEvent { date: p.date, kind: BollingerEventKind::AboveUpperBand, value: p.percent_b,
                    metadata: json!({"event": "above_upper_band", "percent_b": p.percent_b}) });
            } else if p.percent_b < 0.0 {
                events.push(BollingerEvent { date: p.date, kind: BollingerEventKind::BelowLowerBand, value: p.percent_b,
                    metadata: json!({"event": "below_lower_band", "percent_b": p.percent_b}) });
            }
        }
        // Bandwidth extreme low(1y lookback ≈ 252)
        const LB: usize = 252;
        if series.len() > LB {
            for i in LB..series.len() {
                let win = &series[i - LB..i];
                let min_bw = win.iter().map(|p| p.bandwidth).fold(f64::INFINITY, f64::min);
                if series[i].bandwidth < min_bw {
                    events.push(BollingerEvent { date: series[i].date, kind: BollingerEventKind::BandwidthExtremeLow, value: series[i].bandwidth,
                        metadata: json!({"event": "bandwidth_extreme_low", "value": series[i].bandwidth, "lookback": "1y"}) });
                }
            }
        }
        // Squeeze streak(bandwidth < 0.10 連續 N 天)
        let mut sq_count = 0;
        for p in &series {
            if p.bandwidth < 0.10 { sq_count += 1; }
            else { sq_count = 0; }
            if sq_count == SQUEEZE_STREAK_MIN {
                events.push(BollingerEvent { date: p.date, kind: BollingerEventKind::SqueezeStreak, value: sq_count as f64,
                    metadata: json!({"event": "squeeze_streak", "days": sq_count}) });
            }
        }
        // Walking the band
        let mut up_walk = 0; let mut dn_walk = 0;
        for p in &series {
            if p.percent_b >= WALK_BAND_NEAR_THRESHOLD { up_walk += 1; } else { up_walk = 0; }
            if p.percent_b <= 1.0 - WALK_BAND_NEAR_THRESHOLD { dn_walk += 1; } else { dn_walk = 0; }
            if up_walk == 5 {
                events.push(BollingerEvent { date: p.date, kind: BollingerEventKind::WalkingUpperBand, value: up_walk as f64,
                    metadata: json!({"event": "walking_upper_band", "days": up_walk}) });
            }
            if dn_walk == 5 {
                events.push(BollingerEvent { date: p.date, kind: BollingerEventKind::WalkingLowerBand, value: dn_walk as f64,
                    metadata: json!({"event": "walking_lower_band", "days": dn_walk}) });
            }
        }
        Ok(BollingerOutput { stock_id: input.stock_id.clone(), timeframe: params.timeframe, series, events })
    }

    fn produce_facts(&self, output: &Self::Output) -> Vec<Fact> {
        output.events.iter().map(|e| Fact {
            stock_id: output.stock_id.clone(), fact_date: e.date, timeframe: output.timeframe,
            source_core: "bollinger_core".to_string(), source_version: "0.1.0".to_string(),
            params_hash: None, statement: format!("Bollinger {:?} on {}: value={:.4}", e.kind, e.date, e.value),
            metadata: e.metadata.clone(),
        }).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn name_warmup() {
        let core = BollingerCore::new();
        assert_eq!(core.name(), "bollinger_core");
        assert_eq!(core.warmup_periods(&BollingerParams::default()), 25);
    }
}
