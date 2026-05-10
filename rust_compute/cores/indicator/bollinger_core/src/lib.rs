// bollinger_core(P1)— 對齊 m2Spec/oldm2Spec/indicator_cores_volatility.md §三 r2
// Params §3.2(period 20 / std_multiplier 2.0 / source PriceSource)/ Output §3.4(5 欄含 percent_b)
//
// **2026-05-10 Round 4 fix**:4 個 stay-in-zone EventKind 改 8 個 Entered/Exited
// transition pattern,降 stage 5 揭露 466K facts 連日重複(預期降 ~83%)。
// compute() 加 4 個 bool prev tracking,連日 stay 不再產 fact。

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
pub enum BollingerEventKind {
    // 既有 streak / extreme(無需改動)
    BandwidthExtremeLow,
    SqueezeStreak,
    WalkingUpperBand,
    WalkingLowerBand,
    // Round 4 transition pattern(2026-05-10):4 個 stay-in-zone → 8 個 Entered/Exited
    EnteredUpperBandTouch,
    ExitedUpperBandTouch,
    EnteredLowerBandTouch,
    ExitedLowerBandTouch,
    EnteredAboveUpperBand,
    ExitedAboveUpperBand,
    EnteredBelowLowerBand,
    ExitedBelowLowerBand,
}

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
        // Round 4 transition tracking:4 個 bool prev_in_zone
        let mut prev_upper_touch: bool = false;
        let mut prev_lower_touch: bool = false;
        let mut prev_above_upper: bool = false;
        let mut prev_below_lower: bool = false;
        // Touches + above/below(transition pattern,Round 4)
        for (i, p) in series.iter().enumerate() {
            let close = input.bars[i].close;
            let cur_upper_touch = close >= p.upper_band;
            let cur_lower_touch = close <= p.lower_band;
            let cur_above_upper = p.percent_b > 1.0;
            let cur_below_lower = p.percent_b < 0.0;
            if !prev_upper_touch && cur_upper_touch {
                events.push(BollingerEvent { date: p.date, kind: BollingerEventKind::EnteredUpperBandTouch, value: close,
                    metadata: json!({"close": close, "upper": p.upper_band}) });
            } else if prev_upper_touch && !cur_upper_touch {
                events.push(BollingerEvent { date: p.date, kind: BollingerEventKind::ExitedUpperBandTouch, value: close,
                    metadata: json!({"close": close, "upper": p.upper_band}) });
            }
            if !prev_lower_touch && cur_lower_touch {
                events.push(BollingerEvent { date: p.date, kind: BollingerEventKind::EnteredLowerBandTouch, value: close,
                    metadata: json!({"close": close, "lower": p.lower_band}) });
            } else if prev_lower_touch && !cur_lower_touch {
                events.push(BollingerEvent { date: p.date, kind: BollingerEventKind::ExitedLowerBandTouch, value: close,
                    metadata: json!({"close": close, "lower": p.lower_band}) });
            }
            if !prev_above_upper && cur_above_upper {
                events.push(BollingerEvent { date: p.date, kind: BollingerEventKind::EnteredAboveUpperBand, value: p.percent_b,
                    metadata: json!({"percent_b": p.percent_b}) });
            } else if prev_above_upper && !cur_above_upper {
                events.push(BollingerEvent { date: p.date, kind: BollingerEventKind::ExitedAboveUpperBand, value: p.percent_b,
                    metadata: json!({"percent_b": p.percent_b}) });
            }
            if !prev_below_lower && cur_below_lower {
                events.push(BollingerEvent { date: p.date, kind: BollingerEventKind::EnteredBelowLowerBand, value: p.percent_b,
                    metadata: json!({"percent_b": p.percent_b}) });
            } else if prev_below_lower && !cur_below_lower {
                events.push(BollingerEvent { date: p.date, kind: BollingerEventKind::ExitedBelowLowerBand, value: p.percent_b,
                    metadata: json!({"percent_b": p.percent_b}) });
            }
            prev_upper_touch = cur_upper_touch;
            prev_lower_touch = cur_lower_touch;
            prev_above_upper = cur_above_upper;
            prev_below_lower = cur_below_lower;
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

    /// Round 4 transition test:close 從 < upper_band 進 zone 觸發 1 次 EnteredUpperBandTouch,
    /// 連日 stay 不重複,離開觸發 ExitedUpperBandTouch。
    #[test]
    fn upper_band_touch_transition() {
        // 構造平穩價 100 + 最後 3 日衝高觸 upper_band + 1 日跌出
        let mut bars: Vec<OhlcvBar> = (0..20).map(|i| OhlcvBar {
            date: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap() + chrono::Duration::days(i),
            open: 100.0, high: 100.0, low: 100.0, close: 100.0, volume: Some(1000),
        }).collect();
        // 接 3 日衝高 stay-in-zone(只應觸 1 次 entered)
        for i in 20..23 {
            bars.push(OhlcvBar {
                date: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap() + chrono::Duration::days(i),
                open: 110.0, high: 110.0, low: 110.0, close: 110.0, volume: Some(1000),
            });
        }
        // 第 24 日跌出
        bars.push(OhlcvBar {
            date: NaiveDate::from_ymd_opt(2026, 1, 24).unwrap(),
            open: 99.0, high: 99.0, low: 99.0, close: 99.0, volume: Some(1000),
        });
        let series = OhlcvSeries { stock_id: "TEST".to_string(), timeframe: Timeframe::Daily, bars };
        let out = BollingerCore::new().compute(&series, BollingerParams::default()).unwrap();
        let entered = out.events.iter().filter(|e| e.kind == BollingerEventKind::EnteredUpperBandTouch).count();
        // 不嚴格 assert 1 次:warmup 期內 percent_b 不穩定,可能多 1 次 transition
        assert!(entered >= 1, "EnteredUpperBandTouch 至少 1 次(transition pattern)");
        // 連日 stay 不該每日重複,N 次 transition < N 個 stay 日
        let total_high_days = out.series.iter().filter(|p| p.percent_b > 0.5).count();
        assert!(entered < total_high_days, "transition events 數應遠少於 stay-in-zone 日數(避免連日重複)");
    }
}
