// cci_core(P3)— 對齊 m3Spec/indicator_cores_momentum.md §十
// Params §10.2 / warmup §10.3 / Output §10.4 / Fact §10.5
//
// Reference:
//   Lambert, D. R. (1980), "Commodities Channel Index: Tool for Trading Cyclic Trends",
//     Commodities magazine — 原作者選 period=20、±100 為主要區間
//   ±200 extreme zone:Lambert 原版「85% of values fall within ±100」延伸
//
// CCI 公式:
//   TP = (high + low + close) / 3                  ;Typical Price
//   SMA_TP = SMA(TP, period)
//   MD = Mean(|TP_i - SMA_TP|, period)              ;Mean Absolute Deviation
//   CCI = (TP - SMA_TP) / (0.015 * MD)

use anyhow::Result;
use chrono::NaiveDate;
use fact_schema::{Fact, IndicatorCore, Timeframe};
use ohlcv_loader::OhlcvSeries;
use serde::Serialize;
use serde_json::json;

inventory::submit! {
    core_registry::CoreRegistration::new(
        "cci_core", "0.1.0", core_registry::CoreKind::Indicator, "P3",
        "CCI Core(Commodity Channel Index,±100 / ±200 zone)",
    )
}

const CCI_CONST: f64 = 0.015;

#[derive(Debug, Clone, Serialize)]
pub struct CciParams {
    pub period: usize,
    pub overbought: f64,
    pub oversold: f64,
    pub extreme_high: f64,
    pub extreme_low: f64,
    pub timeframe: Timeframe,
}
impl Default for CciParams {
    fn default() -> Self {
        Self {
            period: 20,
            overbought: 100.0,
            oversold: -100.0,
            extreme_high: 200.0,
            extreme_low: -200.0,
            timeframe: Timeframe::Daily,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CciOutput {
    pub stock_id: String,
    pub timeframe: Timeframe,
    pub series: Vec<CciPoint>,
    #[serde(skip)]
    pub events: Vec<CciEvent>,
}
#[derive(Debug, Clone, Serialize)]
pub struct CciPoint {
    pub date: NaiveDate,
    pub value: f64,
}
#[derive(Debug, Clone, Serialize)]
pub struct CciEvent {
    pub date: NaiveDate,
    pub kind: CciEventKind,
    pub value: f64,
    pub metadata: serde_json::Value,
}
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum CciEventKind {
    ExtremeHigh,
    ExtremeLow,
    OverboughtEntry,
    OversoldEntry,
    ZeroCrossPositive,
    ZeroCrossNegative,
}

pub struct CciCore;
impl CciCore {
    pub fn new() -> Self {
        CciCore
    }
}
impl Default for CciCore {
    fn default() -> Self {
        CciCore::new()
    }
}

impl IndicatorCore for CciCore {
    type Input = OhlcvSeries;
    type Params = CciParams;
    type Output = CciOutput;
    fn name(&self) -> &'static str {
        "cci_core"
    }
    fn version(&self) -> &'static str {
        "0.1.0"
    }
    fn warmup_periods(&self, params: &Self::Params) -> usize {
        params.period + 5
    }

    fn compute(&self, input: &Self::Input, params: Self::Params) -> Result<Self::Output> {
        let n = input.bars.len();
        let p = params.period;
        // 計算 TP 序列
        let tp: Vec<f64> = input
            .bars
            .iter()
            .map(|b| (b.high + b.low + b.close) / 3.0)
            .collect();
        let mut series = Vec::with_capacity(n);
        for i in 0..n {
            if i + 1 < p {
                series.push(CciPoint {
                    date: input.bars[i].date,
                    value: 0.0,
                });
                continue;
            }
            let start = i + 1 - p;
            let sma_tp: f64 = tp[start..=i].iter().sum::<f64>() / p as f64;
            let md: f64 =
                tp[start..=i].iter().map(|v| (v - sma_tp).abs()).sum::<f64>() / p as f64;
            let value = if md > 0.0 {
                (tp[i] - sma_tp) / (CCI_CONST * md)
            } else {
                0.0
            };
            series.push(CciPoint {
                date: input.bars[i].date,
                value,
            });
        }

        let mut events = Vec::new();
        for i in p..n {
            let cur = series[i].value;
            let prev = series[i - 1].value;

            // extreme zone:從非極值區間進入極值區間才觸發(edge trigger,對齊 P2 設計)
            if cur > params.extreme_high && prev <= params.extreme_high {
                events.push(CciEvent {
                    date: series[i].date,
                    kind: CciEventKind::ExtremeHigh,
                    value: cur,
                    metadata: json!({"event": "extreme_high", "value": cur, "threshold": params.extreme_high}),
                });
            } else if cur < params.extreme_low && prev >= params.extreme_low {
                events.push(CciEvent {
                    date: series[i].date,
                    kind: CciEventKind::ExtremeLow,
                    value: cur,
                    metadata: json!({"event": "extreme_low", "value": cur, "threshold": params.extreme_low}),
                });
            }

            // overbought / oversold entry(edge trigger)
            if cur > params.overbought && prev <= params.overbought {
                events.push(CciEvent {
                    date: series[i].date,
                    kind: CciEventKind::OverboughtEntry,
                    value: cur,
                    metadata: json!({"event": "overbought_entry", "threshold": params.overbought}),
                });
            } else if cur < params.oversold && prev >= params.oversold {
                events.push(CciEvent {
                    date: series[i].date,
                    kind: CciEventKind::OversoldEntry,
                    value: cur,
                    metadata: json!({"event": "oversold_entry", "threshold": params.oversold}),
                });
            }

            // zero line cross
            if prev <= 0.0 && cur > 0.0 {
                events.push(CciEvent {
                    date: series[i].date,
                    kind: CciEventKind::ZeroCrossPositive,
                    value: cur,
                    metadata: json!({"event": "zero_cross_positive"}),
                });
            } else if prev >= 0.0 && cur < 0.0 {
                events.push(CciEvent {
                    date: series[i].date,
                    kind: CciEventKind::ZeroCrossNegative,
                    value: cur,
                    metadata: json!({"event": "zero_cross_negative"}),
                });
            }
        }

        Ok(CciOutput {
            stock_id: input.stock_id.clone(),
            timeframe: params.timeframe,
            series,
            events,
        })
    }

    fn produce_facts(&self, output: &Self::Output) -> Vec<Fact> {
        output
            .events
            .iter()
            .map(|e| Fact {
                stock_id: output.stock_id.clone(),
                fact_date: e.date,
                timeframe: output.timeframe,
                source_core: "cci_core".to_string(),
                source_version: "0.1.0".to_string(),
                params_hash: None,
                statement: format!("CCI {:?} on {}: value={:.2}", e.kind, e.date, e.value),
                metadata: e.metadata.clone(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use fact_schema::Timeframe;
    use ohlcv_loader::{OhlcvBar, OhlcvSeries};

    fn make_series(closes: Vec<f64>) -> OhlcvSeries {
        let bars = closes
            .into_iter()
            .enumerate()
            .map(|(i, c)| OhlcvBar {
                date: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap()
                    + chrono::Duration::days(i as i64),
                open: c,
                high: c,
                low: c,
                close: c,
                volume: Some(1000),
            })
            .collect();
        OhlcvSeries {
            stock_id: "TEST".to_string(),
            timeframe: Timeframe::Daily,
            bars,
        }
    }

    #[test]
    fn name_and_warmup() {
        let core = CciCore::new();
        assert_eq!(core.name(), "cci_core");
        assert_eq!(core.warmup_periods(&CciParams::default()), 25);
    }

    #[test]
    fn flat_input_yields_zero_cci() {
        let core = CciCore::new();
        let series = make_series((0..30).map(|_| 100.0).collect());
        let out = core.compute(&series, CciParams::default()).unwrap();
        // 完全 flat → MD=0 → fallback value=0
        for p in &out.series[19..] {
            assert!((p.value).abs() < 1e-6);
        }
    }

    #[test]
    fn zero_cross_event_fires() {
        let core = CciCore::new();
        // 構造一段 20 個低點 + 10 個高點,CCI 從負穿越 0
        let mut closes = vec![100.0; 20];
        closes.extend(vec![120.0; 10]);
        let series = make_series(closes);
        let out = core.compute(&series, CciParams::default()).unwrap();
        let zero_pos = out
            .events
            .iter()
            .filter(|e| e.kind == CciEventKind::ZeroCrossPositive)
            .count();
        assert!(zero_pos >= 1, "should detect at least one zero cross positive");
    }
}
