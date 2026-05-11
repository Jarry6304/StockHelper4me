// kd_core(P1)— 對齊 m2Spec/oldm2Spec/indicator_cores_momentum.md §五 r2
// Params §5.2(period 9 / k_smooth 3 / d_smooth 3)/ Output §5.4(僅 k/d)/ warmup §5.3
//
// **Reference(2026-05-10 加)**:
//   period=9:**Asian markets convention / KDJ variant**(George Lane 1957 原版用 14;
//             9 days 是 Asian / 台股慣例,對應 short-term day trader 設定)。
//             ⚠️ 沒明確學術 cite,Phase 2 可考慮對齊國際標準 14
//   k_smooth=3 / d_smooth=3:Lane (1957) 原版 slow stochastic 14/3/3 的 smooth 參數
//   overbought=80 / oversold=20:KDJ 台股慣例(對比 RSI 70/30,KD 走較極端)

use anyhow::Result;
use chrono::NaiveDate;
use fact_schema::{Fact, IndicatorCore, Timeframe};
use ohlcv_loader::OhlcvSeries;
use serde::Serialize;
use serde_json::json;

inventory::submit! {
    core_registry::CoreRegistration::new(
        "kd_core", "0.1.0", core_registry::CoreKind::Indicator, "P1",
        "KD Core(Stochastic %K %D)— 台股慣例 9/3/3",
    )
}

const STREAK_MIN_DAYS: usize = 3;

#[derive(Debug, Clone, Serialize)]
pub struct KdParams {
    pub period: usize,    // 預設 9(台股 §5.6)
    pub k_smooth: usize,  // 預設 3
    pub d_smooth: usize,  // 預設 3
    pub overbought: f64,  // 預設 80.0
    pub oversold: f64,    // 預設 20.0
    pub timeframe: Timeframe,
}
impl Default for KdParams { fn default() -> Self { Self { period: 9, k_smooth: 3, d_smooth: 3, overbought: 80.0, oversold: 20.0, timeframe: Timeframe::Daily } } }

#[derive(Debug, Clone, Serialize)]
pub struct KdOutput {
    pub stock_id: String, pub timeframe: Timeframe,
    pub series: Vec<KdPoint>,
    #[serde(skip)]
    pub events: Vec<KdEvent>,
}
#[derive(Debug, Clone, Serialize)]
pub struct KdPoint { pub date: NaiveDate, pub k: f64, pub d: f64 }
#[derive(Debug, Clone, Serialize)]
pub struct KdEvent { pub date: NaiveDate, pub kind: KdEventKind, pub value: f64, pub metadata: serde_json::Value }
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum KdEventKind { GoldenCross, DeathCross, OverboughtStreak, OversoldStreak, BearishDivergence, BullishDivergence }

pub struct KdCore;
impl KdCore { pub fn new() -> Self { KdCore } }
impl Default for KdCore { fn default() -> Self { KdCore::new() } }

impl IndicatorCore for KdCore {
    type Input = OhlcvSeries;
    type Params = KdParams;
    type Output = KdOutput;
    fn name(&self) -> &'static str { "kd_core" }
    fn version(&self) -> &'static str { "0.1.0" }
    /// §5.3:`period + k_smooth + d_smooth + 10`
    fn warmup_periods(&self, params: &Self::Params) -> usize {
        params.period + params.k_smooth + params.d_smooth + 10
    }

    fn compute(&self, input: &Self::Input, params: Self::Params) -> Result<Self::Output> {
        let n = input.bars.len();
        let mut series = Vec::with_capacity(n);
        // 台股慣例:K = (k_smooth-1)/k_smooth × prev_K + 1/k_smooth × RSV
        // 同 D。預設 k_smooth = 3 → 2/3 prev + 1/3 cur(對齊台股 KD 慣例)
        let mut prev_k = 50.0_f64; let mut prev_d = 50.0_f64;
        for i in 0..n {
            let p = params.period.min(i + 1);
            let start = i + 1 - p;
            let lo = input.bars[start..=i].iter().map(|b| b.low).fold(f64::INFINITY, f64::min);
            let hi = input.bars[start..=i].iter().map(|b| b.high).fold(f64::NEG_INFINITY, f64::max);
            let close = input.bars[i].close;
            let rsv = if hi - lo > 1e-9 { (close - lo) / (hi - lo) * 100.0 } else { 50.0 };
            let ks = params.k_smooth as f64;
            let ds = params.d_smooth as f64;
            let k = ((ks - 1.0) / ks) * prev_k + (1.0 / ks) * rsv;
            let d = ((ds - 1.0) / ds) * prev_d + (1.0 / ds) * k;
            series.push(KdPoint { date: input.bars[i].date, k, d });
            prev_k = k; prev_d = d;
        }
        let mut events = Vec::new();
        for i in 1..series.len() {
            let prev_above = series[i - 1].k > series[i - 1].d;
            let cur_above = series[i].k > series[i].d;
            if !prev_above && cur_above {
                events.push(KdEvent { date: series[i].date, kind: KdEventKind::GoldenCross, value: series[i].k,
                    metadata: json!({"event": "golden_cross", "k": series[i].k, "d": series[i].d}) });
            } else if prev_above && !cur_above {
                events.push(KdEvent { date: series[i].date, kind: KdEventKind::DeathCross, value: series[i].k,
                    metadata: json!({"event": "death_cross", "k": series[i].k, "d": series[i].d}) });
            }
        }
        // streaks
        kd_streak(&series, STREAK_MIN_DAYS, |p| p.k >= params.overbought, KdEventKind::OverboughtStreak, &mut events);
        kd_streak(&series, STREAK_MIN_DAYS, |p| p.k <= params.oversold, KdEventKind::OversoldStreak, &mut events);
        // Divergence vs close
        const DIV_BARS: usize = 20;
        let closes: Vec<f64> = input.bars.iter().map(|b| b.close).collect();
        if series.len() > DIV_BARS {
            for i in DIV_BARS..series.len() {
                let pi = i - DIV_BARS;
                if closes[i] > closes[pi] && series[i].k < series[pi].k {
                    events.push(KdEvent { date: series[i].date, kind: KdEventKind::BearishDivergence, value: series[i].k,
                        metadata: json!({"event": "bearish_divergence"}) });
                } else if closes[i] < closes[pi] && series[i].k > series[pi].k {
                    events.push(KdEvent { date: series[i].date, kind: KdEventKind::BullishDivergence, value: series[i].k,
                        metadata: json!({"event": "bullish_divergence"}) });
                }
            }
        }
        Ok(KdOutput { stock_id: input.stock_id.clone(), timeframe: params.timeframe, series, events })
    }

    fn produce_facts(&self, output: &Self::Output) -> Vec<Fact> {
        output.events.iter().map(|e| Fact {
            stock_id: output.stock_id.clone(), fact_date: e.date, timeframe: output.timeframe,
            source_core: "kd_core".to_string(), source_version: "0.1.0".to_string(),
            params_hash: None, statement: format!("KD {:?} on {}: k={:.2}", e.kind, e.date, e.value),
            metadata: e.metadata.clone(),
        }).collect()
    }
}

fn kd_streak(series: &[KdPoint], min_days: usize, pred: impl Fn(&KdPoint) -> bool, kind: KdEventKind, out: &mut Vec<KdEvent>) {
    let mut start: Option<usize> = None;
    for (i, p) in series.iter().enumerate() {
        if pred(p) { if start.is_none() { start = Some(i); } }
        else if let Some(s) = start.take() {
            if i - s >= min_days {
                out.push(KdEvent { date: series[i - 1].date, kind, value: series[i - 1].k,
                    metadata: json!({"days": i - s, "k": series[i - 1].k}) });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn name_warmup() {
        let core = KdCore::new();
        assert_eq!(core.name(), "kd_core");
        assert_eq!(core.warmup_periods(&KdParams::default()), 9 + 3 + 3 + 10);
    }
}
