// adx_core(P1)— 對齊 m2Spec/oldm2Spec/indicator_cores_momentum.md §六 r2
// Params §6.2(strong_trend / very_strong)/ Output §6.4(僅 adx/+DI/-DI)/ warmup §6.3 ×6

use anyhow::Result;
use chrono::NaiveDate;
use fact_schema::{Fact, IndicatorCore, Timeframe};
use ohlcv_loader::OhlcvSeries;
use serde::Serialize;
use serde_json::json;

inventory::submit! {
    core_registry::CoreRegistration::new(
        "adx_core", "0.1.0", core_registry::CoreKind::Indicator, "P1",
        "ADX Core(Wilder ADX 14 + DI cross)",
    )
}

#[derive(Debug, Clone, Serialize)]
pub struct AdxParams {
    pub period: usize,                // 預設 14
    pub strong_trend_threshold: f64,  // 預設 25.0
    pub very_strong_threshold: f64,   // 預設 50.0
    pub timeframe: Timeframe,
}
impl Default for AdxParams { fn default() -> Self { Self { period: 14, strong_trend_threshold: 25.0, very_strong_threshold: 50.0, timeframe: Timeframe::Daily } } }

#[derive(Debug, Clone, Serialize)]
pub struct AdxOutput {
    pub stock_id: String, pub timeframe: Timeframe,
    pub series: Vec<AdxPoint>,
    #[serde(skip)]
    pub events: Vec<AdxEvent>,
}
#[derive(Debug, Clone, Serialize)]
pub struct AdxPoint { pub date: NaiveDate, pub adx: f64, pub plus_di: f64, pub minus_di: f64 }
#[derive(Debug, Clone, Serialize)]
pub struct AdxEvent { pub date: NaiveDate, pub kind: AdxEventKind, pub value: f64, pub metadata: serde_json::Value }
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum AdxEventKind { StrongTrendStart, VeryStrongTrend, BullishDiCross, BearishDiCross, AdxPeak, UptrendStrength }

pub struct AdxCore;
impl AdxCore { pub fn new() -> Self { AdxCore } }
impl Default for AdxCore { fn default() -> Self { AdxCore::new() } }

impl IndicatorCore for AdxCore {
    type Input = OhlcvSeries;
    type Params = AdxParams;
    type Output = AdxOutput;
    fn name(&self) -> &'static str { "adx_core" }
    fn version(&self) -> &'static str { "0.1.0" }
    fn warmup_periods(&self, params: &Self::Params) -> usize { params.period * 6 }

    fn compute(&self, input: &Self::Input, params: Self::Params) -> Result<Self::Output> {
        let n = input.bars.len();
        if n < 2 {
            return Ok(AdxOutput { stock_id: input.stock_id.clone(), timeframe: params.timeframe, series: Vec::new(), events: Vec::new() });
        }
        let p = params.period as f64;
        let mut tr = vec![0.0; n]; let mut pdm = vec![0.0; n]; let mut mdm = vec![0.0; n];
        for i in 1..n {
            let cur = &input.bars[i]; let prev = &input.bars[i - 1];
            let up = cur.high - prev.high; let dn = prev.low - cur.low;
            if up > dn && up > 0.0 { pdm[i] = up; }
            if dn > up && dn > 0.0 { mdm[i] = dn; }
            tr[i] = (cur.high - cur.low).max((cur.high - prev.close).abs()).max((cur.low - prev.close).abs());
        }
        let mut atr = vec![0.0; n]; let mut psm = vec![0.0; n]; let mut msm = vec![0.0; n];
        let warmup = params.period.min(n - 1);
        let s_tr: f64 = tr[1..=warmup].iter().sum();
        let s_p: f64 = pdm[1..=warmup].iter().sum();
        let s_m: f64 = mdm[1..=warmup].iter().sum();
        atr[warmup] = s_tr / p; psm[warmup] = s_p / p; msm[warmup] = s_m / p;
        for i in (warmup + 1)..n {
            atr[i] = ((p - 1.0) * atr[i - 1] + tr[i]) / p;
            psm[i] = ((p - 1.0) * psm[i - 1] + pdm[i]) / p;
            msm[i] = ((p - 1.0) * msm[i - 1] + mdm[i]) / p;
        }
        let mut series = Vec::with_capacity(n);
        let mut dx = vec![0.0; n];
        for i in 0..n {
            let plus_di = if atr[i] > 0.0 { 100.0 * psm[i] / atr[i] } else { 0.0 };
            let minus_di = if atr[i] > 0.0 { 100.0 * msm[i] / atr[i] } else { 0.0 };
            let denom = plus_di + minus_di;
            dx[i] = if denom > 0.0 { 100.0 * (plus_di - minus_di).abs() / denom } else { 0.0 };
            series.push(AdxPoint { date: input.bars[i].date, plus_di, minus_di, adx: 0.0 });
        }
        let adx_start = (warmup * 2).min(n);
        if adx_start < n {
            let init_sum: f64 = dx[warmup..adx_start].iter().sum();
            let init_n = (adx_start - warmup) as f64;
            if init_n > 0.0 { series[adx_start - 1].adx = init_sum / init_n; }
            for i in adx_start..n {
                series[i].adx = ((p - 1.0) * series[i - 1].adx + dx[i]) / p;
            }
        }
        let mut events = Vec::new();
        for i in 1..series.len() {
            let prev = &series[i - 1]; let cur = &series[i];
            // StrongTrendStart:adx 跨越 threshold
            if prev.adx < params.strong_trend_threshold && cur.adx >= params.strong_trend_threshold {
                events.push(AdxEvent { date: cur.date, kind: AdxEventKind::StrongTrendStart, value: cur.adx,
                    metadata: json!({"event": "strong_trend_start", "adx": cur.adx}) });
            }
            // VeryStrongTrend
            if prev.adx < params.very_strong_threshold && cur.adx >= params.very_strong_threshold {
                events.push(AdxEvent { date: cur.date, kind: AdxEventKind::VeryStrongTrend, value: cur.adx,
                    metadata: json!({"event": "very_strong_trend", "adx": cur.adx}) });
            }
            // DI cross
            let prev_above = prev.plus_di > prev.minus_di;
            let cur_above = cur.plus_di > cur.minus_di;
            if !prev_above && cur_above {
                events.push(AdxEvent { date: cur.date, kind: AdxEventKind::BullishDiCross, value: cur.plus_di,
                    metadata: json!({"event": "di_bullish_cross"}) });
            } else if prev_above && !cur_above {
                events.push(AdxEvent { date: cur.date, kind: AdxEventKind::BearishDiCross, value: cur.plus_di,
                    metadata: json!({"event": "di_bearish_cross"}) });
            }
        }
        // ADX peak detection(連續 5 根降後,前一峰標 peak)
        for i in 5..series.len() {
            let cur = series[i].adx;
            let win_max = series[i - 5..i].iter().map(|s| s.adx).fold(f64::NEG_INFINITY, f64::max);
            if win_max > cur && win_max >= params.strong_trend_threshold {
                // 找 peak idx
                if let Some(peak_idx) = series[i - 5..i].iter().position(|s| (s.adx - win_max).abs() < 1e-9).map(|p| p + i - 5) {
                    if peak_idx == i - 5 {
                        events.push(AdxEvent { date: series[peak_idx].date, kind: AdxEventKind::AdxPeak, value: win_max,
                            metadata: json!({"event": "adx_peak", "value": win_max}) });
                    }
                }
            }
        }
        Ok(AdxOutput { stock_id: input.stock_id.clone(), timeframe: params.timeframe, series, events })
    }

    fn produce_facts(&self, output: &Self::Output) -> Vec<Fact> {
        output.events.iter().map(|e| Fact {
            stock_id: output.stock_id.clone(), fact_date: e.date, timeframe: output.timeframe,
            source_core: "adx_core".to_string(), source_version: "0.1.0".to_string(),
            params_hash: None, statement: format!("ADX {:?} on {}: value={:.2}", e.kind, e.date, e.value),
            metadata: e.metadata.clone(),
        }).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn name_warmup() {
        let core = AdxCore::new();
        assert_eq!(core.name(), "adx_core");
        assert_eq!(core.warmup_periods(&AdxParams::default()), 14 * 6);
    }
}
