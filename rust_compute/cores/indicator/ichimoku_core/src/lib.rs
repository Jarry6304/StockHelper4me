// ichimoku_core(P3)— 對齊 m3Spec/indicator_cores_momentum.md §八
// Params §8.2 / warmup §8.3 / Output §8.4 / Fact §8.5
//
// Reference:
//   Goichi Hosoda (細田悟一,筆名 Ichimoku Sanjin),1969 年集大成出版
//     《Ichimoku Kinkō Hyō》
//   tenkan_period=9 / kijun_period=26 / senkou_b_period=52 / displacement=26:
//     Hosoda 原版常數,對應日本市場 1 週 + 2 週 + 1 個月語意
//
// 一目均衡表五條線:
//   Tenkan-sen   = (max(high,9) + min(low,9)) / 2          ; 轉換線
//   Kijun-sen    = (max(high,26) + min(low,26)) / 2        ; 基準線
//   Senkou A     = (Tenkan + Kijun) / 2,向前 +26 displaced ; 先行帶 A
//   Senkou B     = (max(high,52) + min(low,52)) / 2,向前 +26 displaced ; 先行帶 B
//   Chikou       = close,向後 −26 displaced                ; 遅行線

use anyhow::Result;
use chrono::NaiveDate;
use fact_schema::{Fact, IndicatorCore, Timeframe};
use ohlcv_loader::{OhlcvBar, OhlcvSeries};
use serde::Serialize;
use serde_json::json;

inventory::submit! {
    core_registry::CoreRegistration::new(
        "ichimoku_core", "0.1.0", core_registry::CoreKind::Indicator, "P3",
        "Ichimoku Core(一目均衡表 5 線 + Kumo 雲)",
    )
}

#[derive(Debug, Clone, Serialize)]
pub struct IchimokuParams {
    pub tenkan_period: usize,
    pub kijun_period: usize,
    pub senkou_b_period: usize,
    pub displacement: usize,
    pub timeframe: Timeframe,
}
impl Default for IchimokuParams {
    fn default() -> Self {
        Self {
            tenkan_period: 9,
            kijun_period: 26,
            senkou_b_period: 52,
            displacement: 26,
            timeframe: Timeframe::Daily,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum CloudColor {
    Bullish,
    Bearish,
    Neutral,
}

#[derive(Debug, Clone, Serialize)]
pub struct IchimokuOutput {
    pub stock_id: String,
    pub timeframe: Timeframe,
    pub series: Vec<IchimokuPoint>,
    #[serde(skip)]
    pub events: Vec<IchimokuEvent>,
}
#[derive(Debug, Clone, Serialize)]
pub struct IchimokuPoint {
    pub date: NaiveDate,
    pub tenkan: f64,
    pub kijun: f64,
    pub senkou_a: f64,
    pub senkou_b: f64,
    pub chikou: f64,
    pub cloud_color: CloudColor,
}
#[derive(Debug, Clone, Serialize)]
pub struct IchimokuEvent {
    pub date: NaiveDate,
    pub kind: IchimokuEventKind,
    pub metadata: serde_json::Value,
}
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum IchimokuEventKind {
    TkBullishCross,
    TkBearishCross,
    CloudBreakoutAbove,
    CloudBreakoutBelow,
    KumoTwist,
}

pub struct IchimokuCore;
impl IchimokuCore {
    pub fn new() -> Self {
        IchimokuCore
    }
}
impl Default for IchimokuCore {
    fn default() -> Self {
        IchimokuCore::new()
    }
}

fn rolling_midpoint(bars: &[OhlcvBar], period: usize) -> Vec<f64> {
    let n = bars.len();
    let mut out = vec![0.0; n];
    for i in 0..n {
        if i + 1 < period {
            // 暖機階段以當前 bar HL mid 作 placeholder(NULL-safe)
            out[i] = (bars[i].high + bars[i].low) / 2.0;
            continue;
        }
        let start = i + 1 - period;
        let hi = bars[start..=i].iter().map(|b| b.high).fold(f64::NEG_INFINITY, f64::max);
        let lo = bars[start..=i].iter().map(|b| b.low).fold(f64::INFINITY, f64::min);
        out[i] = (hi + lo) / 2.0;
    }
    out
}

impl IndicatorCore for IchimokuCore {
    type Input = OhlcvSeries;
    type Params = IchimokuParams;
    type Output = IchimokuOutput;
    fn name(&self) -> &'static str {
        "ichimoku_core"
    }
    fn version(&self) -> &'static str {
        "0.1.0"
    }
    fn warmup_periods(&self, params: &Self::Params) -> usize {
        params.senkou_b_period + params.displacement + 10
    }

    fn compute(&self, input: &Self::Input, params: Self::Params) -> Result<Self::Output> {
        let n = input.bars.len();
        let tenkan = rolling_midpoint(&input.bars, params.tenkan_period);
        let kijun = rolling_midpoint(&input.bars, params.kijun_period);
        let senkou_b_raw = rolling_midpoint(&input.bars, params.senkou_b_period);
        // senkou_a_raw = (tenkan + kijun) / 2
        let senkou_a_raw: Vec<f64> = (0..n).map(|i| (tenkan[i] + kijun[i]) / 2.0).collect();
        // displacement:senkou_a / senkou_b 在 +displacement 個 bar 處顯示(我們 align 到 date i:
        // 用 raw[i-displacement] 表示「在 i 這天 visible 的 senkou」,需要 i >= displacement)
        let d = params.displacement;
        let mut series = Vec::with_capacity(n);
        for i in 0..n {
            let sa = if i >= d { senkou_a_raw[i - d] } else { senkou_a_raw[i] };
            let sb = if i >= d { senkou_b_raw[i - d] } else { senkou_b_raw[i] };
            let chikou = if i + d < n {
                input.bars[i + d].close
            } else {
                input.bars[i].close
            };
            let cloud_color = if sa > sb {
                CloudColor::Bullish
            } else if sa < sb {
                CloudColor::Bearish
            } else {
                CloudColor::Neutral
            };
            series.push(IchimokuPoint {
                date: input.bars[i].date,
                tenkan: tenkan[i],
                kijun: kijun[i],
                senkou_a: sa,
                senkou_b: sb,
                chikou,
                cloud_color,
            });
        }

        let mut events = Vec::new();
        let warmup = self.warmup_periods(&params).min(n);
        for i in warmup..n {
            let p = &series[i - 1];
            let c = &series[i];
            // tenkan / kijun cross
            if p.tenkan <= p.kijun && c.tenkan > c.kijun {
                events.push(IchimokuEvent {
                    date: c.date,
                    kind: IchimokuEventKind::TkBullishCross,
                    metadata: json!({"event": "tk_bullish_cross", "tenkan": c.tenkan, "kijun": c.kijun}),
                });
            } else if p.tenkan >= p.kijun && c.tenkan < c.kijun {
                events.push(IchimokuEvent {
                    date: c.date,
                    kind: IchimokuEventKind::TkBearishCross,
                    metadata: json!({"event": "tk_bearish_cross", "tenkan": c.tenkan, "kijun": c.kijun}),
                });
            }
            // cloud breakout — close 從雲內 / 雲下 突破到雲上(對齊 §8.5 "cloud_breakout")
            let close = input.bars[i].close;
            let prev_close = input.bars[i - 1].close;
            let upper = c.senkou_a.max(c.senkou_b);
            let lower = c.senkou_a.min(c.senkou_b);
            let prev_upper = p.senkou_a.max(p.senkou_b);
            let prev_lower = p.senkou_a.min(p.senkou_b);
            if prev_close <= prev_upper && close > upper {
                events.push(IchimokuEvent {
                    date: c.date,
                    kind: IchimokuEventKind::CloudBreakoutAbove,
                    metadata: json!({"event": "cloud_breakout", "direction": "above", "close": close, "cloud_top": upper}),
                });
            } else if prev_close >= prev_lower && close < lower {
                events.push(IchimokuEvent {
                    date: c.date,
                    kind: IchimokuEventKind::CloudBreakoutBelow,
                    metadata: json!({"event": "cloud_breakout", "direction": "below", "close": close, "cloud_bottom": lower}),
                });
            }
            // kumo twist — cloud color 翻轉
            if p.cloud_color != c.cloud_color && c.cloud_color != CloudColor::Neutral {
                events.push(IchimokuEvent {
                    date: c.date,
                    kind: IchimokuEventKind::KumoTwist,
                    metadata: json!({"event": "kumo_twist", "to_color": format!("{:?}", c.cloud_color)}),
                });
            }
        }

        Ok(IchimokuOutput {
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
                source_core: "ichimoku_core".to_string(),
                source_version: "0.1.0".to_string(),
                params_hash: None,
                statement: format!("Ichimoku {:?} on {}", e.kind, e.date),
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

    fn make_series(bars: Vec<(f64, f64, f64)>) -> OhlcvSeries {
        let bars = bars
            .into_iter()
            .enumerate()
            .map(|(i, (h, l, c))| OhlcvBar {
                date: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap()
                    + chrono::Duration::days(i as i64),
                open: c,
                high: h,
                low: l,
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
        let core = IchimokuCore::new();
        assert_eq!(core.name(), "ichimoku_core");
        // senkou_b=52 + displacement=26 + 10 = 88
        assert_eq!(core.warmup_periods(&IchimokuParams::default()), 88);
    }

    #[test]
    fn flat_input_yields_no_events() {
        let core = IchimokuCore::new();
        let series = make_series((0..120).map(|_| (100.0, 100.0, 100.0)).collect());
        let out = core.compute(&series, IchimokuParams::default()).unwrap();
        // 完全 flat → 無 cross / 無 breakout / 無 twist
        assert!(out.events.is_empty(), "flat input should yield no events");
    }
}
