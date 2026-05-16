#![allow(clippy::needless_range_loop)]
// mfi_core(P3)— 對齊 m3Spec/indicator_cores_volume.md §五
// Params §5.2 / warmup §5.3 / Output §5.4 / Fact §5.5
//
// Reference:
//   Quong & Soudack (1989), "Volume-Weighted RSI: Money Flow", Stocks & Commodities
//     原作為 RSI 量加權變體
//   period=14 / overbought=80 / oversold=20:Quong & Soudack 原版
//   Murphy (1999) p.262「MFI 比 RSI 更靈敏,故閾值用 80/20 而非 70/30」
//
// MFI 公式(0–100):
//   TP = (high + low + close) / 3
//   raw_money_flow = TP × volume
//   positive_mf = sum(raw_money_flow where TP_today > TP_yesterday)
//   negative_mf = sum(raw_money_flow where TP_today < TP_yesterday)
//   money_ratio = positive_mf / negative_mf
//   MFI = 100 - 100 / (1 + money_ratio)
//
// Divergence:對齊 rsi_core / obv_core 同款 pivot-based(PIVOT_N=3, MIN_PIVOT_DIST=12)

use anyhow::Result;
use chrono::NaiveDate;
use fact_schema::{Fact, IndicatorCore, Timeframe};
use ohlcv_loader::OhlcvSeries;
use serde::Serialize;
use serde_json::json;

inventory::submit! {
    core_registry::CoreRegistration::new(
        "mfi_core", "0.1.0", core_registry::CoreKind::Indicator, "P3",
        "MFI Core(Money Flow Index,量加權 RSI 變體)",
    )
}

const STREAK_MIN_DAYS: usize = 3;
const PIVOT_N: usize = 3;
const MIN_PIVOT_DIST: usize = 12;

#[derive(Debug, Clone, Serialize)]
pub struct MfiParams {
    pub period: usize,
    pub overbought: f64,
    pub oversold: f64,
    pub timeframe: Timeframe,
}
impl Default for MfiParams {
    fn default() -> Self {
        Self {
            period: 14,
            overbought: 80.0,
            oversold: 20.0,
            timeframe: Timeframe::Daily,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct MfiOutput {
    pub stock_id: String,
    pub timeframe: Timeframe,
    pub series: Vec<MfiPoint>,
    #[serde(skip)]
    pub events: Vec<MfiEvent>,
}
#[derive(Debug, Clone, Serialize)]
pub struct MfiPoint {
    pub date: NaiveDate,
    pub value: f64,
}
#[derive(Debug, Clone, Serialize)]
pub struct MfiEvent {
    pub date: NaiveDate,
    pub kind: MfiEventKind,
    pub value: f64,
    pub metadata: serde_json::Value,
}
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum MfiEventKind {
    OverboughtStreak,
    OversoldStreak,
    BullishDivergence,
    BearishDivergence,
}

pub struct MfiCore;
impl MfiCore {
    pub fn new() -> Self {
        MfiCore
    }
}
impl Default for MfiCore {
    fn default() -> Self {
        MfiCore::new()
    }
}

fn find_swing_highs(values: &[f64]) -> Vec<usize> {
    let n = values.len();
    let mut highs = Vec::new();
    if n < 2 * PIVOT_N + 1 {
        return highs;
    }
    for i in PIVOT_N..n - PIVOT_N {
        if values[i].abs() < 1e-12 {
            continue;
        }
        let mut is_high = true;
        for j in 1..=PIVOT_N {
            if values[i - j] >= values[i] || values[i + j] >= values[i] {
                is_high = false;
                break;
            }
        }
        if is_high {
            highs.push(i);
        }
    }
    highs
}

fn find_swing_lows(values: &[f64]) -> Vec<usize> {
    let n = values.len();
    let mut lows = Vec::new();
    if n < 2 * PIVOT_N + 1 {
        return lows;
    }
    for i in PIVOT_N..n - PIVOT_N {
        if values[i].abs() < 1e-12 {
            continue;
        }
        let mut is_low = true;
        for j in 1..=PIVOT_N {
            if values[i - j] <= values[i] || values[i + j] <= values[i] {
                is_low = false;
                break;
            }
        }
        if is_low {
            lows.push(i);
        }
    }
    lows
}

impl IndicatorCore for MfiCore {
    type Input = OhlcvSeries;
    type Params = MfiParams;
    type Output = MfiOutput;
    fn name(&self) -> &'static str {
        "mfi_core"
    }
    fn version(&self) -> &'static str {
        "0.1.0"
    }
    fn warmup_periods(&self, params: &Self::Params) -> usize {
        params.period * 2 + 5
    }

    fn compute(&self, input: &Self::Input, params: Self::Params) -> Result<Self::Output> {
        let n = input.bars.len();
        let p = params.period;
        let tp: Vec<f64> = input
            .bars
            .iter()
            .map(|b| (b.high + b.low + b.close) / 3.0)
            .collect();
        let mf: Vec<f64> = (0..n)
            .map(|i| tp[i] * input.bars[i].volume.unwrap_or(0).max(0) as f64)
            .collect();

        let mut series = Vec::with_capacity(n);
        for i in 0..n {
            if i < p {
                series.push(MfiPoint {
                    date: input.bars[i].date,
                    value: 0.0,
                });
                continue;
            }
            let mut pos_mf = 0.0;
            let mut neg_mf = 0.0;
            for j in (i - p + 1)..=i {
                if j == 0 {
                    continue;
                }
                if tp[j] > tp[j - 1] {
                    pos_mf += mf[j];
                } else if tp[j] < tp[j - 1] {
                    neg_mf += mf[j];
                }
            }
            let value = if neg_mf > 0.0 {
                let ratio = pos_mf / neg_mf;
                100.0 - 100.0 / (1.0 + ratio)
            } else if pos_mf > 0.0 {
                100.0
            } else {
                50.0
            };
            series.push(MfiPoint {
                date: input.bars[i].date,
                value,
            });
        }

        let mut events = Vec::new();
        let mut over_run = 0usize;
        let mut under_run = 0usize;
        for i in p..n {
            let cur = series[i].value;
            if cur > params.overbought {
                over_run += 1;
                if over_run == STREAK_MIN_DAYS {
                    events.push(MfiEvent {
                        date: series[i].date,
                        kind: MfiEventKind::OverboughtStreak,
                        value: cur,
                        metadata: json!({"event": "overbought_streak", "days": STREAK_MIN_DAYS, "threshold": params.overbought}),
                    });
                }
            } else {
                over_run = 0;
            }
            if cur < params.oversold {
                under_run += 1;
                if under_run == STREAK_MIN_DAYS {
                    events.push(MfiEvent {
                        date: series[i].date,
                        kind: MfiEventKind::OversoldStreak,
                        value: cur,
                        metadata: json!({"event": "oversold_streak", "days": STREAK_MIN_DAYS, "threshold": params.oversold}),
                    });
                }
            } else {
                under_run = 0;
            }
        }

        // Divergence detection — pivot-based(對齊 rsi/kd/macd/obv 同款)
        let values: Vec<f64> = series.iter().map(|p| p.value).collect();
        let price_highs = find_swing_highs(&tp);
        let price_lows = find_swing_lows(&tp);
        // bearish:price HH + MFI LH
        for k in 1..price_highs.len() {
            let p1 = price_highs[k - 1];
            let p2 = price_highs[k];
            if p2 - p1 < MIN_PIVOT_DIST {
                continue;
            }
            if tp[p2] > tp[p1] && values[p2] < values[p1] && values[p1] > 0.0 {
                let confirm_date = series[(p2 + PIVOT_N).min(n - 1)].date;
                events.push(MfiEvent {
                    date: confirm_date,
                    kind: MfiEventKind::BearishDivergence,
                    value: values[p2],
                    metadata: json!({
                        "event": "bearish_divergence",
                        "price_date_1": series[p1].date.to_string(),
                        "price_date_2": series[p2].date.to_string(),
                        "price_1": tp[p1],
                        "price_2": tp[p2],
                        "mfi_1": values[p1],
                        "mfi_2": values[p2],
                    }),
                });
            }
        }
        // bullish:price LL + MFI HL
        for k in 1..price_lows.len() {
            let p1 = price_lows[k - 1];
            let p2 = price_lows[k];
            if p2 - p1 < MIN_PIVOT_DIST {
                continue;
            }
            if tp[p2] < tp[p1] && values[p2] > values[p1] && values[p1] > 0.0 {
                let confirm_date = series[(p2 + PIVOT_N).min(n - 1)].date;
                events.push(MfiEvent {
                    date: confirm_date,
                    kind: MfiEventKind::BullishDivergence,
                    value: values[p2],
                    metadata: json!({
                        "event": "bullish_divergence",
                        "price_date_1": series[p1].date.to_string(),
                        "price_date_2": series[p2].date.to_string(),
                        "price_1": tp[p1],
                        "price_2": tp[p2],
                        "mfi_1": values[p1],
                        "mfi_2": values[p2],
                    }),
                });
            }
        }

        Ok(MfiOutput {
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
                source_core: "mfi_core".to_string(),
                source_version: "0.1.0".to_string(),
                params_hash: None,
                statement: format!("MFI {:?} on {}: value={:.2}", e.kind, e.date, e.value),
                metadata: fact_schema::with_event_kind(e.metadata.clone(), &e.kind),
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

    fn make_series(prices: Vec<f64>, vols: Vec<i64>) -> OhlcvSeries {
        assert_eq!(prices.len(), vols.len());
        let bars = prices
            .into_iter()
            .zip(vols)
            .enumerate()
            .map(|(i, (p, v))| OhlcvBar {
                date: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap()
                    + chrono::Duration::days(i as i64),
                open: p,
                high: p,
                low: p,
                close: p,
                volume: Some(v),
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
        let core = MfiCore::new();
        assert_eq!(core.name(), "mfi_core");
        assert_eq!(core.warmup_periods(&MfiParams::default()), 33);
    }

    #[test]
    fn rising_prices_yield_high_mfi() {
        let core = MfiCore::new();
        let prices: Vec<f64> = (0..30).map(|i| 100.0 + i as f64).collect();
        let vols: Vec<i64> = (0..30).map(|_| 1000).collect();
        let series = make_series(prices, vols);
        let out = core.compute(&series, MfiParams::default()).unwrap();
        // 持續上漲 → MFI 應持續 100(no negative MF)
        assert!(out.series[20].value > 90.0, "rising should yield MFI > 90");
    }
}
