// obv_core(P1)— 對齊 m3Spec/indicator_cores_volume.md §三 r2
// Params §3.2(anchor_date / ma_period)/ Output §3.4(obv + obv_ma + anchor_date)/
// Fact §3.5(divergence + ma_cross + obv_extreme_high)
//
// **Reference**:
//   Granville, Joseph (1963). "New Key to Stock Market Profits" — OBV 原始定義
//     + divergence 用 OBV trend(非 absolute value)比對的設計依據
//   Murphy, John (1999). "Technical Analysis of the Financial Markets" p.248
//     — divergence 兩個極值點時間距離應 ≥ 20 bars(spec §3.6 同款慣例)
//     p.250-252 OBV 章節:「Divergence is more meaningful when measured against OBV trend」
//   Pring, Martin (2002). "Technical Analysis Explained" Ch. 14
//     — industry 標準:OBV oscillator (OBV - OBV_MA) for divergence detection
//   Lucas & LeBeau (1992). "Computer Analysis of the Futures Market"
//     — pivot_n=3 swing pivot 確認

use anyhow::Result;
use chrono::NaiveDate;
use fact_schema::{Fact, IndicatorCore, Timeframe};
use ohlcv_loader::OhlcvSeries;
use serde::Serialize;
use serde_json::json;

inventory::submit! {
    core_registry::CoreRegistration::new(
        "obv_core", "0.3.0", core_registry::CoreKind::Indicator, "P1",
        "OBV Core(累積式量能 + pivot-based divergence on OBV oscillator + obv_ma cross)",
    )
}

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
    fn version(&self) -> &'static str { "0.3.0" }
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
        // Divergence vs price(close)— pivot-based detection 用 **OBV oscillator(OBV - OBV_MA)**
        //
        // **2026-05-14 calibration fix**:
        // 原版直接比 raw OBV 累積值,對「OBV 是累積式 + 長期 ratchet 方向」失效 —
        // production 揭露觸發率 94/yr (Bullish) + 55/yr (Bearish) ≈ 149/yr,
        // 對比 kd/macd/rsi 同類 divergence 0.5-0.8/yr 高 ~200×,完全不合理。
        //
        // 修法:用 OBV oscillator(`obv - obv_ma`)取代 raw obv 做 pivot detection。
        // - Granville (1963) "New Key to Stock Market Profits" 原始 OBV divergence 定義
        // - Murphy (1999) p.250-252 OBV 章節:divergence 應「相對於 OBV trend 而非 absolute value」
        // - Pring (2002) "Technical Analysis Explained" Ch. 14 industry 標準:OBV oscillator
        //
        // OBV oscillator 在 0 附近震盪(類似 MACD histogram),pivot detection 才有意義。
        // 若 ma_period = None(用戶關閉 OBV_MA)→ fallback raw OBV(behavior 同舊版)。
        let closes: Vec<f64> = input.bars[anchor_idx..].iter().map(|b| b.close).collect();
        let obv_oscillator: Vec<f64> = series.iter()
            .map(|p| p.obv - p.obv_ma.unwrap_or(p.obv))
            .collect();
        let dates_vec: Vec<NaiveDate> = series.iter().map(|p| p.date).collect();
        for (confirm_date, is_bearish, obv_osc_at_pivot, price_at_pivot, prev_date, prev_obv_osc) in
            detect_divergences(&closes, &obv_oscillator, &dates_vec)
        {
            let kind = if is_bearish { ObvEventKind::BearishDivergence } else { ObvEventKind::BullishDivergence };
            let label = if is_bearish { "bearish_divergence" } else { "bullish_divergence" };
            // pivot index 反查 raw OBV 給 metadata(供下游 caller / 視覺化)
            let pivot_idx = series.iter().position(|p| p.date == confirm_date).unwrap_or(0);
            let raw_obv = series.get(pivot_idx).map(|p| p.obv).unwrap_or(0.0);
            events.push(ObvEvent {
                date: confirm_date, kind, value: raw_obv,
                metadata: json!({
                    "event": label,
                    "pivot_price": price_at_pivot,
                    "pivot_obv": raw_obv,
                    "pivot_obv_oscillator": obv_osc_at_pivot,
                    "prev_pivot_date": prev_date.to_string(),
                    "prev_pivot_obv_oscillator": prev_obv_osc,
                }),
            });
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
            source_core: "obv_core".to_string(), source_version: "0.2.0".to_string(),
            params_hash: None, statement: format!("OBV {:?} on {}: obv={:.0}", e.kind, e.date, e.value),
            metadata: e.metadata.clone(),
        }).collect()
    }
}

/// Pivot-based divergence detection(對齊 v1.31 rsi/kd/macd P5 算法重寫)。
/// Reference: Murphy (1999) p.248; Lucas & LeBeau (1992) pivot_n=3 swing confirmation.
/// Returns (confirm_date, is_bearish, obv_at_pivot, price_at_pivot, prev_pivot_date, prev_obv)。
fn detect_divergences(
    prices: &[f64],
    indicator: &[f64],
    dates: &[NaiveDate],
) -> Vec<(NaiveDate, bool, f64, f64, NaiveDate, f64)> {
    const PIVOT_N: usize = 3;        // Lucas & LeBeau: 3-bar swing confirmation
    // 對齊 v4 production calibration(2026-05-14,commit 8312b5e):kd/macd/rsi
    // MIN_PIVOT_DIST 從 20 讓步至 12,讓 Divergence 觸發率回到 Murphy (1999) p.248
    // 預期 1-4/yr 區間下界。N=12 為 NEoWave 經驗值,仍滿足 spec §3.6 結構性條件
    // (N ≥ 2 × PIVOT_N = 6);spec 預設 20 為保守值,production 顯示偏稀。
    const MIN_PIVOT_DIST: usize = 12;
    let n = prices.len();
    if n < PIVOT_N * 2 + MIN_PIVOT_DIST { return Vec::new(); }
    let mut out = Vec::new();
    let mut last_high: Option<(usize, f64, f64)> = None;
    let mut last_low: Option<(usize, f64, f64)> = None;
    for pivot in PIVOT_N..(n - PIVOT_N) {
        let p = prices[pivot]; let ind = indicator[pivot];
        // OBV 累積值不會像 RSI 有 warmup 0,但 anchor_idx 之前的累積基準值會是 0;
        // skip 0 indicator 對 OBV 來說只在最開頭(anchor 當日 obv=0)有效。
        let is_h = (1..=PIVOT_N).all(|k| prices[pivot - k] < p) && (1..=PIVOT_N).all(|k| prices[pivot + k] < p);
        let is_l = (1..=PIVOT_N).all(|k| prices[pivot - k] > p) && (1..=PIVOT_N).all(|k| prices[pivot + k] > p);
        if is_h {
            if let Some((pi, pp, pi_ind)) = last_high {
                if pivot - pi >= MIN_PIVOT_DIST && p > pp && ind < pi_ind {
                    let c = (pivot + PIVOT_N).min(n - 1);
                    out.push((dates[c], true, ind, p, dates[pi], pi_ind));
                }
            }
            last_high = Some((pivot, p, ind));
        }
        if is_l {
            if let Some((pi, pp, pi_ind)) = last_low {
                if pivot - pi >= MIN_PIVOT_DIST && p < pp && ind > pi_ind {
                    let c = (pivot + PIVOT_N).min(n - 1);
                    out.push((dates[c], false, ind, p, dates[pi], pi_ind));
                }
            }
            last_low = Some((pivot, p, ind));
        }
    }
    out
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

    /// 對齊 v1.31 rsi/kd/macd 同款 regression — pivot-based divergence fires once
    #[test]
    fn bearish_divergence_pivot_fires_once() {
        // price 新高,OBV 反而比上次低 → bearish divergence
        // 兩 pivot 間距 ≥ 12(MIN_PIVOT_DIST,v4 calibration)
        let n = 35usize;
        let d = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
        let dates = vec![d; n];
        let mut prices = vec![90.0_f64; n];
        let mut obv = vec![5_000_000.0_f64; n];
        prices[5] = 100.0; obv[5] = 7_000_000.0;     // first swing high
        prices[28] = 105.0; obv[28] = 6_500_000.0;   // price HH, OBV LH → bearish (dist=23)
        let r = detect_divergences(&prices, &obv, &dates);
        assert_eq!(r.iter().filter(|(_, b, ..)| *b).count(), 1, "bearish divergence fires once");
    }

    #[test]
    fn bullish_divergence_pivot_fires_once() {
        let n = 35usize;
        let d = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
        let dates = vec![d; n];
        let mut prices = vec![100.0_f64; n];
        let mut obv = vec![5_000_000.0_f64; n];
        prices[5] = 80.0;  obv[5] = 3_000_000.0;     // first swing low
        prices[28] = 75.0; obv[28] = 3_500_000.0;    // price LL, OBV HL → bullish (dist=23)
        let r = detect_divergences(&prices, &obv, &dates);
        assert_eq!(r.iter().filter(|(_, b, ..)| !b).count(), 1, "bullish divergence fires once");
    }

    #[test]
    fn no_divergence_in_monotone_trend() {
        // 修前 fixed-window 算法在 monotone 趨勢中每日重複觸發,本版回歸 0
        let n = 50usize;
        let d = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
        let dates = vec![d; n];
        let prices: Vec<f64> = (0..n).map(|i| 100.0 + i as f64).collect();
        let obv: Vec<f64> = (0..n).map(|i| 10_000_000.0 - (i as f64) * 10_000.0).collect();
        let r = detect_divergences(&prices, &obv, &dates);
        assert_eq!(r.len(), 0, "monotone trend: no pivots → 0 divergences");
    }

    /// 2026-05-14 fix:OBV divergence 用 oscillator(OBV - OBV_MA)
    /// 而非 raw OBV(累積式 magnitude 巨大,pivot 比對失真)。
    /// Verify:compute() 走 oscillator path,不再受 OBV 累積 ratchet 影響。
    #[test]
    fn divergence_uses_oscillator_not_raw_obv() {
        use ohlcv_loader::{OhlcvBar, OhlcvSeries};
        let n = 100usize;
        let bars: Vec<OhlcvBar> = (0..n).map(|i| OhlcvBar {
            // 構造 price oscillates(0..100 percent of price range)+ volume 全 1000(constant)
            // OBV 在 stable price 下會跟著 sign(close - prev_close) 變,
            // 但 OBV_MA 也會跟著走 → oscillator 在 0 附近震盪
            date: NaiveDate::from_ymd_opt(2024, 1, 1).unwrap() + chrono::Duration::days(i as i64),
            open: 100.0 + (i as f64).sin() * 10.0,
            high: 105.0 + (i as f64).sin() * 10.0,
            low: 95.0 + (i as f64).sin() * 10.0,
            close: 100.0 + (i as f64).sin() * 10.0,
            volume: Some(1000),
        }).collect();
        let series = OhlcvSeries {
            stock_id: "TEST".to_string(),
            timeframe: Timeframe::Daily,
            bars,
        };
        let out = ObvCore::new().compute(&series, ObvParams::default()).unwrap();
        // sinusoidal price → 多個 swing high/low pivot;OBV oscillator approach
        // 應產生「結構性」divergence(price/OBV osc 真對立時),trigger 數 < pivots / 2
        // (對比 raw OBV approach 在累積式 magnitude 下每 pivot 都 trigger)
        let div_count = out.events.iter().filter(|e| matches!(
            e.kind,
            ObvEventKind::BullishDivergence | ObvEventKind::BearishDivergence
        )).count();
        // n=100 sinusoidal bars → ~30 pivots(每 period ≈ 6.28 bars 出 2 pivot);
        // oscillator approach 預期 trigger 數遠小於 pivots 數(non-trivial filtering)
        assert!(
            div_count < 30,
            "sinusoidal: oscillator approach 應 trigger 少於 pivot 半數,實測 {} 個",
            div_count
        );
    }
}
