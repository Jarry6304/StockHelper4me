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
        "kalman_filter_core", "0.1.0", core_registry::CoreKind::Indicator, "P3",
        "Kalman Filter Core(1-D 趨勢平滑 + 5-class regime)",
    )
}

// ---------------------------------------------------------------------------
// Params
// ---------------------------------------------------------------------------

/// velocity acceleration 計算窗(用來分辨「持平」與「加速/減速」的 lookback)
const REGIME_LOOKBACK_DAYS: usize = 20;

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
}

impl Default for KalmanFilterParams {
    fn default() -> Self {
        Self {
            timeframe: Timeframe::Daily,
            process_noise_q: 1e-5,
            measurement_noise_rel: 0.01,
            warmup_days: 60,
            velocity_threshold_pct: 0.001,   // 0.1% per day
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
    fn version(&self) -> &'static str { "0.1.0" }

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
        let mut events: Vec<KalmanEvent> = Vec::new();
        let warmup = params.warmup_days.min(n);
        let mut prev_regime: Option<Regime> = None;
        for (i, point) in series.iter().enumerate() {
            if i < warmup {
                prev_regime = Some(point.regime);
                continue;
            }
            let cur_regime = point.regime;
            if Some(cur_regime) != prev_regime {
                events.push(KalmanEvent {
                    date: point.date,
                    kind: KalmanEventKind::from_regime(cur_regime),
                    metadata: json!({
                        "smoothed_price": point.smoothed_price,
                        "raw_close": point.raw_close,
                        "velocity": point.velocity,
                        "uncertainty": point.uncertainty,
                        "from_regime": prev_regime.map(|r| format!("{:?}", r)),
                        "to_regime": format!("{:?}", cur_regime),
                    }),
                });
                prev_regime = Some(cur_regime);
            }
        }

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
            source_version: "0.1.0".to_string(),
            params_hash: None,
            statement: format!("{:?} on {}", e.kind, e.date),
            metadata: e.metadata.clone(),
        }).collect()
    }
}

// ---------------------------------------------------------------------------
// Regime classifier(out of trait,易測試)
// ---------------------------------------------------------------------------

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
        // 上漲:accel > 0 → 加速;accel < 0 → 減速(StableUp 中性視為 stable);
        if accel > 0.0 {
            Regime::Accelerating
        } else if accel < 0.0 {
            Regime::Decelerating
        } else {
            Regime::StableUp
        }
    } else {
        // 下跌:accel < 0 → 加速下跌(視為 StableDown 同類但仍標 StableDown);
        //       accel > 0 → 動能消退(Decelerating;reversal pending)
        if accel > 0.0 {
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
        assert_eq!(core.version(), "0.1.0");
        assert_eq!(core.warmup_periods(&KalmanFilterParams::default()), 60);
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
