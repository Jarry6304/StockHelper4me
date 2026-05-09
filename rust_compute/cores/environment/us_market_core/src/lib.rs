// us_market_core(P2)— Environment Core
// 對齊 m2Spec/oldm2Spec/environment_cores.md §四 r2
// Params §4.4 / Output §4.6(spy/vix 同點 + VixZone)/ stock_id 保留字 _global_

use anyhow::Result;
use chrono::NaiveDate;
use environment_loader::UsMarketCombinedSeries;
use fact_schema::{Fact, IndicatorCore, Timeframe};
use serde::Serialize;
use serde_json::json;
use std::collections::BTreeMap;

inventory::submit! {
    core_registry::CoreRegistration::new(
        "us_market_core", "0.1.0", core_registry::CoreKind::Environment, "P2",
        "US Market Core(SPY MACD + VIX zone + 夜盤異動)",
    )
}

const RESERVED_STOCK_ID: &str = "_global_";

#[derive(Debug, Clone, Serialize)]
pub struct UsMarketParams {
    pub spy_macd_fast: usize,
    pub spy_macd_slow: usize,
    pub spy_macd_signal: usize,
    pub vix_high_threshold: f64,
    pub vix_low_threshold: f64,
    pub overnight_change_threshold: f64,
}
impl Default for UsMarketParams {
    fn default() -> Self { Self { spy_macd_fast: 12, spy_macd_slow: 26, spy_macd_signal: 9, vix_high_threshold: 25.0, vix_low_threshold: 15.0, overnight_change_threshold: 1.5 } }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum VixZone { Low, Normal, High, ExtremeHigh }

#[derive(Debug, Clone, Serialize)]
pub struct UsMarketOutput {
    pub stock_id: String, pub timeframe: Timeframe,
    pub series: Vec<UsMarketPoint>,
    pub events: Vec<UsMarketEvent>,
}
#[derive(Debug, Clone, Serialize)]
pub struct UsMarketPoint {
    pub date: NaiveDate,
    pub spy_close: f64,
    pub spy_change_pct: f64,
    pub spy_macd_histogram: f64,
    pub vix_close: f64,
    pub vix_change_pct: f64,
    pub vix_zone: VixZone,
}
#[derive(Debug, Clone, Serialize)]
pub struct UsMarketEvent { pub date: NaiveDate, pub kind: UsMarketEventKind, pub value: f64, pub metadata: serde_json::Value }
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum UsMarketEventKind {
    SpyMacdGoldenCross, SpyMacdDeathCross, VixSpike, VixHighZoneEntry, VixLowZoneEntry, SpyOvernightLargeMove,
}

pub struct UsMarketCore;
impl UsMarketCore { pub fn new() -> Self { UsMarketCore } }
impl Default for UsMarketCore { fn default() -> Self { UsMarketCore::new() } }

fn ema(v: &[f64], p: usize) -> Vec<f64> {
    let mut out = vec![0.0; v.len()];
    if v.is_empty() || p == 0 { return out; }
    let a = 2.0 / (p as f64 + 1.0);
    out[0] = v[0];
    for i in 1..v.len() { out[i] = a * v[i] + (1.0 - a) * out[i - 1]; }
    out
}

fn classify_vix_zone(vix: f64, low: f64, high: f64) -> VixZone {
    if vix <= low { VixZone::Low }
    else if vix < high { VixZone::Normal }
    else if vix < high * 1.5 { VixZone::High }
    else { VixZone::ExtremeHigh }
}

impl IndicatorCore for UsMarketCore {
    type Input = UsMarketCombinedSeries;
    type Params = UsMarketParams;
    type Output = UsMarketOutput;
    fn name(&self) -> &'static str { "us_market_core" }
    fn version(&self) -> &'static str { "0.1.0" }
    /// §4.5:`spy_macd_slow * 4`
    fn warmup_periods(&self, params: &Self::Params) -> usize { params.spy_macd_slow * 4 }

    fn compute(&self, input: &Self::Input, params: Self::Params) -> Result<Self::Output> {
        // Build date → (spy_close, vix_close) lookup
        let mut by_date: BTreeMap<NaiveDate, (Option<f64>, Option<f64>)> = BTreeMap::new();
        for p in &input.spy.points { by_date.entry(p.date).or_insert((None, None)).0 = p.close; }
        for p in &input.vix.points { by_date.entry(p.date).or_insert((None, None)).1 = p.close; }
        let dates: Vec<NaiveDate> = by_date.keys().copied().collect();
        let spy_closes: Vec<f64> = dates.iter().map(|d| by_date[d].0.unwrap_or(0.0)).collect();
        let vix_closes: Vec<f64> = dates.iter().map(|d| by_date[d].1.unwrap_or(0.0)).collect();
        // SPY MACD
        let ema_fast = ema(&spy_closes, params.spy_macd_fast);
        let ema_slow = ema(&spy_closes, params.spy_macd_slow);
        let macd_line: Vec<f64> = (0..dates.len()).map(|i| ema_fast[i] - ema_slow[i]).collect();
        let macd_signal = ema(&macd_line, params.spy_macd_signal);

        let mut series = Vec::with_capacity(dates.len());
        let mut prev_spy: Option<f64> = None;
        let mut prev_vix: Option<f64> = None;
        for i in 0..dates.len() {
            let sc = spy_closes[i]; let vc = vix_closes[i];
            let spy_change = match prev_spy { Some(p) if p > 0.0 => (sc - p) / p * 100.0, _ => 0.0 };
            let vix_change = match prev_vix { Some(p) if p > 0.0 => (vc - p) / p * 100.0, _ => 0.0 };
            series.push(UsMarketPoint {
                date: dates[i], spy_close: sc, spy_change_pct: spy_change,
                spy_macd_histogram: macd_line[i] - macd_signal[i],
                vix_close: vc, vix_change_pct: vix_change,
                vix_zone: classify_vix_zone(vc, params.vix_low_threshold, params.vix_high_threshold),
            });
            prev_spy = Some(sc); prev_vix = Some(vc);
        }
        let mut events = Vec::new();
        for i in 1..series.len() {
            let prev = &series[i - 1]; let cur = &series[i];
            // SPY MACD cross
            let pa = macd_line[i - 1] > macd_signal[i - 1];
            let ca = macd_line[i] > macd_signal[i];
            if !pa && ca {
                events.push(UsMarketEvent { date: cur.date, kind: UsMarketEventKind::SpyMacdGoldenCross, value: macd_line[i],
                    metadata: json!({"index": "spy"}) });
            } else if pa && !ca {
                events.push(UsMarketEvent { date: cur.date, kind: UsMarketEventKind::SpyMacdDeathCross, value: macd_line[i],
                    metadata: json!({"index": "spy"}) });
            }
            // VIX spike(單日 +20% 以上)
            if cur.vix_change_pct >= 20.0 {
                events.push(UsMarketEvent { date: cur.date, kind: UsMarketEventKind::VixSpike, value: cur.vix_close,
                    metadata: json!({"vix": cur.vix_close, "change": cur.vix_change_pct}) });
            }
            // VIX zone entry
            if prev.vix_close < params.vix_high_threshold && cur.vix_close >= params.vix_high_threshold {
                events.push(UsMarketEvent { date: cur.date, kind: UsMarketEventKind::VixHighZoneEntry, value: cur.vix_close,
                    metadata: json!({"vix": cur.vix_close, "threshold": params.vix_high_threshold}) });
            }
            if prev.vix_close > params.vix_low_threshold && cur.vix_close <= params.vix_low_threshold {
                events.push(UsMarketEvent { date: cur.date, kind: UsMarketEventKind::VixLowZoneEntry, value: cur.vix_close,
                    metadata: json!({"vix": cur.vix_close, "threshold": params.vix_low_threshold}) });
            }
            // SPY overnight large move
            if cur.spy_change_pct.abs() >= params.overnight_change_threshold {
                events.push(UsMarketEvent { date: cur.date, kind: UsMarketEventKind::SpyOvernightLargeMove, value: cur.spy_change_pct,
                    metadata: json!({"change": cur.spy_change_pct, "us_date": cur.date}) });
            }
        }
        Ok(UsMarketOutput { stock_id: RESERVED_STOCK_ID.to_string(), timeframe: Timeframe::Daily, series, events })
    }

    fn produce_facts(&self, output: &Self::Output) -> Vec<Fact> {
        output.events.iter().map(|e| Fact {
            stock_id: output.stock_id.clone(), fact_date: e.date, timeframe: output.timeframe,
            source_core: "us_market_core".to_string(), source_version: "0.1.0".to_string(),
            params_hash: None, statement: format!("US {:?} on {}: value={:.2}", e.kind, e.date, e.value),
            metadata: e.metadata.clone(),
        }).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use environment_loader::MarketIndexUsSeries;
    #[test]
    fn name_warmup_reserved_id() {
        let core = UsMarketCore::new();
        assert_eq!(core.name(), "us_market_core");
        assert_eq!(core.warmup_periods(&UsMarketParams::default()), 26 * 4);
        let input = UsMarketCombinedSeries {
            spy: MarketIndexUsSeries { stock_id: "SPY".to_string(), points: vec![] },
            vix: MarketIndexUsSeries { stock_id: "^VIX".to_string(), points: vec![] },
        };
        let out = core.compute(&input, UsMarketParams::default()).unwrap();
        assert_eq!(out.stock_id, "_global_");
    }
}
