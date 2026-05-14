#![allow(clippy::needless_range_loop)]
// coppock_core(P3)— 對齊 m3Spec/indicator_cores_momentum.md §十一
// Params §11.2 / warmup §11.3 / Output §11.4 / Fact §11.5
//
// Reference:
//   Edwin Coppock (1962), "Practical Relative Strength Charting",
//     Barron's National Business and Financial Weekly
//   原作為月線指標,(14, 11, 10) 為原版參數;Workflow toml 預設 timeframe=Monthly
//
// Coppock 公式:
//   ROC(n)_t = (close_t - close_{t-n}) / close_{t-n} × 100
//   Coppock_t = WMA(roc_long + roc_short, wma_period)
//
// 月線預期觸發率:zero_cross 平均 ~每 5-10 年 1 次(主升段起點)

use anyhow::Result;
use chrono::NaiveDate;
use fact_schema::{Fact, IndicatorCore, Timeframe};
use ohlcv_loader::OhlcvSeries;
use serde::Serialize;
use serde_json::json;

inventory::submit! {
    core_registry::CoreRegistration::new(
        "coppock_core", "0.1.0", core_registry::CoreKind::Indicator, "P3",
        "Coppock Curve Core(長期動能指標,月線主升段起點)",
    )
}

#[derive(Debug, Clone, Serialize)]
pub struct CoppockParams {
    pub roc_long: usize,
    pub roc_short: usize,
    pub wma_period: usize,
    pub timeframe: Timeframe,
}
impl Default for CoppockParams {
    fn default() -> Self {
        Self {
            roc_long: 14,
            roc_short: 11,
            wma_period: 10,
            timeframe: Timeframe::Monthly,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CoppockOutput {
    pub stock_id: String,
    pub timeframe: Timeframe,
    pub series: Vec<CoppockPoint>,
    #[serde(skip)]
    pub events: Vec<CoppockEvent>,
}
#[derive(Debug, Clone, Serialize)]
pub struct CoppockPoint {
    pub date: NaiveDate,
    pub value: f64,
}
#[derive(Debug, Clone, Serialize)]
pub struct CoppockEvent {
    pub date: NaiveDate,
    pub kind: CoppockEventKind,
    pub value: f64,
    pub metadata: serde_json::Value,
}
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum CoppockEventKind {
    ZeroCrossPositive,
    ZeroCrossNegative,
    Trough,
}

pub struct CoppockCore;
impl CoppockCore {
    pub fn new() -> Self {
        CoppockCore
    }
}
impl Default for CoppockCore {
    fn default() -> Self {
        CoppockCore::new()
    }
}

fn wma(values: &[f64], period: usize) -> Vec<f64> {
    let n = values.len();
    let mut out = vec![0.0; n];
    if period == 0 || n == 0 {
        return out;
    }
    let denom: f64 = (1..=period).map(|x| x as f64).sum();
    for i in 0..n {
        if i + 1 < period {
            continue;
        }
        let mut num = 0.0;
        for k in 0..period {
            num += values[i - k] * (period - k) as f64;
        }
        out[i] = num / denom;
    }
    out
}

impl IndicatorCore for CoppockCore {
    type Input = OhlcvSeries;
    type Params = CoppockParams;
    type Output = CoppockOutput;
    fn name(&self) -> &'static str {
        "coppock_core"
    }
    fn version(&self) -> &'static str {
        "0.1.0"
    }
    fn warmup_periods(&self, params: &Self::Params) -> usize {
        params.roc_long.max(params.roc_short) + params.wma_period + 5
    }

    fn compute(&self, input: &Self::Input, params: Self::Params) -> Result<Self::Output> {
        let n = input.bars.len();
        let closes: Vec<f64> = input.bars.iter().map(|b| b.close).collect();
        let roc = |period: usize, i: usize| -> f64 {
            if i < period || closes[i - period] == 0.0 {
                0.0
            } else {
                (closes[i] - closes[i - period]) / closes[i - period] * 100.0
            }
        };
        // sum of ROC long + ROC short, 然後 WMA
        let combined: Vec<f64> = (0..n)
            .map(|i| {
                if i < params.roc_long.max(params.roc_short) {
                    0.0
                } else {
                    roc(params.roc_long, i) + roc(params.roc_short, i)
                }
            })
            .collect();
        let copp = wma(&combined, params.wma_period);
        let mut series = Vec::with_capacity(n);
        for i in 0..n {
            series.push(CoppockPoint {
                date: input.bars[i].date,
                value: copp[i],
            });
        }

        let mut events = Vec::new();
        let warmup = self.warmup_periods(&params).min(n);
        for i in warmup..n {
            let prev = series[i - 1].value;
            let cur = series[i].value;
            // zero line cross
            if prev <= 0.0 && cur > 0.0 {
                events.push(CoppockEvent {
                    date: series[i].date,
                    kind: CoppockEventKind::ZeroCrossPositive,
                    value: cur,
                    metadata: json!({"event": "zero_cross_positive", "value": cur}),
                });
            } else if prev >= 0.0 && cur < 0.0 {
                events.push(CoppockEvent {
                    date: series[i].date,
                    kind: CoppockEventKind::ZeroCrossNegative,
                    value: cur,
                    metadata: json!({"event": "zero_cross_negative", "value": cur}),
                });
            }
            // trough detection — local min(window 3)& value < 0
            if i + 1 < n {
                let next = series[i + 1].value;
                if cur < prev && cur < next && cur < 0.0 {
                    events.push(CoppockEvent {
                        date: series[i].date,
                        kind: CoppockEventKind::Trough,
                        value: cur,
                        metadata: json!({"event": "trough", "value": cur}),
                    });
                }
            }
        }

        Ok(CoppockOutput {
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
                source_core: "coppock_core".to_string(),
                source_version: "0.1.0".to_string(),
                params_hash: None,
                statement: format!("Coppock {:?} on {}: value={:.2}", e.kind, e.date, e.value),
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
            timeframe: Timeframe::Monthly,
            bars,
        }
    }

    #[test]
    fn name_and_warmup() {
        let core = CoppockCore::new();
        assert_eq!(core.name(), "coppock_core");
        // max(14, 11) + 10 + 5 = 29
        assert_eq!(core.warmup_periods(&CoppockParams::default()), 29);
    }

    #[test]
    fn rising_input_yields_positive_coppock() {
        let core = CoppockCore::new();
        // 40 個遞增 close
        let series = make_series((0..40).map(|i| 100.0 + i as f64).collect());
        let out = core.compute(&series, CoppockParams::default()).unwrap();
        // 暖機後應為正值(持續上漲)
        assert!(out.series[35].value > 0.0, "rising prices should yield positive coppock");
    }
}
