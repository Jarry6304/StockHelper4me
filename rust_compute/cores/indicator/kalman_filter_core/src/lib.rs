// kalman_filter_core(P3)— Indicator Core(對齊 m3Spec/kalman_filter_core.md)
//
// v3.33 multi-horizon refactor(2026-05-18):
//   原本 single Q=1e-5(~12 年 halflife)鎖住一個極端平滑 horizon,對非平穩急漲股
//   (如 3030 8 年 8 倍)smoothed_price 永遠落後 raw_close 數倍。改 multi-horizon:
//   同 OhlcvSeries 跑 4 個獨立 Kalman recursion(Q ∈ [1e-1, 1e-2, 1e-3, 1e-5]),
//   對應 ~31 / 99 / 310 / 3100 bars halflife = 月 / 季 / 年 / 長期均衡 horizon。
//   LLM 可看跨 horizon regime 一致性判斷訊號信心。
//
// 1-D 趨勢平滑(state-space)— 每個 horizon 獨立跑一次:
//   state x_t       = "true underlying price"(unobservable)
//   observation z_t = price_daily_fwd.close
//
//   Predict:  x_pred = x_prev;        P_pred = P_prev + Q
//   Update:   K = P_pred / (P_pred + R)
//             x = x_pred + K * (z - x_pred)
//             P = (1 - K) * P_pred
//
// Regime classification(5 類,對齊 user 拍版 2026-05-15):
//   StableUp / Accelerating / Sideway / Decelerating / StableDown
//
// EventKind:5 種 transition(regime 變更時觸發一次)。
//   **只從 primary horizon(預設 "medium")產 facts** — 保留 ≤ 12/yr/stock production
//   行為(對齊 v1.32 P2 acceptance);其他 horizon 訊號走 indicator_values JSONB,
//   LLM 自己讀 horizons array。
//
// **v3.33 Output schema**(`indicator_values.value`):
//   {
//     "stock_id": "3030",
//     "timeframe": "Daily",
//     "primary_horizon": "medium",
//     "series": [...KalmanPoint]   ← primary horizon full series,backward compat v3.30
//     "events": [...KalmanEvent]   ← primary horizon events(也是 facts source)
//     "horizons": [
//       { "label":"short", "process_noise_q":0.1, "halflife_bars":31, ...,
//         "series_last": {date,raw_close,smoothed_price,uncertainty,velocity,regime},
//         "event_count": 8 },
//       { "label":"medium", ... },
//       { "label":"long",  ... },
//       { "label":"ultra_long", ... }
//     ]
//   }
//
// **Reference**:
//   - Kalman, R. E. (1960). "A new approach to linear filtering and prediction
//     problems." *Trans. ASME — Journal of Basic Engineering*, 82(1), 35–45.
//   - Roncalli, T. (2013). *Lectures on Risk Management*. CRC Press, §11.2.
//     個股 trend filter,Q ∈ [1e-5, 1e-3] 推薦範圍對應不同 horizon
//   - Bork & Petersen (2014). "Trends in stock prices?" Working Paper(相對 R 推薦)

use anyhow::Result;
use chrono::NaiveDate;
use fact_schema::{Fact, IndicatorCore, Timeframe};
use ohlcv_loader::OhlcvSeries;
use serde::Serialize;
use serde_json::json;

inventory::submit! {
    core_registry::CoreRegistration::new(
        "kalman_filter_core", "0.3.0", core_registry::CoreKind::Indicator, "P3",
        "Kalman Filter Core(multi-horizon 1-D 趨勢平滑 + 5-class regime)",
    )
}

// ---------------------------------------------------------------------------
// Params
// ---------------------------------------------------------------------------

/// velocity acceleration 計算窗(用來分辨「持平」與「加速/減速」的 lookback)
const REGIME_LOOKBACK_DAYS: usize = 20;

/// v3.33:per-horizon 配置。Q 越大 → smoothed 跟得越緊 → halflife 越短 →
/// 對應越短期 prediction horizon。velocity_threshold / min_regime_duration 都
/// 隨 horizon 變化(short horizon 需更大 threshold 過 noise,duration 更短)。
#[derive(Debug, Clone, Serialize)]
pub struct KalmanFilterHorizon {
    pub label: String,
    pub process_noise_q: f64,
    pub velocity_threshold_pct: f64,
    pub min_regime_duration_days: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct KalmanFilterParams {
    pub timeframe: Timeframe,
    /// 觀察噪聲 R 相對係數:R = (rel × mean_price)²(對齊 Bork & Petersen 2014)
    pub measurement_noise_rel: f64,
    /// State 收斂熱身天數(對齊 Roncalli 2013 推薦 30-60)
    pub warmup_days: usize,
    /// v3.33:4 horizons,共用同 OhlcvSeries 跑獨立 recursion。
    pub horizons: Vec<KalmanFilterHorizon>,
    /// primary horizon label(events / facts / top-level series 對齊此 horizon)。
    /// 預設 "medium"(Q=1e-2,~99 bars / 5 月 halflife),對齊 v1.32 P2 ≤ 12/yr/stock。
    pub primary_horizon: String,
}

impl Default for KalmanFilterParams {
    fn default() -> Self {
        Self {
            timeframe: Timeframe::Daily,
            measurement_noise_rel: 0.01,
            warmup_days: 60,
            horizons: default_horizons(),
            primary_horizon: "medium".to_string(),
        }
    }
}

/// v3.33 default 4 horizons:
///   short      Q=1e-1  halflife ~31  bars(~6 週)  threshold 0.003  min_dur 3
///   medium     Q=1e-2  halflife ~99  bars(~5 月)  threshold 0.002  min_dur 5
///   long       Q=1e-3  halflife ~310 bars(~1.2 年)threshold 0.0015 min_dur 5
///   ultra_long Q=1e-5  halflife ~3100 bars(~12 年)threshold 0.001  min_dur 5
///
/// velocity_threshold scaling rationale:Q 大 → smoothed 跟 raw 更緊 → velocity 自然大
/// (high-Q daily smoothed velocity ~ daily return ~ 1-5%);threshold 0.001 對 short
/// horizon 等於所有 trending bars 都 fire → 改用 per-horizon threshold 過濾。
///
/// **v3.34 calibration**(2026-05-18):3030 short horizon velocity=-1.445/day(0.36%)
/// 被 threshold=0.005 歸 Sideway — 此 velocity 已有明顯方向。改 0.003(0.3%/day,
/// 對齊台股 daily return 1σ noise floor ≈ 0.5-1%/day 中位)。
pub fn default_horizons() -> Vec<KalmanFilterHorizon> {
    vec![
        KalmanFilterHorizon {
            label: "short".to_string(),
            process_noise_q: 1e-1,
            velocity_threshold_pct: 0.003,
            min_regime_duration_days: 3,
        },
        KalmanFilterHorizon {
            label: "medium".to_string(),
            process_noise_q: 1e-2,
            velocity_threshold_pct: 0.002,
            min_regime_duration_days: 5,
        },
        KalmanFilterHorizon {
            label: "long".to_string(),
            process_noise_q: 1e-3,
            velocity_threshold_pct: 0.0015,
            min_regime_duration_days: 5,
        },
        KalmanFilterHorizon {
            label: "ultra_long".to_string(),
            process_noise_q: 1e-5,
            velocity_threshold_pct: 0.001,
            min_regime_duration_days: 5,
        },
    ]
}

/// halflife ≈ 9.8 / sqrt(Q)(對齊 R=(0.01·p)² 配方;production-data calibration)。
/// Q=1e-1 → 31 bars / Q=1e-2 → 99 / Q=1e-3 → 310 / Q=1e-5 → 3100。
pub fn halflife_bars_for_q(q: f64) -> f64 {
    if q <= 0.0 {
        return f64::INFINITY;
    }
    9.8 / q.sqrt()
}

// ---------------------------------------------------------------------------
// Output
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct KalmanFilterOutput {
    pub stock_id: String,
    pub timeframe: Timeframe,
    /// primary horizon label(top-level `series` + `events` 對齊此 horizon)
    pub primary_horizon: String,
    /// primary horizon 完整 series(backward compat v3.30 series-last-entry path)
    pub series: Vec<KalmanPoint>,
    /// primary horizon events(對齊 produce_facts 來源)
    pub events: Vec<KalmanEvent>,
    /// v3.33:4 horizons 的 latest state + event_count
    pub horizons: Vec<KalmanHorizonOutput>,
}

#[derive(Debug, Clone, Serialize)]
pub struct KalmanPoint {
    pub date: NaiveDate,
    pub raw_close: f64,
    pub smoothed_price: f64,
    pub uncertainty: f64,           // sqrt(P_t|t),~1-σ
    pub velocity: f64,              // smoothed_t - smoothed_{t-1}
    pub regime: Regime,
}

/// v3.33:per-horizon output(只記 latest state + event count,full series 不存
/// 避免 indicator_values JSONB 體積膨脹 4×)。
#[derive(Debug, Clone, Serialize)]
pub struct KalmanHorizonOutput {
    pub label: String,
    pub process_noise_q: f64,
    pub halflife_bars: f64,
    pub velocity_threshold_pct: f64,
    pub min_regime_duration_days: usize,
    /// 該 horizon 的 series 最末 state(對齊 v3.30 series-last-entry path 慣例)
    pub series_last: Option<KalmanPoint>,
    /// 該 horizon transition event 數(facts 只走 primary,但其他 horizon 仍可參考)
    pub event_count: usize,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum Regime {
    StableUp,
    Accelerating,
    Sideway,
    Decelerating,
    StableDown,
}

#[derive(Debug, Clone, Serialize)]
pub struct KalmanEvent {
    pub date: NaiveDate,
    pub kind: KalmanEventKind,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum KalmanEventKind {
    EnteredStableUp,
    EnteredAccelerating,
    EnteredSideway,
    EnteredDecelerating,
    EnteredStableDown,
}

impl KalmanEventKind {
    fn from_regime(regime: Regime) -> Self {
        match regime {
            Regime::StableUp        => KalmanEventKind::EnteredStableUp,
            Regime::Accelerating    => KalmanEventKind::EnteredAccelerating,
            Regime::Sideway         => KalmanEventKind::EnteredSideway,
            Regime::Decelerating    => KalmanEventKind::EnteredDecelerating,
            Regime::StableDown      => KalmanEventKind::EnteredStableDown,
        }
    }
}

// ---------------------------------------------------------------------------
// Core
// ---------------------------------------------------------------------------

pub struct KalmanFilterCore;
impl KalmanFilterCore { pub fn new() -> Self { KalmanFilterCore } }
impl Default for KalmanFilterCore { fn default() -> Self { KalmanFilterCore::new() } }

impl IndicatorCore for KalmanFilterCore {
    type Input = OhlcvSeries;
    type Params = KalmanFilterParams;
    type Output = KalmanFilterOutput;

    fn name(&self) -> &'static str { "kalman_filter_core" }
    fn version(&self) -> &'static str { "0.3.0" }

    fn warmup_periods(&self, params: &Self::Params) -> usize { params.warmup_days }

    fn compute(&self, input: &Self::Input, params: Self::Params) -> Result<Self::Output> {
        let bars = &input.bars;
        let n = bars.len();
        if n == 0 {
            return Ok(KalmanFilterOutput {
                stock_id: input.stock_id.clone(),
                timeframe: params.timeframe,
                primary_horizon: params.primary_horizon,
                series: vec![],
                events: vec![],
                horizons: vec![],
            });
        }

        // R = (rel × mean_close)²(對齊 Bork & Petersen 2014 推薦的相對誤差模型)。
        // 4 horizons 共用同 R,只 Q / threshold / min_dur 不同。
        let mean_close: f64 = bars.iter().map(|b| b.close).sum::<f64>() / n as f64;
        let r = (params.measurement_noise_rel * mean_close).powi(2);
        let warmup = params.warmup_days.min(n);

        // 跑 4 個 horizons,各自獨立 Kalman state
        let mut horizon_outputs: Vec<(KalmanHorizonOutput, Vec<KalmanPoint>, Vec<KalmanEvent>)> =
            Vec::with_capacity(params.horizons.len());
        for horizon in &params.horizons {
            let series = compute_kalman_recursion(bars, horizon.process_noise_q, r);
            let series = classify_series_regimes(series, horizon.velocity_threshold_pct);
            let events = detect_events_run_length(
                &series, warmup, horizon.min_regime_duration_days,
            );
            let series_last = series.last().cloned();
            let horizon_out = KalmanHorizonOutput {
                label: horizon.label.clone(),
                process_noise_q: horizon.process_noise_q,
                halflife_bars: halflife_bars_for_q(horizon.process_noise_q),
                velocity_threshold_pct: horizon.velocity_threshold_pct,
                min_regime_duration_days: horizon.min_regime_duration_days,
                series_last,
                event_count: events.len(),
            };
            horizon_outputs.push((horizon_out, series, events));
        }

        // 取 primary horizon 的 series + events 寫 top-level(backward compat
        // v3.30 series-last-entry path + produce_facts events source)
        let primary_idx = horizon_outputs
            .iter()
            .position(|(h, _, _)| h.label == params.primary_horizon)
            .unwrap_or(0);
        let (primary_series, primary_events) = {
            let (_, s, e) = &horizon_outputs[primary_idx];
            (s.clone(), e.clone())
        };

        let horizons: Vec<KalmanHorizonOutput> = horizon_outputs
            .into_iter()
            .map(|(h, _, _)| h)
            .collect();

        Ok(KalmanFilterOutput {
            stock_id: input.stock_id.clone(),
            timeframe: params.timeframe,
            primary_horizon: params.primary_horizon,
            series: primary_series,
            events: primary_events,
            horizons,
        })
    }

    fn produce_facts(&self, output: &Self::Output) -> Vec<Fact> {
        output.events.iter().map(|e| Fact {
            stock_id: output.stock_id.clone(),
            fact_date: e.date,
            timeframe: output.timeframe,
            source_core: "kalman_filter_core".to_string(),
            source_version: "0.3.0".to_string(),
            params_hash: None,
            statement: format!("{:?} on {}", e.kind, e.date),
            metadata: fact_schema::with_event_kind(e.metadata.clone(), &e.kind),
        }).collect()
    }
}

// ---------------------------------------------------------------------------
// Kalman recursion helper(v3.33 抽出,給 4 horizons 共用)
// ---------------------------------------------------------------------------

/// 對 OhlcvBars 跑 1-D Kalman recursion 一次,回 series(regime 還未分類,placeholder
/// Sideway)。共用 R,Q 來自 horizon。
fn compute_kalman_recursion(
    bars: &[ohlcv_loader::OhlcvBar], q: f64, r: f64,
) -> Vec<KalmanPoint> {
    let n = bars.len();
    if n == 0 { return Vec::new(); }

    // 初始化:x_0 = 首筆 close;P_0 = R(信任 measurement)
    let mut x = bars[0].close;
    let mut p = r;
    let mut series: Vec<KalmanPoint> = Vec::with_capacity(n);
    let mut prev_x: Option<f64> = None;
    for bar in bars {
        // Predict
        let x_pred = x;
        let p_pred = p + q;
        // Update
        let z = bar.close;
        let k = p_pred / (p_pred + r);
        x = x_pred + k * (z - x_pred);
        p = (1.0 - k) * p_pred;

        let velocity = match prev_x {
            Some(prev) => x - prev,
            None       => 0.0,
        };
        prev_x = Some(x);

        series.push(KalmanPoint {
            date: bar.date,
            raw_close: z,
            smoothed_price: x,
            uncertainty: p.sqrt(),
            velocity,
            regime: Regime::Sideway,
        });
    }
    series
}

/// 對 series 內 KalmanPoint 跑 regime classification(in-place 不變,回新 Vec)。
/// 對齊 spec §7.2 + §7.3:accel = velocity_i - velocity_{i-lookback}(20 bars window)。
fn classify_series_regimes(
    mut series: Vec<KalmanPoint>, velocity_threshold: f64,
) -> Vec<KalmanPoint> {
    let n = series.len();
    for i in 0..n {
        let vel_pct = if series[i].smoothed_price.abs() > 0.0 {
            series[i].velocity / series[i].smoothed_price
        } else { 0.0 };

        let lookback = REGIME_LOOKBACK_DAYS.min(i);
        let accel = if lookback > 0 {
            series[i].velocity - series[i - lookback].velocity
        } else {
            0.0
        };

        series[i].regime = classify_regime(vel_pct, accel, velocity_threshold);
    }
    series
}

// ---------------------------------------------------------------------------
// Event detection — run-length 合併 noise filter(v3.4 r2 r3,v3.33 per-horizon)
// ---------------------------------------------------------------------------

/// 把 series.regime 序列轉成 transition events。
///
/// 演算法:
///   1. 從 `warmup` 起切 runs:`(start_idx, regime, length)` triples
///   2. Length < `min_run_len` 的 run 視為 noise,併入前一個 run
///   3. 在 merged runs 上偵測 transition:相鄰 run regime 不同時 emit event
fn detect_events_run_length(
    series: &[KalmanPoint],
    warmup: usize,
    min_run_len: usize,
) -> Vec<KalmanEvent> {
    let n = series.len();
    if warmup >= n {
        return Vec::new();
    }

    // Pass A:切原始 runs
    let mut runs: Vec<(usize, Regime, usize)> = Vec::new();
    let mut start = warmup;
    let mut cur = series[warmup].regime;
    for i in (warmup + 1)..n {
        if series[i].regime != cur {
            runs.push((start, cur, i - start));
            start = i;
            cur = series[i].regime;
        }
    }
    runs.push((start, cur, n - start));

    // Pass B:合併 short runs 進前一段(noise filter)
    let mut merged: Vec<(usize, Regime, usize)> = Vec::new();
    for (st, reg, len) in runs {
        if len < min_run_len {
            if let Some(last) = merged.last_mut() {
                last.2 += len;
                continue;
            }
        }
        merged.push((st, reg, len));
    }

    // Pass C:在 merged runs 上偵測 regime transition
    let mut events: Vec<KalmanEvent> = Vec::new();
    let mut prev_regime: Option<Regime> = None;
    for (st, reg, len) in merged {
        if Some(reg) != prev_regime {
            let anchor = &series[st];
            events.push(KalmanEvent {
                date: anchor.date,
                kind: KalmanEventKind::from_regime(reg),
                metadata: json!({
                    "smoothed_price": anchor.smoothed_price,
                    "raw_close": anchor.raw_close,
                    "velocity": anchor.velocity,
                    "uncertainty": anchor.uncertainty,
                    "from_regime": prev_regime.map(|r| format!("{:?}", r)),
                    "to_regime": format!("{:?}", reg),
                    "run_length": len,
                }),
            });
            prev_regime = Some(reg);
        }
    }
    events
}

// ---------------------------------------------------------------------------
// Regime classifier(out of trait,易測試)
// ---------------------------------------------------------------------------

/// |accel| < ACCEL_STABLE_EPS → 視為 stable(對齊 spec §4「|accel| 小 → Stable*」)。
const ACCEL_STABLE_EPS: f64 = 0.01;

/// 5-class regime(對齊 user 拍版 2026-05-15)。
pub fn classify_regime(vel_pct: f64, accel: f64, threshold: f64) -> Regime {
    if vel_pct.abs() < threshold {
        return Regime::Sideway;
    }
    if vel_pct > 0.0 {
        if accel.abs() < ACCEL_STABLE_EPS {
            Regime::StableUp
        } else if accel > 0.0 {
            Regime::Accelerating
        } else {
            Regime::Decelerating
        }
    } else {
        if accel.abs() < ACCEL_STABLE_EPS {
            Regime::StableDown
        } else if accel > 0.0 {
            Regime::Decelerating
        } else {
            Regime::StableDown
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ohlcv_loader::OhlcvBar;

    fn nd(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn mk_bar(date: &str, close: f64) -> OhlcvBar {
        OhlcvBar {
            date: nd(date),
            open: close, high: close, low: close, close, volume: None,
        }
    }

    #[test]
    fn name_version() {
        let core = KalmanFilterCore::new();
        assert_eq!(core.name(), "kalman_filter_core");
        assert_eq!(core.version(), "0.3.0");
        assert_eq!(core.warmup_periods(&KalmanFilterParams::default()), 60);
    }

    #[test]
    fn default_has_four_horizons() {
        // v3.33:default 4 horizons,Q=1e-1/1e-2/1e-3/1e-5
        let p = KalmanFilterParams::default();
        assert_eq!(p.horizons.len(), 4);
        let qs: Vec<f64> = p.horizons.iter().map(|h| h.process_noise_q).collect();
        assert_eq!(qs, vec![1e-1, 1e-2, 1e-3, 1e-5]);
        let labels: Vec<&str> = p.horizons.iter().map(|h| h.label.as_str()).collect();
        assert_eq!(labels, vec!["short", "medium", "long", "ultra_long"]);
        assert_eq!(p.primary_horizon, "medium");
    }

    #[test]
    fn halflife_formula_matches_spec_table() {
        // halflife ≈ 9.8 / sqrt(Q)
        // Q=1e-1 → 31 / Q=1e-2 → 99 / Q=1e-3 → 310 / Q=1e-4 → 990 / Q=1e-5 → 3100
        assert!((halflife_bars_for_q(1e-1) - 30.99).abs() < 1.0);
        assert!((halflife_bars_for_q(1e-2) - 98.0).abs() < 2.0);
        assert!((halflife_bars_for_q(1e-3) - 309.83).abs() < 2.0);
        assert!((halflife_bars_for_q(1e-5) - 3098.39).abs() < 5.0);
    }

    #[test]
    fn empty_series_no_panic() {
        let series = OhlcvSeries {
            stock_id: "2330".to_string(),
            timeframe: Timeframe::Daily,
            bars: vec![],
        };
        let out = KalmanFilterCore::new().compute(&series, KalmanFilterParams::default()).unwrap();
        assert!(out.series.is_empty());
        assert!(out.events.is_empty());
        assert!(out.horizons.is_empty());
        assert_eq!(out.primary_horizon, "medium");
    }

    #[test]
    fn kalman_converges_to_step_input() {
        let bars: Vec<OhlcvBar> = (0..100)
            .map(|i| mk_bar(&format!("2026-01-{:02}", (i % 28) + 1), 1000.0))
            .collect();
        let series = OhlcvSeries {
            stock_id: "S1".to_string(), timeframe: Timeframe::Daily, bars,
        };
        let out = KalmanFilterCore::new().compute(&series, KalmanFilterParams::default()).unwrap();
        // primary horizon (medium) series 全 1000
        let last = out.series.last().unwrap();
        assert!((last.smoothed_price - 1000.0).abs() < 0.5,
            "step input medium horizon 應收斂到 ~1000.0;實際 {}", last.smoothed_price);
    }

    #[test]
    fn multi_horizon_independent_state_and_full_population() {
        // v3.33:4 horizons 各自獨立跑,horizons.len()==4 + 每個 series_last 在
        let bars: Vec<OhlcvBar> = (0..150)
            .map(|i| {
                let close = if i < 75 { 100.0 } else { 200.0 };
                mk_bar(&format!("2026-01-{:02}", (i % 28) + 1), close)
            })
            .collect();
        let series = OhlcvSeries {
            stock_id: "S1".to_string(), timeframe: Timeframe::Daily, bars,
        };
        let out = KalmanFilterCore::new().compute(&series, KalmanFilterParams::default()).unwrap();
        assert_eq!(out.horizons.len(), 4);

        let labels: Vec<&str> = out.horizons.iter().map(|h| h.label.as_str()).collect();
        assert_eq!(labels, vec!["short", "medium", "long", "ultra_long"]);

        // 每個 horizon 都有 series_last
        for h in &out.horizons {
            assert!(h.series_last.is_some(), "{} horizon series_last 缺失", h.label);
        }

        // short horizon Q 大 → smoothed 對 step 跟得更緊,short.smoothed >
        // medium.smoothed > long.smoothed > ultra_long.smoothed(假設新一段 200 高於舊 100)
        let smooth_short = out.horizons[0].series_last.as_ref().unwrap().smoothed_price;
        let smooth_medium = out.horizons[1].series_last.as_ref().unwrap().smoothed_price;
        let smooth_long = out.horizons[2].series_last.as_ref().unwrap().smoothed_price;
        let smooth_ultra = out.horizons[3].series_last.as_ref().unwrap().smoothed_price;
        assert!(smooth_short >= smooth_medium,
            "short ({}) 應 ≥ medium ({})", smooth_short, smooth_medium);
        assert!(smooth_medium >= smooth_long,
            "medium ({}) 應 ≥ long ({})", smooth_medium, smooth_long);
        assert!(smooth_long >= smooth_ultra,
            "long ({}) 應 ≥ ultra_long ({})", smooth_long, smooth_ultra);
        // short horizon 應跟最緊,smoothed 接近 200(raw final)
        assert!(smooth_short > 180.0,
            "short horizon 應對 step 跟得緊,實際 {}", smooth_short);
        // ultra_long horizon 反應應該明顯比 short 慢(margin ≥ 30,具體值不重要)
        // 注意:K 在初始 transient 期較大(P_0=R → P 逐步降到 K_∞·R),所以 ultra_long
        // 也會吸收部分 step;但 steady-state K_∞ ≈ sqrt(Q/R) 對 Q=1e-5 ≈ 0.002,
        // 之後極慢追蹤,smoothed 應顯著低於 short
        assert!(smooth_short - smooth_ultra > 30.0,
            "short - ultra_long margin 應 > 30(short跟得緊 / ultra慢);實際 short={} ultra={}",
            smooth_short, smooth_ultra);
    }

    #[test]
    fn primary_horizon_drives_top_level_series_and_events() {
        // primary="medium" → out.series 是 medium horizon 的,events 也是
        let bars: Vec<OhlcvBar> = (0..200)
            .map(|i| mk_bar(&format!("2026-01-{:02}", (i % 28) + 1), 100.0 + (i as f64) * 0.5))
            .collect();
        let series = OhlcvSeries {
            stock_id: "S1".to_string(), timeframe: Timeframe::Daily, bars,
        };
        let out = KalmanFilterCore::new().compute(&series, KalmanFilterParams::default()).unwrap();

        // out.series 對齊 medium horizon last
        let medium_last = out.horizons.iter()
            .find(|h| h.label == "medium")
            .and_then(|h| h.series_last.clone())
            .unwrap();
        let top_last = out.series.last().unwrap().clone();
        assert!((medium_last.smoothed_price - top_last.smoothed_price).abs() < 1e-9);
        assert_eq!(medium_last.date, top_last.date);

        // top events 數量 = medium horizon event_count
        let medium_event_count = out.horizons.iter()
            .find(|h| h.label == "medium")
            .map(|h| h.event_count).unwrap();
        assert_eq!(out.events.len(), medium_event_count);
    }

    #[test]
    fn produce_facts_only_from_primary_horizon() {
        // facts 數量 = out.events 數量 = primary horizon event count
        let bars: Vec<OhlcvBar> = (0..200)
            .map(|i| mk_bar(&format!("2026-01-{:02}", (i % 28) + 1), 100.0 + (i as f64) * 0.5))
            .collect();
        let series = OhlcvSeries {
            stock_id: "S1".to_string(), timeframe: Timeframe::Daily, bars,
        };
        let core = KalmanFilterCore::new();
        let out = core.compute(&series, KalmanFilterParams::default()).unwrap();
        let facts = core.produce_facts(&out);
        assert_eq!(facts.len(), out.events.len(),
            "produce_facts 數應等於 primary horizon events 數");
    }

    #[test]
    fn classify_regime_sideway_when_velocity_below_threshold() {
        assert_eq!(classify_regime(0.0005, 1.0, 0.001), Regime::Sideway);
        assert_eq!(classify_regime(-0.0005, -10.0, 0.001), Regime::Sideway);
    }

    #[test]
    fn classify_regime_stable_up() {
        assert_eq!(classify_regime(0.005, 0.0, 0.001), Regime::StableUp);
    }

    #[test]
    fn classify_regime_accelerating() {
        assert_eq!(classify_regime(0.005, 0.5, 0.001), Regime::Accelerating);
    }

    #[test]
    fn classify_regime_decelerating_during_uptrend() {
        assert_eq!(classify_regime(0.005, -0.5, 0.001), Regime::Decelerating);
    }

    #[test]
    fn classify_regime_stable_down() {
        assert_eq!(classify_regime(-0.005, 0.0, 0.001), Regime::StableDown);
        assert_eq!(classify_regime(-0.005, -0.5, 0.001), Regime::StableDown);
    }

    #[test]
    fn classify_regime_decelerating_during_downtrend() {
        assert_eq!(classify_regime(-0.005, 0.5, 0.001), Regime::Decelerating);
    }

    #[test]
    fn run_length_filter_absorbs_short_runs() {
        let mk = |i: i64, r: Regime| KalmanPoint {
            date: nd("2026-01-01") + chrono::Duration::days(i),
            raw_close: 100.0, smoothed_price: 100.0, uncertainty: 0.1,
            velocity: 0.0, regime: r,
        };
        let mut series = Vec::new();
        for i in 0..10 { series.push(mk(i, Regime::Sideway)); }
        for i in 10..13 { series.push(mk(i, Regime::StableUp)); }
        for i in 13..23 { series.push(mk(i, Regime::Sideway)); }

        let events = detect_events_run_length(&series, 0, 5);
        assert_eq!(events.len(), 1,
            "3-bar StableUp 短 run 應被吸收,只剩 1 個 transition,實際 {}", events.len());
        assert_eq!(events[0].kind, KalmanEventKind::EnteredSideway);
    }

    #[test]
    fn run_length_filter_emits_long_run_transition() {
        let mk = |i: i64, r: Regime| KalmanPoint {
            date: nd("2026-01-01") + chrono::Duration::days(i),
            raw_close: 100.0, smoothed_price: 100.0, uncertainty: 0.1,
            velocity: 0.0, regime: r,
        };
        let mut series = Vec::new();
        for i in 0..10 { series.push(mk(i, Regime::Sideway)); }
        for i in 10..20 { series.push(mk(i, Regime::StableUp)); }
        for i in 20..30 { series.push(mk(i, Regime::Sideway)); }

        let events = detect_events_run_length(&series, 0, 5);
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].kind, KalmanEventKind::EnteredSideway);
        assert_eq!(events[1].kind, KalmanEventKind::EnteredStableUp);
        assert_eq!(events[2].kind, KalmanEventKind::EnteredSideway);
    }

    #[test]
    fn warmup_suppresses_phantom_events() {
        let mut bars: Vec<OhlcvBar> = Vec::new();
        for i in 0..30 {
            bars.push(mk_bar(&format!("2026-01-{:02}", (i % 28) + 1), 1000.0));
        }
        for i in 30..60 {
            bars.push(mk_bar(&format!("2026-02-{:02}", (i % 28) + 1), 1100.0));
        }
        let series = OhlcvSeries { stock_id: "S1".to_string(), timeframe: Timeframe::Daily, bars };
        let out = KalmanFilterCore::new().compute(&series, KalmanFilterParams::default()).unwrap();
        assert_eq!(out.events.len(), 0, "warmup 期間不該產 event(全部 60 bars)");
    }

    #[test]
    fn short_horizon_responds_faster_than_ultra_long() {
        // v3.33:對 step input(100 → 200)short horizon halflife ~31 bars,應在
        // 60 bars 內 smoothed 跨過 150(中點),ultra_long(~3100)需 thousands of bars。
        let mut bars: Vec<OhlcvBar> = Vec::new();
        for i in 0..30 { bars.push(mk_bar(&format!("2026-01-{:02}", (i % 28) + 1), 100.0)); }
        // step up at bar 30
        for i in 30..120 { bars.push(mk_bar(&format!("2026-02-{:02}", (i % 28) + 1), 200.0)); }
        let series = OhlcvSeries { stock_id: "S1".to_string(), timeframe: Timeframe::Daily, bars };
        let out = KalmanFilterCore::new().compute(&series, KalmanFilterParams::default()).unwrap();

        let short_last = out.horizons[0].series_last.as_ref().unwrap();   // short
        let ultra_last = out.horizons[3].series_last.as_ref().unwrap();   // ultra_long
        // 90 bars step後,short 應已接近 200
        assert!(short_last.smoothed_price > 180.0,
            "90 bars 後 short horizon smoothed 應接近 200,實際 {}",
            short_last.smoothed_price);
        // ultra_long 應顯著落後 short(margin > 20)
        assert!(short_last.smoothed_price - ultra_last.smoothed_price > 20.0,
            "short - ultra_long margin 應 > 20;實際 short={} ultra={}",
            short_last.smoothed_price, ultra_last.smoothed_price);
    }
}
