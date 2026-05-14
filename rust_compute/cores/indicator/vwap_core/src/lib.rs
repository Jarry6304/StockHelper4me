// vwap_core(P3)— 對齊 m3Spec/indicator_cores_volume.md §四
// Params §4.3 / warmup §4.4 / Output §4.5 / Fact §4.6
//
// Reference:
//   Berkowitz, Logue, Noser (1988), "The Total Cost of Transactions on the NYSE",
//     Journal of Finance — VWAP 原作為機構交易成本基準
//   Anchored VWAP:Brian Shannon (2008) "Technical Analysis Using Multiple Timeframes"
//     首推結構性 anchor 從重要事件日起算
//
// Anchored VWAP:
//   typical_price = (high + low + close) / 3   ; default Hlc3
//   cum_pv = Σ (typical_price × volume) since anchor_date
//   cum_v  = Σ volume since anchor_date
//   vwap   = cum_pv / cum_v
//   σ_band = sqrt(Σ (typical_price - vwap)² × volume / cum_v)  ; running stdev around vwap

use anyhow::{anyhow, Result};
use chrono::NaiveDate;
use fact_schema::{Fact, IndicatorCore, Timeframe};
use ohlcv_loader::OhlcvSeries;
use serde::Serialize;
use serde_json::json;

inventory::submit! {
    core_registry::CoreRegistration::new(
        "vwap_core", "0.1.0", core_registry::CoreKind::Indicator, "P3",
        "VWAP Core(Anchored Volume-Weighted Average Price)",
    )
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum VwapMode {
    Anchored,
    Session,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum PriceSource {
    Close,
    Hlc3,
}

#[derive(Debug, Clone, Serialize)]
pub struct VwapParams {
    pub mode: VwapMode,
    pub anchor_date: Option<NaiveDate>,
    pub source: PriceSource,
    pub timeframe: Timeframe,
}
impl Default for VwapParams {
    fn default() -> Self {
        Self {
            mode: VwapMode::Anchored,
            anchor_date: None,
            source: PriceSource::Hlc3,
            timeframe: Timeframe::Daily,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct VwapOutput {
    pub stock_id: String,
    pub timeframe: Timeframe,
    pub anchor_date: NaiveDate,
    pub series: Vec<VwapPoint>,
    #[serde(skip)]
    pub events: Vec<VwapEvent>,
}
#[derive(Debug, Clone, Serialize)]
pub struct VwapPoint {
    pub date: NaiveDate,
    pub vwap: f64,
    pub upper_band_1sd: f64,
    pub upper_band_2sd: f64,
    pub lower_band_1sd: f64,
    pub lower_band_2sd: f64,
}
#[derive(Debug, Clone, Serialize)]
pub struct VwapEvent {
    pub date: NaiveDate,
    pub kind: VwapEventKind,
    pub metadata: serde_json::Value,
}
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum VwapEventKind {
    BullishCross,
    BearishCross,
    Upper2sdTouch,
    Lower2sdTouch,
}

pub struct VwapCore;
impl VwapCore {
    pub fn new() -> Self {
        VwapCore
    }
}
impl Default for VwapCore {
    fn default() -> Self {
        VwapCore::new()
    }
}

impl IndicatorCore for VwapCore {
    type Input = OhlcvSeries;
    type Params = VwapParams;
    type Output = VwapOutput;
    fn name(&self) -> &'static str {
        "vwap_core"
    }
    fn version(&self) -> &'static str {
        "0.1.0"
    }
    fn warmup_periods(&self, _params: &Self::Params) -> usize {
        0
    }

    fn compute(&self, input: &Self::Input, params: Self::Params) -> Result<Self::Output> {
        if params.mode != VwapMode::Anchored {
            return Err(anyhow!(
                "vwap_core v2.0 只支援 Anchored 模式;Session 模式留 P3 後續"
            ));
        }
        let anchor = params
            .anchor_date
            .ok_or_else(|| anyhow!("VwapParams.anchor_date 必填(Anchored 模式)"))?;

        let n = input.bars.len();
        let mut series = Vec::with_capacity(n);
        let mut cum_pv = 0.0;
        let mut cum_v = 0.0;
        let mut cum_p2v = 0.0;

        for i in 0..n {
            let b = &input.bars[i];
            let tp = match params.source {
                PriceSource::Close => b.close,
                PriceSource::Hlc3 => (b.high + b.low + b.close) / 3.0,
            };
            let vol = b.volume.unwrap_or(0).max(0) as f64;

            // 錨點之前:vwap = NaN-ish placeholder(用 close 對齊保持 series 完整)
            if b.date < anchor {
                series.push(VwapPoint {
                    date: b.date,
                    vwap: tp,
                    upper_band_1sd: tp,
                    upper_band_2sd: tp,
                    lower_band_1sd: tp,
                    lower_band_2sd: tp,
                });
                continue;
            }
            cum_pv += tp * vol;
            cum_v += vol;
            cum_p2v += tp * tp * vol;
            let vwap = if cum_v > 0.0 { cum_pv / cum_v } else { tp };
            // running variance:E[X²] − (E[X])²
            let var = if cum_v > 0.0 {
                (cum_p2v / cum_v - vwap * vwap).max(0.0)
            } else {
                0.0
            };
            let sd = var.sqrt();
            series.push(VwapPoint {
                date: b.date,
                vwap,
                upper_band_1sd: vwap + sd,
                upper_band_2sd: vwap + 2.0 * sd,
                lower_band_1sd: vwap - sd,
                lower_band_2sd: vwap - 2.0 * sd,
            });
        }

        // events
        let mut events = Vec::new();
        let mut touched_upper_2sd = false;
        let mut touched_lower_2sd = false;
        for i in 1..n {
            if input.bars[i].date < anchor {
                continue;
            }
            let prev_close = input.bars[i - 1].close;
            let cur_close = input.bars[i].close;
            let vwap_prev = series[i - 1].vwap;
            let vwap_cur = series[i].vwap;
            // bullish cross
            if prev_close <= vwap_prev && cur_close > vwap_cur {
                events.push(VwapEvent {
                    date: series[i].date,
                    kind: VwapEventKind::BullishCross,
                    metadata: json!({"event": "vwap_bullish_cross", "anchor": anchor.to_string(), "vwap": vwap_cur}),
                });
            } else if prev_close >= vwap_prev && cur_close < vwap_cur {
                events.push(VwapEvent {
                    date: series[i].date,
                    kind: VwapEventKind::BearishCross,
                    metadata: json!({"event": "vwap_bearish_cross", "anchor": anchor.to_string(), "vwap": vwap_cur}),
                });
            }
            // 2σ touch — edge trigger 只在首次觸及紀錄,後續離開 1σ 後重置
            let high = input.bars[i].high;
            let low = input.bars[i].low;
            if high >= series[i].upper_band_2sd && !touched_upper_2sd {
                events.push(VwapEvent {
                    date: series[i].date,
                    kind: VwapEventKind::Upper2sdTouch,
                    metadata: json!({"event": "vwap_2sd_upper_touch", "anchor": anchor.to_string()}),
                });
                touched_upper_2sd = true;
            } else if cur_close < series[i].upper_band_1sd {
                touched_upper_2sd = false;
            }
            if low <= series[i].lower_band_2sd && !touched_lower_2sd {
                events.push(VwapEvent {
                    date: series[i].date,
                    kind: VwapEventKind::Lower2sdTouch,
                    metadata: json!({"event": "vwap_2sd_lower_touch", "anchor": anchor.to_string()}),
                });
                touched_lower_2sd = true;
            } else if cur_close > series[i].lower_band_1sd {
                touched_lower_2sd = false;
            }
        }

        Ok(VwapOutput {
            stock_id: input.stock_id.clone(),
            timeframe: params.timeframe,
            anchor_date: anchor,
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
                source_core: "vwap_core".to_string(),
                source_version: "0.1.0".to_string(),
                params_hash: None,
                statement: format!("VWAP {:?} on {}", e.kind, e.date),
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

    fn make_series(closes: Vec<f64>, vol: i64) -> OhlcvSeries {
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
                volume: Some(vol),
            })
            .collect();
        OhlcvSeries {
            stock_id: "TEST".to_string(),
            timeframe: Timeframe::Daily,
            bars,
        }
    }

    #[test]
    fn requires_anchor_date() {
        let core = VwapCore::new();
        let series = make_series((0..10).map(|i| 100.0 + i as f64).collect(), 1000);
        let result = core.compute(&series, VwapParams::default());
        assert!(result.is_err(), "missing anchor_date should be Err");
    }

    #[test]
    fn flat_input_vwap_equals_price() {
        let core = VwapCore::new();
        let series = make_series((0..10).map(|_| 100.0).collect(), 1000);
        let params = VwapParams {
            anchor_date: Some(NaiveDate::from_ymd_opt(2026, 1, 1).unwrap()),
            source: PriceSource::Close,
            ..Default::default()
        };
        let out = core.compute(&series, params).unwrap();
        for p in &out.series {
            assert!((p.vwap - 100.0).abs() < 1e-9);
            assert!((p.upper_band_1sd - 100.0).abs() < 1e-6);
        }
    }
}
