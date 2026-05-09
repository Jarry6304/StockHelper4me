// adx_core(P1)— Indicator Core(動量 / 趨勢強度類)
// 對齊 oldm2Spec/indicator_cores_momentum.md §六
// Welles Wilder ADX(1978):+DI / -DI / DX / ADX
//
// **本 PR 範圍**:基本 ADX 計算 + Trending(ADX>25)/Ranging(ADX<20)事件
// TODO:+DI/-DI cross 事件 — 留 PR-future

use anyhow::Result;
use chrono::NaiveDate;
use fact_schema::{Fact, IndicatorCore, Timeframe};
use indicator_kernel::{true_range, wilder_smooth_step};
use ohlcv_loader::OhlcvSeries;
use serde::Serialize;
use serde_json::json;

inventory::submit! {
    core_registry::CoreRegistration::new(
        "adx_core", "0.1.0", core_registry::CoreKind::Indicator, "P1",
        "ADX Core(Wilder ADX 趨勢強度)",
    )
}

#[derive(Debug, Clone, Serialize)]
pub struct AdxParams { pub timeframe: Timeframe, pub period: usize, pub trending_threshold: f64, pub ranging_threshold: f64 }
impl Default for AdxParams { fn default() -> Self { Self { timeframe: Timeframe::Daily, period: 14, trending_threshold: 25.0, ranging_threshold: 20.0 } } }

#[derive(Debug, Clone, Serialize)]
pub struct AdxOutput { pub stock_id: String, pub timeframe: Timeframe, pub series: Vec<AdxPoint>, pub events: Vec<AdxEvent> }
#[derive(Debug, Clone, Serialize)]
pub struct AdxPoint { pub date: NaiveDate, pub plus_di: f64, pub minus_di: f64, pub adx: f64 }
#[derive(Debug, Clone, Serialize)]
pub struct AdxEvent { pub date: NaiveDate, pub kind: AdxEventKind, pub value: f64, pub metadata: serde_json::Value }
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum AdxEventKind { Trending, Ranging, BullishDiCross, BearishDiCross }

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
        // True Range 抽到 indicator_kernel
        let tr = true_range(&input.bars);
        // +DM / -DM(ADX 特有)
        let mut plus_dm = vec![0.0; n];
        let mut minus_dm = vec![0.0; n];
        for i in 1..n {
            let cur = &input.bars[i];
            let prev = &input.bars[i - 1];
            let up_move = cur.high - prev.high;
            let down_move = prev.low - cur.low;
            if up_move > down_move && up_move > 0.0 { plus_dm[i] = up_move; }
            if down_move > up_move && down_move > 0.0 { minus_dm[i] = down_move; }
        }
        // Wilder smoothing(暖機 sum 風格,對齊 Wilder 1978 ADX 標準)
        let mut atr = vec![0.0; n]; let mut pdi_sm = vec![0.0; n]; let mut mdi_sm = vec![0.0; n];
        let warmup = params.period.min(n - 1);
        let sum_tr: f64 = tr[1..=warmup].iter().sum();
        let sum_p: f64 = plus_dm[1..=warmup].iter().sum();
        let sum_m: f64 = minus_dm[1..=warmup].iter().sum();
        atr[warmup] = sum_tr / p; pdi_sm[warmup] = sum_p / p; mdi_sm[warmup] = sum_m / p;
        for i in (warmup + 1)..n {
            atr[i] = wilder_smooth_step(atr[i - 1], tr[i], params.period);
            pdi_sm[i] = wilder_smooth_step(pdi_sm[i - 1], plus_dm[i], params.period);
            mdi_sm[i] = wilder_smooth_step(mdi_sm[i - 1], minus_dm[i], params.period);
        }
        // +DI / -DI / DX / ADX
        let mut series = Vec::with_capacity(n);
        let mut dx = vec![0.0; n];
        for i in 0..n {
            let plus_di = if atr[i] > 0.0 { 100.0 * pdi_sm[i] / atr[i] } else { 0.0 };
            let minus_di = if atr[i] > 0.0 { 100.0 * mdi_sm[i] / atr[i] } else { 0.0 };
            let denom = plus_di + minus_di;
            dx[i] = if denom > 0.0 { 100.0 * (plus_di - minus_di).abs() / denom } else { 0.0 };
            series.push(AdxPoint { date: input.bars[i].date, plus_di, minus_di, adx: 0.0 });
        }
        // ADX = Wilder smoothing of DX,從 warmup * 2 起算
        let adx_start = (warmup * 2).min(n);
        if adx_start < n {
            let init_sum: f64 = dx[warmup..adx_start].iter().sum();
            let init_n = (adx_start - warmup) as f64;
            if init_n > 0.0 { series[adx_start - 1].adx = init_sum / init_n; }
            for i in adx_start..n {
                series[i].adx = wilder_smooth_step(series[i - 1].adx, dx[i], params.period);
            }
        }
        let mut events = Vec::new();
        for i in 1..series.len() {
            let s = &series[i];
            if s.adx >= params.trending_threshold {
                events.push(AdxEvent { date: s.date, kind: AdxEventKind::Trending, value: s.adx,
                    metadata: json!({"adx": s.adx, "plus_di": s.plus_di, "minus_di": s.minus_di}) });
            } else if s.adx > 0.0 && s.adx <= params.ranging_threshold {
                events.push(AdxEvent { date: s.date, kind: AdxEventKind::Ranging, value: s.adx,
                    metadata: json!({"adx": s.adx, "plus_di": s.plus_di, "minus_di": s.minus_di}) });
            }
            // +DI/-DI cross
            let prev = &series[i - 1];
            let prev_above = prev.plus_di > prev.minus_di;
            let cur_above = s.plus_di > s.minus_di;
            if !prev_above && cur_above {
                events.push(AdxEvent { date: s.date, kind: AdxEventKind::BullishDiCross, value: s.plus_di,
                    metadata: json!({"plus_di": s.plus_di, "minus_di": s.minus_di}) });
            } else if prev_above && !cur_above {
                events.push(AdxEvent { date: s.date, kind: AdxEventKind::BearishDiCross, value: s.plus_di,
                    metadata: json!({"plus_di": s.plus_di, "minus_di": s.minus_di}) });
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
    fn name() { assert_eq!(AdxCore::new().name(), "adx_core"); }
}
