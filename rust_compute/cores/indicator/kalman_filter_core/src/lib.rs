// kalman_filter_core(P3)— Indicator Core(對齊 m3Spec/kalman_filter_core.md)
//
// 1-D 趨勢平滑(state-space):
//   state x_t       = "true underlying price"(unobservable)
//   observation z_t = price_daily_fwd.close
//
//   Predict:  x_pred = x_prev;        P_pred = P_prev + Q
//   Update:   K = P_pred / (P_pred + R)
//             x = x_pred + K * (z - x_pred)
//             P = (1 - K) * P_pred
//
// Regime classification(5 類,對齊 user 拍版 2026-05-15):
//   StableUp        : velocity_pct >  threshold, |accel| 小
//   Accelerating    : velocity_pct >  threshold, accel > 0(加速上漲)
//   Sideway         : |velocity_pct| < threshold
//   Decelerating    : (velocity_pct >  threshold, accel < 0) ∪
//                     (velocity_pct < -threshold, accel > 0)
//                     ← 同 sign 但減速 / 反向但減速,都算「動能在消退」
//   StableDown      : velocity_pct < -threshold, |accel| 小
//
// EventKind:5 種 transition(regime 變更時觸發一次,對齊 v1.32 P2 transition pattern):
//   EnteredStableUp / EnteredAccelerating / EnteredSideway /
//   EnteredDecelerating / EnteredStableDown
//
// **v3.4 r2 calibration(2026-05-16)**:
//   - 保留 velocity_threshold_pct = 0.001(Roncalli 2013 推薦,production 驗證可達)
//     ⚠️ 注意:Q=1e-5/R=(0.01p)² steady-state K≈0.002,vel_pct 上限 ~0.002,
//        threshold>0.001 會把所有 stock 鎖在 Sideway → 0 events(初版 0.003 已驗失敗)
//   - 加 min_regime_duration_days(預設 5):regime 必須持續 ≥ 5 個交易日才產 event
//     (suppress consecutive flips,避開 close noise 引發的 false transition)
//   - 1263 stocks × ~134 K events(107/yr/stock,9× 超 v1.32 P2 ≤ 12/yr)→
//     sustain filter 預估降至 ~9-12/yr,落入 P2 acceptance 標準
//
// **Reference**:
//   - Kalman, R. E. (1960). "A new approach to linear filtering and prediction
//     problems." *Trans. ASME — Journal of Basic Engineering*, 82(1), 35–45.
//     原始 paper,公式 2.4 + 2.7(1-D 退化版本本檔採用)
//   - Roncalli, T. (2013). *Lectures on Risk Management*. CRC Press, §11.2.
//     個股 trend filter,Q=1e-5 / R 相對 price 推薦
//   - Bork & Petersen (2014). "Trends in stock prices?" Working Paper

use anyhow::Result;
use chrono::NaiveDate;
use fact_schema::{Fact, IndicatorCore, Timeframe};
use ohlcv_loader::OhlcvSeries;
use serde::Serialize;
use serde_json::json;

inventory::submit! {
    core_registry::CoreRegistration::new(
        "kalman_filter_core", "0.2.0", core_registry::CoreKind::Indicator, "P3",
        "Kalman Filter Core(1-D 趨勢平滑 + 5-class regime)",
    )
}

// ---------------------------------------------------------------------------
// Params
// ---------------------------------------------------------------------------

/// velocity acceleration 計算窗(用來分辨「持平」與「加速/減速」的 lookback)
const REGIME_LOOKBACK_DAYS: usize = 20;

/// v3.5 R4 C11 default:對齊 audit Layer 3 痛點 12 — const 改 Params field 給 caller override。
/// 維持 5 個交易日(≈ 1 週)default,避免 close noise 引發的高頻 regime flip
/// (對齊 v1.32 P2 ≤ 12/yr/stock 目標 + v3.4 r2 calibration 結果)。
const MIN_REGIME_DURATION_DAYS_DEFAULT: usize = 5;

#[derive(Debug, Clone, Serialize)]
pub struct KalmanFilterParams {
    pub timeframe: Timeframe,
    /// 狀態噪聲 Q:越小 → smoothed_price 越平滑(對齊 Roncalli 2013 推薦 1e-5)
    pub process_noise_q: f64,
    /// 觀察噪聲 R 相對係數:R = (rel × mean_price)²(對齊 Bork & Petersen 2014)
    pub measurement_noise_rel: f64,
    /// State 收斂熱身天數(對齊 Roncalli 2013 推薦 30-60)
    pub warmup_days: usize,
    /// velocity / smoothed_price 比例閾值(0.001 = 0.1%/day)— 分辨 stable vs sideway
    pub velocity_threshold_pct: f64,
    /// v3.5 R4 C11:regime 必須維持 ≥ min_regime_duration_days 個交易日才視為「進入」,
    /// 避免 close noise 引發的高頻 regime flip(對齊 v1.32 P2 ≤ 12/yr/stock 目標)。
    /// 預設 5 個交易日(≈ 1 週)。caller 可調(monthly tf 建議升 2-3,
    /// intraday tf 建議降到 1-2)。
    pub min_regime_duration_days: usize,
}

impl Default for KalmanFilterParams {
    fn default() -> Self {
        Self {
            timeframe: Timeframe::Daily,
            process_noise_q: 1e-5,
            measurement_noise_rel: 0.01,
            warmup_days: 60,
            // v3.4 r2 r2:revert 0.001(對齊 Roncalli 2013 + Bork & Petersen 2014)
            //
            // 數學分析(2026-05-16):Q=1e-5 / R=(0.01×p)² 配方下,steady-state
            // Kalman gain K_∞ = sqrt(Q/R)/2 ≈ 0.002。daily innovation 1% → smoothed
            // velocity = K × innovation ≈ 0.002 × 0.01 × price = 2e-5 × price →
            // velocity_pct ≈ 2e-5。threshold=0.003 完全不可達(production 1266
            // stocks × 0 events 驗證)。
            //
            // 0.001(0.1%/day)實際 production 約 17.6 events/yr/stock,加 sustain=5
            // 過濾 noise 後預估 ~9-12 events/yr/stock(對齊 v1.32 P2 ≤ 12/yr 標準)。
            velocity_threshold_pct: 0.001,
            min_regime_duration_days: MIN_REGIME_DURATION_DAYS_DEFAULT,
        }
    }
}

// ---------------------------------------------------------------------------
// Output
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct KalmanFilterOutput {
    pub stock_id: String,
    pub timeframe: Timeframe,
    pub series: Vec<KalmanPoint>,
    pub events: Vec<KalmanEvent>,
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
    fn version(&self) -> &'static str { "0.2.0" }

    fn warmup_periods(&self, params: &Self::Params) -> usize { params.warmup_days }

    fn compute(&self, input: &Self::Input, params: Self::Params) -> Result<Self::Output> {
        let bars = &input.bars;
        let n = bars.len();
        if n == 0 {
            return Ok(KalmanFilterOutput {
                stock_id: input.stock_id.clone(),
                timeframe: params.timeframe,
                series: vec![], events: vec![],
            });
        }

        // R = (rel × mean_close)²(對齊 Bork & Petersen 2014 推薦的相對誤差模型)
        let mean_close: f64 = bars.iter().map(|b| b.close).sum::<f64>() / n as f64;
        let r = (params.measurement_noise_rel * mean_close).powi(2);
        let q = params.process_noise_q;

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
                regime: Regime::Sideway,    // placeholder,下迴圈 classify
            });
        }

        // Pass 2:Regime classification(需 acceleration → 用 lookback)
        for i in 0..n {
            let vel_pct = if series[i].smoothed_price.abs() > 0.0 {
                series[i].velocity / series[i].smoothed_price
            } else { 0.0 };

            // accel = velocity_i - velocity_{i-lookback}(若 i 不足,用 i 自身比 0)
            let lookback = REGIME_LOOKBACK_DAYS.min(i);
            let accel = if lookback > 0 {
                series[i].velocity - series[i - lookback].velocity
            } else {
                0.0
            };

            let regime = classify_regime(vel_pct, accel, params.velocity_threshold_pct);
            series[i].regime = regime;
        }

        // Pass 3:Transition events(warmup 後才產 event,避免 Kalman state 還在收斂)
        //
        // v3.4 r2 r3 calibration(2026-05-16):consecutive-bars sustain filter 失敗。
        // 揭露 production 0 events 後分析:vel_pct 在 threshold 邊界 oscillate 每
        // 1-3 bars,從來不會連 5 bars 同 regime。改 **run-length 合併**:
        //   1. 把 regime series 切成 runs(連續同 regime 段)
        //   2. Length < min_regime_duration_days 的 run 併入前一個 run(視為 noise)
        //   3. 在 merged runs 上偵測 transition
        // 預期:134K → ~10-30K events(對齊 v1.32 P2 ≤ 12/yr/stock 標準)。
        let events = detect_events_run_length(
            &series,
            params.warmup_days.min(n),
            params.min_regime_duration_days,
        );

        Ok(KalmanFilterOutput {
            stock_id: input.stock_id.clone(),
            timeframe: params.timeframe,
            series, events,
        })
    }

    fn produce_facts(&self, output: &Self::Output) -> Vec<Fact> {
        output.events.iter().map(|e| Fact {
            stock_id: output.stock_id.clone(),
            fact_date: e.date,
            timeframe: output.timeframe,
            source_core: "kalman_filter_core".to_string(),
            source_version: "0.2.0".to_string(),
            params_hash: None,
            statement: format!("{:?} on {}", e.kind, e.date),
            metadata: fact_schema::with_event_kind(e.metadata.clone(), &e.kind),
        }).collect()
    }
}

// ---------------------------------------------------------------------------
// Event detection — run-length 合併 noise filter(v3.4 r2 r3)
// ---------------------------------------------------------------------------

/// 把 series.regime 序列轉成 transition events。
///
/// 演算法:
///   1. 從 `warmup` 起切 runs:`(start_idx, regime, length)` triples
///   2. Length < `min_run_len` 的 run 視為 noise,併入前一個 run
///      (沒前一個 run → 保留;corner case 第一段就 < min 罕見也無害)
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
                // 將 short run 併入前一 run:延長 length,不改 regime / start
                last.2 += len;
                continue;
            }
            // 第一段就 < min:無前可併,保留(罕見 edge case)
        }
        merged.push((st, reg, len));
    }

    // Pass C:在 merged runs 上偵測 regime transition
    let mut events: Vec<KalmanEvent> = Vec::new();
    let mut prev_regime: Option<Regime> = None;
    for (st, reg, len) in merged {
        if Some(reg) != prev_regime {
            // 第一個 run 也產 event(從 None 進入該 regime)
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
///
/// v3.4 r2 r4(2026-05-16):原本「`else { StableUp/Down }`」只在 `accel == 0.0`
/// 精確相等才觸發,浮點實際從不命中 → production 34593 events 中 StableUp = 0,
/// 上下行對映不對稱(StableDown 在 `accel ≤ 0` 涵蓋,StableUp 只在 accel==0
/// 涵蓋)。改用 ACCEL_STABLE_EPS,讓 |accel| < EPS 走 Stable*,二側對稱。
///
/// EPS = 0.01:對 Kalman smoothed velocity 是 1 cent / day(price scale 100 NTD
/// 下 ≈ 0.01% 加速度),足夠濾掉浮點 noise 而仍能區分有意義的 accel/decel。
const ACCEL_STABLE_EPS: f64 = 0.01;

/// 5-class regime(對齊 user 拍版 2026-05-15)。
///
/// vel_pct:smoothed velocity / smoothed_price(±, 0.001=0.1%/day)
/// accel:  velocity_now - velocity_{N-bars-ago}
/// thresh: |vel_pct| < thresh → Sideway
pub fn classify_regime(vel_pct: f64, accel: f64, threshold: f64) -> Regime {
    if vel_pct.abs() < threshold {
        return Regime::Sideway;
    }
    if vel_pct > 0.0 {
        // 上漲:|accel| < EPS → StableUp;accel > 0 → 加速;accel < 0 → 減速
        if accel.abs() < ACCEL_STABLE_EPS {
            Regime::StableUp
        } else if accel > 0.0 {
            Regime::Accelerating
        } else {
            Regime::Decelerating
        }
    } else {
        // 下跌:|accel| < EPS → StableDown;accel > 0 → 動能消退(reversal pending);
        //       accel < 0 → 加速下跌(視為 StableDown 連動性 → 仍標 StableDown)
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
        assert_eq!(core.version(), "0.2.0");
        assert_eq!(core.warmup_periods(&KalmanFilterParams::default()), 60);
    }

    #[test]
    fn default_threshold_stays_at_0_001_for_kalman_recipe() {
        // v3.4 r2 r2:keep 0.001(initial 0.003 attempt produced 0 events
        // because Q=1e-5/R=(0.01p)² Kalman recipe caps vel_pct ~ 0.002)
        let p = KalmanFilterParams::default();
        assert!((p.velocity_threshold_pct - 0.001).abs() < 1e-9,
            "default velocity_threshold_pct 應為 0.001,實際 {}", p.velocity_threshold_pct);
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
    }

    #[test]
    fn kalman_converges_to_step_input() {
        // 60+ bars 全是 1000.0;Kalman smoothed_price 應收斂到 1000
        let bars: Vec<OhlcvBar> = (0..100)
            .map(|i| mk_bar(&format!("2026-01-{:02}", (i % 28) + 1), 1000.0))
            .enumerate()
            .map(|(_, b)| b)
            .collect();
        let series = OhlcvSeries {
            stock_id: "S1".to_string(), timeframe: Timeframe::Daily, bars,
        };
        let out = KalmanFilterCore::new().compute(&series, KalmanFilterParams::default()).unwrap();
        let last = out.series.last().unwrap();
        assert!((last.smoothed_price - 1000.0).abs() < 0.01,
            "step input 應收斂到 1000.0;實際 {}", last.smoothed_price);
        // uncertainty 應該收斂到接近 sqrt(Q) 的 steady state(非常小)
        assert!(last.uncertainty < 1.0);
    }

    #[test]
    fn classify_regime_sideway_when_velocity_below_threshold() {
        // |vel_pct| < threshold → Sideway,不管 accel
        assert_eq!(classify_regime(0.0005, 1.0, 0.001), Regime::Sideway);
        assert_eq!(classify_regime(-0.0005, -10.0, 0.001), Regime::Sideway);
    }

    #[test]
    fn classify_regime_stable_up() {
        // vel > thresh, accel ≈ 0 → StableUp
        assert_eq!(classify_regime(0.005, 0.0, 0.001), Regime::StableUp);
    }

    #[test]
    fn classify_regime_accelerating() {
        // vel > thresh, accel > 0 → Accelerating
        assert_eq!(classify_regime(0.005, 0.5, 0.001), Regime::Accelerating);
    }

    #[test]
    fn classify_regime_decelerating_during_uptrend() {
        // vel > thresh, accel < 0 → Decelerating(上漲動能消退)
        assert_eq!(classify_regime(0.005, -0.5, 0.001), Regime::Decelerating);
    }

    #[test]
    fn classify_regime_stable_down() {
        // vel < -thresh, accel ≤ 0 → StableDown
        assert_eq!(classify_regime(-0.005, 0.0, 0.001), Regime::StableDown);
        assert_eq!(classify_regime(-0.005, -0.5, 0.001), Regime::StableDown);
    }

    #[test]
    fn classify_regime_decelerating_during_downtrend() {
        // vel < -thresh, accel > 0 → Decelerating(下跌動能消退;reversal pending)
        assert_eq!(classify_regime(-0.005, 0.5, 0.001), Regime::Decelerating);
    }

    #[test]
    fn run_length_filter_absorbs_short_runs() {
        // v3.4 r2 r3:short runs(< min_regime_duration_days=5)併入前一段。
        // 構造合成 regime sequence,測試 detect_events_run_length:
        //   Sideway × 10 → StableUp × 3 → Sideway × 10
        //   預期:StableUp(3) 併入前 Sideway,merged 結果 = Sideway(23)
        //   final transitions = 1 個(EnteredSideway 起始,from None)
        let nd = |s: &str| NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap();
        let mk = |i: i64, r: Regime| KalmanPoint {
            date: nd("2026-01-01") + chrono::Duration::days(i),
            raw_close: 100.0,
            smoothed_price: 100.0,
            uncertainty: 0.1,
            velocity: 0.0,
            regime: r,
        };
        let mut series = Vec::new();
        for i in 0..10 { series.push(mk(i, Regime::Sideway)); }
        for i in 10..13 { series.push(mk(i, Regime::StableUp)); }   // 3 bars,< 5
        for i in 13..23 { series.push(mk(i, Regime::Sideway)); }

        let events = detect_events_run_length(&series, 0, 5);
        // 預期:StableUp(3) 短 run 被併入前 Sideway,只剩 1 段 Sideway → 1 event
        assert_eq!(events.len(), 1,
            "3-bar StableUp 短 run 應被吸收,只剩 1 個 transition,實際 {}", events.len());
        assert_eq!(events[0].kind, KalmanEventKind::EnteredSideway);
    }

    #[test]
    fn run_length_filter_emits_long_run_transition() {
        // 構造:Sideway × 10 → StableUp × 10 → Sideway × 10
        //   兩段都 ≥ 5,merged = 3 段 → 3 transitions(Sideway / StableUp / Sideway)
        let nd = |s: &str| NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap();
        let mk = |i: i64, r: Regime| KalmanPoint {
            date: nd("2026-01-01") + chrono::Duration::days(i),
            raw_close: 100.0,
            smoothed_price: 100.0,
            uncertainty: 0.1,
            velocity: 0.0,
            regime: r,
        };
        let mut series = Vec::new();
        for i in 0..10 { series.push(mk(i, Regime::Sideway)); }
        for i in 10..20 { series.push(mk(i, Regime::StableUp)); }
        for i in 20..30 { series.push(mk(i, Regime::Sideway)); }

        let events = detect_events_run_length(&series, 0, 5);
        assert_eq!(events.len(), 3,
            "3 段長 run 應產 3 transitions,實際 {}", events.len());
        assert_eq!(events[0].kind, KalmanEventKind::EnteredSideway);
        assert_eq!(events[1].kind, KalmanEventKind::EnteredStableUp);
        assert_eq!(events[2].kind, KalmanEventKind::EnteredSideway);
    }

    #[test]
    fn warmup_suppresses_phantom_events() {
        // 前 warmup_days(60)bars 內不產 event,避免 Kalman state 收斂時的 noise
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
}
