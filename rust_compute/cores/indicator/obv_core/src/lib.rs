// obv_core(P1)— 對齊 m2Spec/oldm2Spec/indicator_cores_volume.md §三 r2
// Params §3.2(anchor_date / ma_period)/ Output §3.4(obv + obv_ma + anchor_date)/
// Fact §3.5(divergence + ma_cross + obv_extreme_high)

use anyhow::Result;
use chrono::NaiveDate;
use fact_schema::{Fact, IndicatorCore, Timeframe};
use ohlcv_loader::OhlcvSeries;
use serde::Serialize;
use serde_json::json;

inventory::submit! {
    core_registry::CoreRegistration::new(
        "obv_core", "0.1.0", core_registry::CoreKind::Indicator, "P1",
        "OBV Core(累積式量能 + divergence + obv_ma cross)",
    )
}

const DIV_LOOKBACK: usize = 20;
const EXTREME_LOOKBACK: usize = 126; // 6m ≈ 126 trading days

#[derive(Debug, Clone, Serialize)]
pub struct ObvParams {
    pub timeframe: Timeframe,
    pub anchor_date: Option<NaiveDate>,
    pub ma_period: Option<usize>,
}
impl Default for ObvParams {
    fn default() -> Self { Self { timeframe: Timeframe::Daily, anchor_date: None, ma_period: Some(20) } }
}

#[derive(Debug, Clone, Serialize)]
pub struct ObvOutput {
    pub stock_id: String, pub timeframe: Timeframe,
    pub anchor_date: NaiveDate,
    pub series: Vec<ObvPoint>,
    #[serde(skip)]
    pub events: Vec<ObvEvent>,
}
#[derive(Debug, Clone, Serialize)]
pub struct ObvPoint {
    pub date: NaiveDate,
    pub obv: f64,                  // 累積值(spec §3.4 用 f64 而非 i64)
    pub obv_ma: Option<f64>,
}
#[derive(Debug, Clone, Serialize)]
pub struct ObvEvent { pub date: NaiveDate, pub kind: ObvEventKind, pub value: f64, pub metadata: serde_json::Value }
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum ObvEventKind { BullishDivergence, BearishDivergence, ObvMaBullishCross, ObvMaBearishCross, ObvExtremeHigh, ObvExtremeLow }

pub struct ObvCore;
impl ObvCore { pub fn new() -> Self { ObvCore } }
impl Default for ObvCore { fn default() -> Self { ObvCore::new() } }

impl IndicatorCore for ObvCore {
    type Input = OhlcvSeries;
    type Params = ObvParams;
    type Output = ObvOutput;
    fn name(&self) -> &'static str { "obv_core" }
    fn version(&self) -> &'static str { "0.1.0" }
    /// §3.3:有 ma_period 時 `p + 10`,無則 0
    fn warmup_periods(&self, params: &Self::Params) -> usize {
        match params.ma_period { Some(p) => p + 10, None => 0 }
    }

    fn compute(&self, input: &Self::Input, params: Self::Params) -> Result<Self::Output> {
        let n = input.bars.len();
        // 找 anchor index
        let anchor_idx = match params.anchor_date {
            Some(d) => input.bars.iter().position(|b| b.date >= d).unwrap_or(0),
            None => 0,
        };
        let anchor_date = if n > 0 { input.bars[anchor_idx].date } else {
            chrono::NaiveDate::from_ymd_opt(1900, 1, 1).unwrap()
        };
        // OBV 累積(spec §3.6:從 anchor 起累積)
        let mut series = Vec::with_capacity(n.saturating_sub(anchor_idx));
        let mut obv: f64 = 0.0;
        let mut prev_close: Option<f64> = None;
        for i in anchor_idx..n {
            let b = &input.bars[i];
            let v = b.volume.unwrap_or(0) as f64;
            if let Some(prev) = prev_close {
                if b.close > prev { obv += v; }
                else if b.close < prev { obv -= v; }
            }
            series.push(ObvPoint { date: b.date, obv, obv_ma: None });
            prev_close = Some(b.close);
        }
        // OBV MA
        if let Some(ma_period) = params.ma_period {
            let ma_period = ma_period.max(1);
            let mut sum = 0.0;
            for i in 0..series.len() {
                sum += series[i].obv;
                if i >= ma_period { sum -= series[i - ma_period].obv; }
                let div = (i + 1).min(ma_period) as f64;
                series[i].obv_ma = Some(sum / div);
            }
        }
        let mut events = Vec::new();
        // Divergence vs price(close)
        let closes: Vec<f64> = input.bars[anchor_idx..].iter().map(|b| b.close).collect();
        if series.len() > DIV_LOOKBACK {
            for i in DIV_LOOKBACK..series.len() {
                let pi = i - DIV_LOOKBACK;
                if closes[i] > closes[pi] && series[i].obv < series[pi].obv {
                    events.push(ObvEvent { date: series[i].date, kind: ObvEventKind::BearishDivergence, value: series[i].obv,
                        metadata: json!({"event": "bearish_divergence", "price_date": input.bars[anchor_idx + pi].date, "obv_date": series[i].date}) });
                } else if closes[i] < closes[pi] && series[i].obv > series[pi].obv {
                    events.push(ObvEvent { date: series[i].date, kind: ObvEventKind::BullishDivergence, value: series[i].obv,
                        metadata: json!({"event": "bullish_divergence", "price_date": input.bars[anchor_idx + pi].date, "obv_date": series[i].date}) });
                }
            }
        }
        // OBV vs OBV_MA cross
        if params.ma_period.is_some() {
            for i in 1..series.len() {
                if let (Some(prev_ma), Some(cur_ma)) = (series[i - 1].obv_ma, series[i].obv_ma) {
                    let prev_above = series[i - 1].obv > prev_ma;
                    let cur_above = series[i].obv > cur_ma;
                    if !prev_above && cur_above {
                        events.push(ObvEvent { date: series[i].date, kind: ObvEventKind::ObvMaBullishCross, value: series[i].obv,
                            metadata: json!({"event": "obv_ma_bullish_cross", "ma_period": params.ma_period.unwrap()}) });
                    } else if prev_above && !cur_above {
                        events.push(ObvEvent { date: series[i].date, kind: ObvEventKind::ObvMaBearishCross, value: series[i].obv,
                            metadata: json!({"event": "obv_ma_bearish_cross", "ma_period": params.ma_period.unwrap()}) });
                    }
                }
            }
        }
        // OBV extreme high/low(6m lookback)
        if series.len() > EXTREME_LOOKBACK {
            for i in EXTREME_LOOKBACK..series.len() {
                let win = &series[i - EXTREME_LOOKBACK..i];
                let max_o = win.iter().map(|p| p.obv).fold(f64::NEG_INFINITY, f64::max);
                let min_o = win.iter().map(|p| p.obv).fold(f64::INFINITY, f64::min);
                if series[i].obv > max_o {
                    events.push(ObvEvent { date: series[i].date, kind: ObvEventKind::ObvExtremeHigh, value: series[i].obv,
                        metadata: json!({"event": "obv_extreme_high", "lookback": "6m"}) });
                } else if series[i].obv < min_o {
                    events.push(ObvEvent { date: series[i].date, kind: ObvEventKind::ObvExtremeLow, value: series[i].obv,
                        metadata: json!({"event": "obv_extreme_low", "lookback": "6m"}) });
                }
            }
        }
        Ok(ObvOutput { stock_id: input.stock_id.clone(), timeframe: params.timeframe, anchor_date, series, events })
    }

    fn produce_facts(&self, output: &Self::Output) -> Vec<Fact> {
        output.events.iter().map(|e| Fact {
            stock_id: output.stock_id.clone(), fact_date: e.date, timeframe: output.timeframe,
            source_core: "obv_core".to_string(), source_version: "0.1.0".to_string(),
            params_hash: None, statement: format!("OBV {:?} on {}: obv={:.0}", e.kind, e.date, e.value),
            metadata: e.metadata.clone(),
        }).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn name_warmup_with_ma() {
        let core = ObvCore::new();
        assert_eq!(core.name(), "obv_core");
        assert_eq!(core.warmup_periods(&ObvParams::default()), 30); // 20 + 10
    }
    #[test]
    fn warmup_no_ma() {
        let params = ObvParams { timeframe: Timeframe::Daily, anchor_date: None, ma_period: None };
        assert_eq!(ObvCore::new().warmup_periods(&params), 0);
    }
}
