// taiex_core(P2)— Environment Core
// 對齊 m3Spec/environment_cores.md §三 r3
// Params §3.4(MACD/RSI/volume_z/trend_lookback)/ Output §3.6(series_by_index + event.index_code)
// EventKind 9 個(§3.6)/ stock_id 保留字:
//   - TAIEX → _index_taiex_
//   - TPEx  → _index_tpex_
// 兩者並列大盤,Loader 拆兩條獨立序列,Core 對每條獨立 compute,
// 事件依 index_code 寫入對應保留 stock_id(§3.7 Fact 範例)

use anyhow::Result;
use chrono::NaiveDate;
use environment_loader::{MarketIndexTwRaw, MarketIndexTwSeries};
use fact_schema::{Fact, IndicatorCore, Timeframe};
use serde::Serialize;
use serde_json::json;

inventory::submit! {
    core_registry::CoreRegistration::new(
        "taiex_core", "0.2.0", core_registry::CoreKind::Environment, "P2",
        "TAIEX Core(加權指數 + 櫃買 — MACD/RSI/Volume Z + MA60 trend)",
    )
}

const RESERVED_TAIEX: &str = "_index_taiex_";
const RESERVED_TPEX: &str = "_index_tpex_";

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

/// §3.6:TAIEX / TPEx 兩條並列大盤序列,各自獨立保留字
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum TaiexIndexCode { Taiex, Tpex }

impl TaiexIndexCode {
    pub fn reserved_stock_id(self) -> &'static str {
        match self { TaiexIndexCode::Taiex => RESERVED_TAIEX, TaiexIndexCode::Tpex => RESERVED_TPEX }
    }
    pub fn label(self) -> &'static str {
        match self { TaiexIndexCode::Taiex => "taiex", TaiexIndexCode::Tpex => "tpex" }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TaiexSeriesEntry {
    pub index_code: TaiexIndexCode,
    pub series: Vec<TaiexPoint>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TaiexOutput {
    /// dispatch_indicator 用:取兩條序列中較完整者作 indicator_values lookup stock_id
    pub stock_id: String,
    pub timeframe: Timeframe,
    /// §3.6:TAIEX 與 TPEx 各一條
    pub series_by_index: Vec<TaiexSeriesEntry>,
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
pub struct TaiexEvent {
    pub date: NaiveDate,
    pub index_code: TaiexIndexCode,
    pub kind: TaiexEventKind,
    pub value: f64,
    pub metadata: serde_json::Value,
}

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

/// 對單一指數(TAIEX 或 TPEx)算 series + events
fn compute_one(
    raw: &[MarketIndexTwRaw],
    params: &TaiexParams,
    index_code: TaiexIndexCode,
) -> (Vec<TaiexPoint>, Vec<TaiexEvent>) {
    let n = raw.len();
    let closes: Vec<f64> = raw.iter().map(|p| p.close.unwrap_or(0.0)).collect();
    let volumes: Vec<i64> = raw.iter().map(|p| p.volume.unwrap_or(0)).collect();
    // MACD
    let ema_fast = ema(&closes, params.macd_fast);
    let ema_slow = ema(&closes, params.macd_slow);
    let macd_line: Vec<f64> = (0..n).map(|i| ema_fast[i] - ema_slow[i]).collect();
    let macd_signal = ema(&macd_line, params.macd_signal);
    let rsi = wilder_rsi(&closes, params.rsi_period);
    let ma60 = sma(&closes, params.trend_lookback_bars);
    // Volume z(lookback bar window)
    let vol_z: Vec<f64> = (0..n).map(|i| {
        let lb = params.trend_lookback_bars.min(i);
        if lb < 10 { return 0.0; }
        let win: Vec<f64> = volumes[i - lb..i].iter().map(|&v| v as f64).collect();
        let mean = win.iter().sum::<f64>() / win.len() as f64;
        let var = win.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / win.len() as f64;
        let std = var.sqrt();
        if std > 0.0 { (volumes[i] as f64 - mean) / std } else { 0.0 }
    }).collect();

    let label = index_code.label();
    let mut series = Vec::with_capacity(n);
    let mut prev_close: Option<f64> = None;
    for i in 0..n {
        let close = closes[i];
        let change_pct = match prev_close { Some(c) if c > 0.0 => (close - c) / c * 100.0, _ => 0.0 };
        let trend_state = if close > ma60[i] && ma60[i] > 0.0 { TrendState::BullishMa }
            else if close < ma60[i] && ma60[i] > 0.0 { TrendState::BearishMa }
            else { TrendState::Neutral };
        series.push(TaiexPoint {
            date: raw[i].date, close, volume: volumes[i], change_pct,
            macd_line: macd_line[i], macd_signal: macd_signal[i],
            macd_histogram: macd_line[i] - macd_signal[i],
            rsi: rsi[i], volume_z: vol_z[i], trend_state,
        });
        prev_close = Some(close);
    }

    let mut events = Vec::new();
    for i in 1..series.len() {
        let prev = &series[i - 1]; let cur = &series[i];
        let prev_macd_above = prev.macd_line > prev.macd_signal;
        let cur_macd_above = cur.macd_line > cur.macd_signal;
        if !prev_macd_above && cur_macd_above {
            events.push(TaiexEvent { date: cur.date, index_code, kind: TaiexEventKind::MacdGoldenCross, value: cur.macd_line,
                metadata: json!({"index": label}) });
        } else if prev_macd_above && !cur_macd_above {
            events.push(TaiexEvent { date: cur.date, index_code, kind: TaiexEventKind::MacdDeathCross, value: cur.macd_line,
                metadata: json!({"index": label}) });
        }
        if cur.rsi >= 70.0 {
            events.push(TaiexEvent { date: cur.date, index_code, kind: TaiexEventKind::RsiOverbought, value: cur.rsi,
                metadata: json!({"index": label, "rsi": cur.rsi}) });
        } else if cur.rsi > 0.0 && cur.rsi <= 30.0 {
            events.push(TaiexEvent { date: cur.date, index_code, kind: TaiexEventKind::RsiOversold, value: cur.rsi,
                metadata: json!({"index": label, "rsi": cur.rsi}) });
        }
        if cur.volume_z >= params.volume_z_threshold {
            events.push(TaiexEvent { date: cur.date, index_code, kind: TaiexEventKind::VolumeSurge, value: cur.volume_z,
                metadata: json!({"index": label, "z": cur.volume_z}) });
        }
        if prev.close < ma60[i - 1] && cur.close > ma60[i] && ma60[i] > 0.0 {
            events.push(TaiexEvent { date: cur.date, index_code, kind: TaiexEventKind::BreakoutAboveMa60, value: cur.close,
                metadata: json!({"index": label, "close": cur.close, "ma60": ma60[i]}) });
        } else if prev.close > ma60[i - 1] && cur.close < ma60[i] && ma60[i] > 0.0 {
            events.push(TaiexEvent { date: cur.date, index_code, kind: TaiexEventKind::BreakdownBelowMa60, value: cur.close,
                metadata: json!({"index": label, "close": cur.close, "ma60": ma60[i]}) });
        }
    }
    const NEW_HL_LB: usize = 20;
    if series.len() > NEW_HL_LB {
        for i in NEW_HL_LB..series.len() {
            let win = &series[i - NEW_HL_LB..i];
            let max_c = win.iter().map(|p| p.close).fold(f64::NEG_INFINITY, f64::max);
            let min_c = win.iter().map(|p| p.close).fold(f64::INFINITY, f64::min);
            if series[i].close > max_c {
                events.push(TaiexEvent { date: series[i].date, index_code, kind: TaiexEventKind::NewHigh20d, value: series[i].close,
                    metadata: json!({"index": label, "close": series[i].close, "lookback": "20d"}) });
            } else if series[i].close < min_c {
                events.push(TaiexEvent { date: series[i].date, index_code, kind: TaiexEventKind::NewLow20d, value: series[i].close,
                    metadata: json!({"index": label, "close": series[i].close, "lookback": "20d"}) });
            }
        }
    }
    (series, events)
}

impl IndicatorCore for TaiexCore {
    type Input = MarketIndexTwSeries;
    type Params = TaiexParams;
    type Output = TaiexOutput;
    fn name(&self) -> &'static str { "taiex_core" }
    fn version(&self) -> &'static str { "0.2.0" }
    /// §3.5:`(macd_slow * 4).max(trend_lookback_bars + 10)`
    fn warmup_periods(&self, params: &Self::Params) -> usize {
        (params.macd_slow * 4).max(params.trend_lookback_bars + 10)
    }

    fn compute(&self, input: &Self::Input, params: Self::Params) -> Result<Self::Output> {
        let (taiex_series, mut events) = compute_one(&input.taiex, &params, TaiexIndexCode::Taiex);
        let (tpex_series, tpex_events) = compute_one(&input.tpex, &params, TaiexIndexCode::Tpex);
        events.extend(tpex_events);
        let series_by_index = vec![
            TaiexSeriesEntry { index_code: TaiexIndexCode::Taiex, series: taiex_series },
            TaiexSeriesEntry { index_code: TaiexIndexCode::Tpex,  series: tpex_series  },
        ];
        Ok(TaiexOutput {
            stock_id: RESERVED_TAIEX.to_string(),
            timeframe: params.timeframe,
            series_by_index,
            events,
        })
    }

    fn produce_facts(&self, output: &Self::Output) -> Vec<Fact> {
        output.events.iter().map(|e| Fact {
            stock_id: e.index_code.reserved_stock_id().to_string(),
            fact_date: e.date, timeframe: output.timeframe,
            source_core: "taiex_core".to_string(), source_version: "0.2.0".to_string(),
            params_hash: None,
            statement: format!("{} {:?} on {}: value={:.2}",
                e.index_code.label().to_uppercase(), e.kind, e.date, e.value),
            metadata: e.metadata.clone(),
        }).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_series() -> MarketIndexTwSeries {
        MarketIndexTwSeries { taiex: vec![], tpex: vec![] }
    }

    #[test]
    fn name_warmup_reserved_id() {
        let core = TaiexCore::new();
        assert_eq!(core.name(), "taiex_core");
        assert_eq!(core.warmup_periods(&TaiexParams::default()), (26 * 4).max(60 + 10));
        let out = core.compute(&empty_series(), TaiexParams::default()).unwrap();
        assert_eq!(out.stock_id, "_index_taiex_");
        assert_eq!(out.series_by_index.len(), 2);
        assert_eq!(out.series_by_index[0].index_code, TaiexIndexCode::Taiex);
        assert_eq!(out.series_by_index[1].index_code, TaiexIndexCode::Tpex);
    }

    #[test]
    fn reserved_stock_ids() {
        assert_eq!(TaiexIndexCode::Taiex.reserved_stock_id(), "_index_taiex_");
        assert_eq!(TaiexIndexCode::Tpex.reserved_stock_id(), "_index_tpex_");
    }

    #[test]
    fn produce_facts_splits_by_index_code() {
        // 兩 events 不同 index_code,分別寫入對應保留 stock_id
        let output = TaiexOutput {
            stock_id: "_index_taiex_".to_string(),
            timeframe: Timeframe::Daily,
            series_by_index: vec![],
            events: vec![
                TaiexEvent {
                    date: NaiveDate::from_ymd_opt(2026, 4, 22).unwrap(),
                    index_code: TaiexIndexCode::Taiex,
                    kind: TaiexEventKind::MacdGoldenCross,
                    value: 100.0,
                    metadata: json!({"index": "taiex"}),
                },
                TaiexEvent {
                    date: NaiveDate::from_ymd_opt(2026, 4, 23).unwrap(),
                    index_code: TaiexIndexCode::Tpex,
                    kind: TaiexEventKind::RsiOverbought,
                    value: 78.0,
                    metadata: json!({"index": "tpex"}),
                },
            ],
        };
        let core = TaiexCore::new();
        let facts = core.produce_facts(&output);
        assert_eq!(facts.len(), 2);
        assert_eq!(facts[0].stock_id, "_index_taiex_");
        assert!(facts[0].statement.starts_with("TAIEX"));
        assert_eq!(facts[1].stock_id, "_index_tpex_");
        assert!(facts[1].statement.starts_with("TPEX"));
    }
}
