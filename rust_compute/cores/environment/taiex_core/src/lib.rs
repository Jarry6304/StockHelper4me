// taiex_core(P2)— Environment Core
// 對齊 m2Spec/oldm2Spec/environment_cores.md §三 r2
// Params §3.4(MACD/RSI/volume_z/trend_lookback)/ Output §3.6(MACD/RSI/Volume Z/TrendState)
// EventKind 9 個(§3.6)/ stock_id 保留字 _index_taiex_(§6.2.1)

use anyhow::Result;
use chrono::NaiveDate;
use environment_loader::MarketIndexTwSeries;
use fact_schema::{Fact, IndicatorCore, Timeframe};
use serde::Serialize;
use serde_json::json;

inventory::submit! {
    core_registry::CoreRegistration::new(
        "taiex_core", "0.1.0", core_registry::CoreKind::Environment, "P2",
        "TAIEX Core(加權指數 — MACD/RSI/Volume Z + MA60 trend)",
    )
}

const RESERVED_STOCK_ID: &str = "_index_taiex_";

#[derive(Debug, Clone, Serialize)]
pub struct TaiexParams {
    pub timeframe: Timeframe,
    pub macd_fast: usize,
    pub macd_slow: usize,
    pub macd_signal: usize,
    pub rsi_period: usize,
    pub volume_z_threshold: f64,
    pub trend_lookback_bars: usize,
}
impl Default for TaiexParams {
    fn default() -> Self { Self { timeframe: Timeframe::Daily, macd_fast: 12, macd_slow: 26, macd_signal: 9, rsi_period: 14, volume_z_threshold: 2.0, trend_lookback_bars: 60 } }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum TrendState { BullishMa, BearishMa, Neutral }

#[derive(Debug, Clone, Serialize)]
pub struct TaiexOutput {
    pub stock_id: String, pub timeframe: Timeframe,
    pub series: Vec<TaiexPoint>,
    pub events: Vec<TaiexEvent>,
}
#[derive(Debug, Clone, Serialize)]
pub struct TaiexPoint {
    pub date: NaiveDate,
    pub close: f64,
    pub volume: i64,
    pub change_pct: f64,
    pub macd_line: f64,
    pub macd_signal: f64,
    pub macd_histogram: f64,
    pub rsi: f64,
    pub volume_z: f64,
    pub trend_state: TrendState,
}
#[derive(Debug, Clone, Serialize)]
pub struct TaiexEvent { pub date: NaiveDate, pub kind: TaiexEventKind, pub value: f64, pub metadata: serde_json::Value }
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum TaiexEventKind {
    MacdGoldenCross, MacdDeathCross, RsiOverbought, RsiOversold,
    VolumeSurge, NewHigh20d, NewLow20d, BreakdownBelowMa60, BreakoutAboveMa60,
}

pub struct TaiexCore;
impl TaiexCore { pub fn new() -> Self { TaiexCore } }
impl Default for TaiexCore { fn default() -> Self { TaiexCore::new() } }

// 內嵌 indicator math(§3.3:不從外部 Indicator Core 取資料,維持零耦合)
fn ema(v: &[f64], p: usize) -> Vec<f64> {
    let mut out = vec![0.0; v.len()];
    if v.is_empty() || p == 0 { return out; }
    let a = 2.0 / (p as f64 + 1.0);
    out[0] = v[0];
    for i in 1..v.len() { out[i] = a * v[i] + (1.0 - a) * out[i - 1]; }
    out
}
fn sma(v: &[f64], p: usize) -> Vec<f64> {
    let mut out = vec![0.0; v.len()];
    if v.is_empty() || p == 0 { return out; }
    let mut s = 0.0;
    for i in 0..v.len() { s += v[i]; if i >= p { s -= v[i - p]; } out[i] = s / (i + 1).min(p) as f64; }
    out
}
fn wilder_rsi(closes: &[f64], period: usize) -> Vec<f64> {
    let n = closes.len();
    if n < 2 || period == 0 { return vec![0.0; n]; }
    let mut gains = vec![0.0; n]; let mut losses = vec![0.0; n];
    for i in 1..n { let d = closes[i] - closes[i - 1]; if d > 0.0 { gains[i] = d; } else { losses[i] = -d; } }
    let mut ag = vec![0.0; n]; let mut al = vec![0.0; n];
    let warmup = period.min(n - 1);
    let (mut sg, mut sl) = (0.0, 0.0);
    for i in 1..=warmup { sg += gains[i]; sl += losses[i]; }
    let p = warmup as f64;
    ag[warmup] = sg / p; al[warmup] = sl / p;
    for i in (warmup + 1)..n {
        ag[i] = ((period as f64 - 1.0) * ag[i - 1] + gains[i]) / period as f64;
        al[i] = ((period as f64 - 1.0) * al[i - 1] + losses[i]) / period as f64;
    }
    let mut rsi = vec![0.0; n];
    for i in warmup..n { rsi[i] = if al[i] < 1e-12 { 100.0 } else { let rs = ag[i] / al[i]; 100.0 - 100.0 / (1.0 + rs) }; }
    rsi
}

impl IndicatorCore for TaiexCore {
    type Input = MarketIndexTwSeries;
    type Params = TaiexParams;
    type Output = TaiexOutput;
    fn name(&self) -> &'static str { "taiex_core" }
    fn version(&self) -> &'static str { "0.1.0" }
    /// §3.5:`(macd_slow * 4).max(trend_lookback_bars + 10)`
    fn warmup_periods(&self, params: &Self::Params) -> usize {
        (params.macd_slow * 4).max(params.trend_lookback_bars + 10)
    }

    fn compute(&self, input: &Self::Input, params: Self::Params) -> Result<Self::Output> {
        let n = input.points.len();
        let closes: Vec<f64> = input.points.iter().map(|p| p.close.unwrap_or(0.0)).collect();
        let volumes: Vec<i64> = input.points.iter().map(|p| p.volume.unwrap_or(0)).collect();
        // MACD
        let ema_fast = ema(&closes, params.macd_fast);
        let ema_slow = ema(&closes, params.macd_slow);
        let macd_line: Vec<f64> = (0..n).map(|i| ema_fast[i] - ema_slow[i]).collect();
        let macd_signal = ema(&macd_line, params.macd_signal);
        // RSI
        let rsi = wilder_rsi(&closes, params.rsi_period);
        // MA60(trend)
        let ma60 = sma(&closes, params.trend_lookback_bars);
        // Volume z(60-bar window)
        let vol_z: Vec<f64> = (0..n).map(|i| {
            let lb = params.trend_lookback_bars.min(i);
            if lb < 10 { return 0.0; }
            let win: Vec<f64> = volumes[i - lb..i].iter().map(|&v| v as f64).collect();
            let mean = win.iter().sum::<f64>() / win.len() as f64;
            let var = win.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / win.len() as f64;
            let std = var.sqrt();
            if std > 0.0 { (volumes[i] as f64 - mean) / std } else { 0.0 }
        }).collect();

        let mut series = Vec::with_capacity(n);
        let mut prev_close: Option<f64> = None;
        for i in 0..n {
            let close = closes[i];
            let change_pct = match prev_close {
                Some(c) if c > 0.0 => (close - c) / c * 100.0,
                _ => 0.0,
            };
            let trend_state = if close > ma60[i] && ma60[i] > 0.0 { TrendState::BullishMa }
                else if close < ma60[i] && ma60[i] > 0.0 { TrendState::BearishMa }
                else { TrendState::Neutral };
            series.push(TaiexPoint {
                date: input.points[i].date, close, volume: volumes[i], change_pct,
                macd_line: macd_line[i], macd_signal: macd_signal[i],
                macd_histogram: macd_line[i] - macd_signal[i],
                rsi: rsi[i], volume_z: vol_z[i], trend_state,
            });
            prev_close = Some(close);
        }
        // Events
        let mut events = Vec::new();
        for i in 1..series.len() {
            let prev = &series[i - 1]; let cur = &series[i];
            // MACD cross
            let prev_macd_above = prev.macd_line > prev.macd_signal;
            let cur_macd_above = cur.macd_line > cur.macd_signal;
            if !prev_macd_above && cur_macd_above {
                events.push(TaiexEvent { date: cur.date, kind: TaiexEventKind::MacdGoldenCross, value: cur.macd_line,
                    metadata: json!({"index": "taiex"}) });
            } else if prev_macd_above && !cur_macd_above {
                events.push(TaiexEvent { date: cur.date, kind: TaiexEventKind::MacdDeathCross, value: cur.macd_line,
                    metadata: json!({"index": "taiex"}) });
            }
            // RSI
            if cur.rsi >= 70.0 {
                events.push(TaiexEvent { date: cur.date, kind: TaiexEventKind::RsiOverbought, value: cur.rsi,
                    metadata: json!({"index": "taiex", "rsi": cur.rsi}) });
            } else if cur.rsi > 0.0 && cur.rsi <= 30.0 {
                events.push(TaiexEvent { date: cur.date, kind: TaiexEventKind::RsiOversold, value: cur.rsi,
                    metadata: json!({"index": "taiex", "rsi": cur.rsi}) });
            }
            // Volume surge
            if cur.volume_z >= params.volume_z_threshold {
                events.push(TaiexEvent { date: cur.date, kind: TaiexEventKind::VolumeSurge, value: cur.volume_z,
                    metadata: json!({"index": "taiex", "z": cur.volume_z}) });
            }
            // MA60 break
            if prev.close < ma60[i - 1] && cur.close > ma60[i] && ma60[i] > 0.0 {
                events.push(TaiexEvent { date: cur.date, kind: TaiexEventKind::BreakoutAboveMa60, value: cur.close,
                    metadata: json!({"index": "taiex", "close": cur.close, "ma60": ma60[i]}) });
            } else if prev.close > ma60[i - 1] && cur.close < ma60[i] && ma60[i] > 0.0 {
                events.push(TaiexEvent { date: cur.date, kind: TaiexEventKind::BreakdownBelowMa60, value: cur.close,
                    metadata: json!({"index": "taiex", "close": cur.close, "ma60": ma60[i]}) });
            }
        }
        // 20-day NewHigh / NewLow
        const NEW_HL_LB: usize = 20;
        if series.len() > NEW_HL_LB {
            for i in NEW_HL_LB..series.len() {
                let win = &series[i - NEW_HL_LB..i];
                let max_c = win.iter().map(|p| p.close).fold(f64::NEG_INFINITY, f64::max);
                let min_c = win.iter().map(|p| p.close).fold(f64::INFINITY, f64::min);
                if series[i].close > max_c {
                    events.push(TaiexEvent { date: series[i].date, kind: TaiexEventKind::NewHigh20d, value: series[i].close,
                        metadata: json!({"index": "taiex", "close": series[i].close, "lookback": "20d"}) });
                } else if series[i].close < min_c {
                    events.push(TaiexEvent { date: series[i].date, kind: TaiexEventKind::NewLow20d, value: series[i].close,
                        metadata: json!({"index": "taiex", "close": series[i].close, "lookback": "20d"}) });
                }
            }
        }
        Ok(TaiexOutput { stock_id: RESERVED_STOCK_ID.to_string(), timeframe: params.timeframe, series, events })
    }

    fn produce_facts(&self, output: &Self::Output) -> Vec<Fact> {
        output.events.iter().map(|e| Fact {
            stock_id: output.stock_id.clone(), fact_date: e.date, timeframe: output.timeframe,
            source_core: "taiex_core".to_string(), source_version: "0.1.0".to_string(),
            params_hash: None, statement: format!("TAIEX {:?} on {}: value={:.2}", e.kind, e.date, e.value),
            metadata: e.metadata.clone(),
        }).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn name_warmup_reserved_id() {
        let core = TaiexCore::new();
        assert_eq!(core.name(), "taiex_core");
        assert_eq!(core.warmup_periods(&TaiexParams::default()), (26 * 4).max(60 + 10));
        let series = MarketIndexTwSeries { points: vec![] };
        let out = core.compute(&series, TaiexParams::default()).unwrap();
        assert_eq!(out.stock_id, "_index_taiex_");
    }
}
