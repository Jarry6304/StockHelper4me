// atr_core(P1)— 對齊 m2Spec/oldm2Spec/indicator_cores_volatility.md §四 r2
// Params §4.2(period only)/ Output §4.4(atr + atr_pct)/ Fact §4.5(3 種)
//
// **Reference(2026-05-10 加)**:
//   period=14:Wilder, J. Welles Jr. (1978). "New Concepts in Technical Trading
//             Systems". Trend Research. Ch. 21 — 原作者選 14 對應 ~2 週 cycle
//   atr_pct 公式 = ATR / close × 100:Wilder 原版「normalised volatility」
//   1y lookback / 14d expansion 50%:無明確學術出處,14d 對齊 Wilder ATR period=14 語意一致

use anyhow::Result;
use chrono::NaiveDate;
use fact_schema::{Fact, IndicatorCore, Timeframe};
use ohlcv_loader::OhlcvSeries;
use serde::Serialize;
use serde_json::json;

inventory::submit! {
    core_registry::CoreRegistration::new(
        "atr_core", "0.1.0", core_registry::CoreKind::Indicator, "P1",
        "ATR Core(Wilder ATR + atr_pct + 波動率事件)",
    )
}

/// 1 年 lookback 給 volatility extreme high/low(對齊 §4.5 Fact lookback "1y")
const LOOKBACK_1Y: usize = 252;
/// volatility expansion lookback days — 對齊 Wilder ATR period=14(2026-05-11 由 10 改 14)
const EXPANSION_LOOKBACK: usize = 14;
const EXPANSION_THRESHOLD: f64 = 0.5; // 50%

#[derive(Debug, Clone, Serialize)]
pub struct AtrParams {
    pub period: usize,
    pub timeframe: Timeframe,
}
impl Default for AtrParams { fn default() -> Self { Self { period: 14, timeframe: Timeframe::Daily } } }

#[derive(Debug, Clone, Serialize)]
pub struct AtrOutput {
    pub stock_id: String, pub timeframe: Timeframe,
    pub series: Vec<AtrPoint>,
    #[serde(skip)]
    pub events: Vec<AtrEvent>,
}
#[derive(Debug, Clone, Serialize)]
pub struct AtrPoint { pub date: NaiveDate, pub atr: f64, pub atr_pct: f64 }
#[derive(Debug, Clone, Serialize)]
pub struct AtrEvent { pub date: NaiveDate, pub kind: AtrEventKind, pub value: f64, pub metadata: serde_json::Value }
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum AtrEventKind { VolatilityExtremeHigh, VolatilityExtremeLow, VolatilityExpansion }

pub struct AtrCore;
impl AtrCore { pub fn new() -> Self { AtrCore } }
impl Default for AtrCore { fn default() -> Self { AtrCore::new() } }

pub fn wilder_atr(bars: &[ohlcv_loader::OhlcvBar], period: usize) -> Vec<f64> {
    let n = bars.len();
    if n == 0 || period == 0 { return vec![0.0; n]; }
    let mut tr = Vec::with_capacity(n);
    tr.push(bars[0].high - bars[0].low);
    for i in 1..n {
        let prev = bars[i - 1].close;
        let h = bars[i].high; let l = bars[i].low;
        let cands = [(h - l).abs(), (h - prev).abs(), (l - prev).abs()];
        tr.push(cands.iter().cloned().fold(0.0_f64, f64::max));
    }
    let mut atr = vec![0.0; n];
    let warmup = period.min(n);
    let mut sum = 0.0;
    for i in 0..warmup { sum += tr[i]; atr[i] = sum / (i + 1) as f64; }
    for i in warmup..n { atr[i] = ((period as f64 - 1.0) * atr[i - 1] + tr[i]) / period as f64; }
    atr
}

impl IndicatorCore for AtrCore {
    type Input = OhlcvSeries;
    type Params = AtrParams;
    type Output = AtrOutput;
    fn name(&self) -> &'static str { "atr_core" }
    fn version(&self) -> &'static str { "0.1.0" }
    fn warmup_periods(&self, params: &Self::Params) -> usize { params.period * 4 }

    fn compute(&self, input: &Self::Input, params: Self::Params) -> Result<Self::Output> {
        let atrs = wilder_atr(&input.bars, params.period);
        let series: Vec<AtrPoint> = (0..input.bars.len()).map(|i| {
            let close = input.bars[i].close;
            let atr = atrs[i];
            let atr_pct = if close > 0.0 { atr / close * 100.0 } else { 0.0 };
            AtrPoint { date: input.bars[i].date, atr, atr_pct }
        }).collect();
        let mut events = Vec::new();
        // Volatility extreme high/low(1y lookback)
        if series.len() > LOOKBACK_1Y {
            for i in LOOKBACK_1Y..series.len() {
                let win = &series[i - LOOKBACK_1Y..i];
                let max_pct = win.iter().map(|p| p.atr_pct).fold(f64::NEG_INFINITY, f64::max);
                let min_pct = win.iter().filter(|p| p.atr_pct > 0.0).map(|p| p.atr_pct).fold(f64::INFINITY, f64::min);
                if series[i].atr_pct > max_pct {
                    events.push(AtrEvent { date: series[i].date, kind: AtrEventKind::VolatilityExtremeHigh, value: series[i].atr_pct,
                        metadata: json!({"event": "volatility_extreme_high", "value_pct": series[i].atr_pct, "lookback": "1y"}) });
                } else if series[i].atr_pct > 0.0 && series[i].atr_pct < min_pct {
                    events.push(AtrEvent { date: series[i].date, kind: AtrEventKind::VolatilityExtremeLow, value: series[i].atr_pct,
                        metadata: json!({"event": "volatility_extreme_low", "value_pct": series[i].atr_pct, "lookback": "1y"}) });
                }
            }
        }
        // Volatility expansion(N 天內 atr_pct 增 +50%)
        if series.len() > EXPANSION_LOOKBACK {
            for i in EXPANSION_LOOKBACK..series.len() {
                let prev = series[i - EXPANSION_LOOKBACK].atr_pct;
                let cur = series[i].atr_pct;
                if prev > 0.0 && (cur - prev) / prev >= EXPANSION_THRESHOLD {
                    events.push(AtrEvent { date: series[i].date, kind: AtrEventKind::VolatilityExpansion, value: (cur - prev) / prev * 100.0,
                        metadata: json!({"event": "volatility_expansion", "from": prev, "to": cur, "days": EXPANSION_LOOKBACK}) });
                }
            }
        }
        Ok(AtrOutput { stock_id: input.stock_id.clone(), timeframe: params.timeframe, series, events })
    }

    fn produce_facts(&self, output: &Self::Output) -> Vec<Fact> {
        output.events.iter().map(|e| Fact {
            stock_id: output.stock_id.clone(), fact_date: e.date, timeframe: output.timeframe,
            source_core: "atr_core".to_string(), source_version: "0.1.0".to_string(),
            params_hash: None, statement: format!("ATR {:?} on {}: value={:.2}", e.kind, e.date, e.value),
            metadata: e.metadata.clone(),
        }).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn name_warmup() {
        let core = AtrCore::new();
        assert_eq!(core.name(), "atr_core");
        assert_eq!(core.warmup_periods(&AtrParams::default()), 56);
    }
}
