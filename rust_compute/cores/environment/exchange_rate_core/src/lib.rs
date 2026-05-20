// exchange_rate_core(P2)— 對齊 m3Spec/environment_cores.md §五 r3
// Params §5.4(currency_pairs / ma_period / key_levels / significant_change)/
// Output §5.6(rate / change_pct / ma_value / TrendState)/ EventKind 4 個

use anyhow::Result;
use chrono::NaiveDate;
use environment_loader::ExchangeRateSeries;
use fact_schema::{Fact, IndicatorCore, Timeframe};
use serde::Serialize;
use serde_json::json;

inventory::submit! {
    core_registry::CoreRegistration::new(
        "exchange_rate_core", "0.1.0", core_registry::CoreKind::Environment, "P2",
        "Exchange Rate Core(MA cross + key level breakout)",
    )
}

const RESERVED_STOCK_ID: &str = "_global_";

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum TrendState { BullishMa, BearishMa, Neutral }

#[derive(Debug, Clone, Serialize)]
pub struct ExchangeRateParams {
    pub timeframe: Timeframe,
    pub currency_pairs: Vec<String>,
    pub ma_period: usize,
    pub key_levels: Vec<f64>,
    pub significant_change_threshold: f64,
    /// Fusion Layer P1.2:TwdStrengthenStreak 連續天數門檻(TWD 連續走強)。
    pub strengthen_streak_days: usize,
}
impl Default for ExchangeRateParams {
    fn default() -> Self {
        Self { timeframe: Timeframe::Daily, currency_pairs: vec!["USD/TWD".to_string()],
            ma_period: 20, key_levels: vec![30.0, 31.0, 32.0], significant_change_threshold: 0.5,
            strengthen_streak_days: 5 }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ExchangeRateOutput {
    pub stock_id: String, pub timeframe: Timeframe,
    pub series: Vec<ExchangeRatePoint>,
    pub events: Vec<ExchangeRateEvent>,
}
#[derive(Debug, Clone, Serialize)]
pub struct ExchangeRatePoint {
    pub date: NaiveDate,
    pub currency_pair: String,
    pub rate: f64,
    pub change_pct: f64,
    pub ma_value: f64,
    pub trend_state: TrendState,
    /// Fusion Layer P1.2b:rate 在尾段 252 個資料點內的百分位(0.0-1.0)。
    pub percentile_252: f64,
}
#[derive(Debug, Clone, Serialize)]
pub struct ExchangeRateEvent { pub date: NaiveDate, pub kind: ExchangeRateEventKind, pub value: f64, pub metadata: serde_json::Value }
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum ExchangeRateEventKind {
    KeyLevelBreakout, KeyLevelBreakdown, SignificantSingleDayMove, MaCross,
    // Fusion Layer P1.2:供 Fusion market_events 用。
    // 註:spec 提的 TwdBreak31 已由 KeyLevelBreakdown(key_levels 含 31.0)涵蓋,不重複新增。
    TwdStrengthenStreak,
}

impl ExchangeRateEventKind {
    /// Fact 嚴重度 — 本 core 自行映射(對齊 fusion_layer §9 #6)。
    fn severity(self) -> fact_schema::Severity {
        use fact_schema::Severity::*;
        use ExchangeRateEventKind::*;
        match self {
            MaCross => Info,
            KeyLevelBreakout | KeyLevelBreakdown | SignificantSingleDayMove
            | TwdStrengthenStreak => Notable,
        }
    }
}

pub struct ExchangeRateCore;
impl ExchangeRateCore { pub fn new() -> Self { ExchangeRateCore } }
impl Default for ExchangeRateCore { fn default() -> Self { ExchangeRateCore::new() } }

fn sma(v: &[f64], p: usize) -> Vec<f64> {
    let mut out = vec![0.0; v.len()];
    if v.is_empty() || p == 0 { return out; }
    let mut s = 0.0;
    for i in 0..v.len() { s += v[i]; if i >= p { s -= v[i - p]; } out[i] = s / (i + 1).min(p) as f64; }
    out
}

/// Fusion Layer P1.2b:回 `values[i]` 在尾段 `window` 個資料點(含自己)內的百分位(0.0-1.0)。
fn percentile_trailing(values: &[f64], i: usize, window: usize) -> f64 {
    if values.is_empty() || i >= values.len() || window == 0 {
        return 0.0;
    }
    let lo = i.saturating_sub(window - 1);
    let win = &values[lo..=i];
    let le = win.iter().filter(|&&v| v <= values[i]).count();
    le as f64 / win.len() as f64
}

impl IndicatorCore for ExchangeRateCore {
    type Input = ExchangeRateSeries;
    type Params = ExchangeRateParams;
    type Output = ExchangeRateOutput;
    fn name(&self) -> &'static str { "exchange_rate_core" }
    fn version(&self) -> &'static str { "0.1.0" }
    /// §5.5:`ma_period + 10`
    fn warmup_periods(&self, params: &Self::Params) -> usize { params.ma_period + 10 }

    fn compute(&self, input: &Self::Input, params: Self::Params) -> Result<Self::Output> {
        let n = input.points.len();
        let pair = format!("{}/TWD", input.currency); // best-guess(input.currency = "USD" 等)
        let rates: Vec<f64> = input.points.iter().map(|p| p.rate.unwrap_or(0.0)).collect();
        let mas = sma(&rates, params.ma_period);
        let mut series = Vec::with_capacity(n);
        let mut prev: Option<f64> = None;
        for i in 0..n {
            let rate = rates[i];
            let change = match prev { Some(p) if p > 0.0 => (rate - p) / p * 100.0, _ => 0.0 };
            let trend = if rate > mas[i] && mas[i] > 0.0 { TrendState::BullishMa }
                else if rate < mas[i] && mas[i] > 0.0 { TrendState::BearishMa }
                else { TrendState::Neutral };
            series.push(ExchangeRatePoint {
                date: input.points[i].date, currency_pair: pair.clone(),
                rate, change_pct: change, ma_value: mas[i], trend_state: trend,
                percentile_252: percentile_trailing(&rates, i, 252),
            });
            prev = Some(rate);
        }
        let mut events = Vec::new();
        for i in 1..series.len() {
            let prev_p = &series[i - 1]; let cur = &series[i];
            // Key level breakout / breakdown
            for &level in &params.key_levels {
                if prev_p.rate < level && cur.rate >= level {
                    events.push(ExchangeRateEvent { date: cur.date, kind: ExchangeRateEventKind::KeyLevelBreakout, value: cur.rate,
                        metadata: json!({"pair": pair, "level": level, "rate": cur.rate}) });
                } else if prev_p.rate > level && cur.rate <= level {
                    events.push(ExchangeRateEvent { date: cur.date, kind: ExchangeRateEventKind::KeyLevelBreakdown, value: cur.rate,
                        metadata: json!({"pair": pair, "level": level, "rate": cur.rate}) });
                }
            }
            // Significant single-day move
            if cur.change_pct.abs() >= params.significant_change_threshold {
                events.push(ExchangeRateEvent { date: cur.date, kind: ExchangeRateEventKind::SignificantSingleDayMove, value: cur.change_pct,
                    metadata: json!({"pair": pair, "change": cur.change_pct}) });
            }
            // MA cross(rate cross MA)
            let prev_above = prev_p.rate > prev_p.ma_value && prev_p.ma_value > 0.0;
            let cur_above = cur.rate > cur.ma_value && cur.ma_value > 0.0;
            if prev_above != cur_above {
                let dir = if cur_above { "above" } else { "below" };
                events.push(ExchangeRateEvent { date: cur.date, kind: ExchangeRateEventKind::MaCross, value: cur.rate,
                    metadata: json!({"pair": pair, "direction": dir, "ma_period": params.ma_period}) });
            }
        }
        // Fusion Layer P1.2:TwdStrengthenStreak — TWD 連續走強(USD/TWD 連續下跌)
        let mut down_streak = 0usize;
        for i in 1..series.len() {
            if series[i].rate < series[i - 1].rate {
                down_streak += 1;
            } else {
                down_streak = 0;
            }
            if down_streak == params.strengthen_streak_days {
                events.push(ExchangeRateEvent { date: series[i].date, kind: ExchangeRateEventKind::TwdStrengthenStreak,
                    value: down_streak as f64,
                    metadata: json!({"pair": pair, "streak_days": down_streak, "rate": series[i].rate}) });
            }
        }
        Ok(ExchangeRateOutput { stock_id: RESERVED_STOCK_ID.to_string(), timeframe: params.timeframe, series, events })
    }

    fn produce_facts(&self, output: &Self::Output) -> Vec<Fact> {
        output.events.iter().map(|e| Fact { severity: e.kind.severity(),
            stock_id: output.stock_id.clone(), fact_date: e.date, timeframe: output.timeframe,
            source_core: "exchange_rate_core".to_string(), source_version: "0.1.0".to_string(),
            params_hash: None, statement: format!("FX {:?} on {}: value={:.4}", e.kind, e.date, e.value),
            metadata: fact_schema::with_event_kind(e.metadata.clone(), &e.kind),
        }).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn name_warmup_reserved_id() {
        let core = ExchangeRateCore::new();
        assert_eq!(core.name(), "exchange_rate_core");
        assert_eq!(core.warmup_periods(&ExchangeRateParams::default()), 30);
        let input = ExchangeRateSeries { currency: "USD".to_string(), points: vec![] };
        let out = core.compute(&input, ExchangeRateParams::default()).unwrap();
        assert_eq!(out.stock_id, "_global_");
    }

    #[test]
    fn severity_and_percentile() {
        use fact_schema::Severity;
        assert_eq!(ExchangeRateEventKind::MaCross.severity(), Severity::Info);
        assert_eq!(ExchangeRateEventKind::TwdStrengthenStreak.severity(), Severity::Notable);
        let v = vec![32.0, 31.5, 31.0];
        assert_eq!(percentile_trailing(&v, 0, 252), 1.0);
        assert!((percentile_trailing(&v, 2, 252) - (1.0 / 3.0)).abs() < 1e-9); // 31.0 為最小
    }
}
