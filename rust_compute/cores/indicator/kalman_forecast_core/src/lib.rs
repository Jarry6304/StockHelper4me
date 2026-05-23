// kalman_forecast_core(P3,v0.3 interval-forecast spine phase 3)
//
// Local Linear Trend (LLT) Kalman filter for interval forecasting.
//
// Spec source: user v0.3 interval-forecast spine spec(2026-05-23)+
// plan /root/.claude/plans/stockhelper4me-serene-thacker.md phase 3.
//
// Critical rules (v0.3 spec §「強制規則」):
//   - filtered state ONLY(x_t|t),禁用 RTS smoother(t 後資訊不可用)
//   - calibrated=false 寫入 forecast_log(原始帶 — CQR/ACI 校準在 phase 4)
//   - forecast_date = 最後一根 bar 的 date(對齊 causal one-pass 回測)
//
// LLT 數學 (Harvey 1989 + Durbin & Koopman 2012):
//   State:        x = [level, slope]ᵀ
//   Transition:   F = [[1, 1], [0, 1]]
//   Observation:  H = [1, 0]
//   Process Q:    diag(Q_level, Q_slope)
//   Obs noise:    R(scalar)
//
//   Predict:  x⁻ = F x;  P⁻ = F P Fᵀ + Q
//   Update:   K = P⁻ Hᵀ / (H P⁻ Hᵀ + R)
//             x = x⁻ + K (z - H x⁻)
//             P = (I - K H) P⁻
//
//   h-step projection from filtered (x_t|t, P_t|t):
//     mean(t+h) = level_t|t + h × slope_t|t
//     var (t+h) = P[0,0] + 2h·P[0,1] + h²·P[1,1]
//               + h·Q_level + Q_slope · h(h-1)(2h-1)/6
//     [interval] = mean ± z(c)·√var
//
// 與 kalman_filter_core 區別:
//   kalman_filter_core 是 regime classifier(寫 indicator_values + facts)。
//   kalman_forecast_core 是 forecast generator(寫 forecast_log,本 v0.3 工作)。
//   兩者並存,職責不同。

use anyhow::Result;
use chrono::NaiveDate;
use fact_schema::{Fact, IndicatorCore, Timeframe};
use ohlcv_loader::OhlcvSeries;
use serde::Serialize;

inventory::submit! {
    core_registry::CoreRegistration::new(
        "kalman_forecast_core", "0.1.0", core_registry::CoreKind::Indicator, "P3",
        "Kalman LLT 區間預測(filtered state + analytical projection,v0.3 spine)",
    )
}

/// Default horizons(calendar days)— 對齊 v0.3 spec §forecast_log
pub const DEFAULT_HORIZONS_DAYS: &[u16] = &[21, 63, 126];

/// Default confidence levels — 對齊 v0.3 spec
pub const DEFAULT_CONFIDENCES: &[f64] = &[0.50, 0.80, 0.95];

#[derive(Debug, Clone, Serialize)]
pub struct KalmanForecastParams {
    pub timeframe: Timeframe,
    pub horizons_days: Vec<u16>,
    pub confidence_levels: Vec<f64>,
    /// Process noise for level component(LLT trend variance)。
    /// 0.0 = sentinel for adaptive(0.005·mean_price)²。
    pub q_level: f64,
    /// Process noise for slope component。
    /// 0.0 = sentinel for adaptive(0.0005·mean_price)²。
    pub q_slope: f64,
    /// Observation noise。
    /// 0.0 = sentinel for adaptive(0.01·mean_price)²。
    pub r: f64,
}

impl Default for KalmanForecastParams {
    fn default() -> Self {
        Self {
            timeframe: Timeframe::Daily,
            horizons_days: DEFAULT_HORIZONS_DAYS.to_vec(),
            confidence_levels: DEFAULT_CONFIDENCES.to_vec(),
            // v0.3 tuning(2026-05-23):全部用 0.0 sentinel,讓 R/Q 都 price-scale
            // adaptive。原 absolute defaults(1e-3 / 1e-5)對 $100 vs $2000 股
            // 不分尺度,raw band 過窄(production 觀察 sharpness 5.79 對 $2200 股
            // → 0.3% 寬,荒謬)。Adaptive:
            //   R       = (1.0% × mean_p)²    obs noise(spec §11)
            //   Q_level = (0.5% × mean_p)²    ~ R/4,讓 Kalman 仍平滑而非 pass-through
            //   Q_slope = (0.05% × mean_p)²   slope 變化緩慢,~Q_level/100
            // 各 magnitude 對齊典型股價 daily innovation。CQR phase 4 後仍會校準,
            // 但 raw band 越接近實際 → CQR q 越小 → 整體 sharpness 越好。
            q_level: 0.0,
            q_slope: 0.0,
            r: 0.0,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ForecastInterval {
    pub horizon_days: u16,
    pub confidence: f64,
    pub lower: f64,
    pub upper: f64,
    pub point: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct KalmanForecastOutput {
    pub stock_id: String,
    pub timeframe: Timeframe,
    /// Last bar of the input series — becomes the forecast_log.forecast_date
    pub forecast_date: NaiveDate,
    /// Final filtered state(diagnostic)
    pub final_level: f64,
    pub final_slope: f64,
    /// Final state covariance(diagnostic)
    pub p_00: f64,
    pub p_01: f64,
    pub p_11: f64,
    /// Effective process noise actually used(diagnostic)
    pub effective_q_level: f64,
    pub effective_q_slope: f64,
    /// Effective observation noise actually used(if adapted from data)
    pub effective_r: f64,
    /// All forecast intervals(horizon × confidence cartesian)
    pub forecasts: Vec<ForecastInterval>,
}

pub struct KalmanForecastCore;
impl KalmanForecastCore {
    pub fn new() -> Self {
        KalmanForecastCore
    }
}
impl Default for KalmanForecastCore {
    fn default() -> Self {
        KalmanForecastCore::new()
    }
}

/// One-pass LLT Kalman filter forward sweep(filtered state only).
///
/// Returns (level_t|t, slope_t|t, P_t|t as (p00, p01, p11),
///          effective_q_level, effective_q_slope, effective_r).
fn kalman_llt_filter(
    prices: &[f64],
    q_level_input: f64,
    q_slope_input: f64,
    r_input: f64,
) -> (f64, f64, (f64, f64, f64), f64, f64, f64) {
    debug_assert!(!prices.is_empty(), "kalman_llt_filter: empty input");

    // Price-scale adaptive R/Q if user gave 0(v0.3 tuning,2026-05-23):
    //   R       = (1.0% × mean_p)²    obs noise(spec §11)
    //   Q_level = (0.5% × mean_p)²    ~ R/4,讓 Kalman 仍平滑
    //   Q_slope = (0.05% × mean_p)²   slope 變化緩慢
    let mean_p = prices.iter().sum::<f64>() / prices.len() as f64;
    let r = if r_input > 0.0 {
        r_input
    } else {
        (0.01 * mean_p).powi(2).max(1e-9)
    };
    let q_level = if q_level_input > 0.0 {
        q_level_input
    } else {
        (0.005 * mean_p).powi(2).max(1e-9)
    };
    let q_slope = if q_slope_input > 0.0 {
        q_slope_input
    } else {
        (0.0005 * mean_p).powi(2).max(1e-11)
    };

    // Initial state: level = first observation, slope = 0
    let mut level = prices[0];
    let mut slope = 0.0;

    // Initial covariance: large (uninformative prior)
    let mut p00 = (prices[0].abs() * 0.1).max(1.0);
    let mut p01 = 0.0;
    let mut p11 = (prices[0].abs() * 0.01).max(0.01);

    for &z in prices.iter().skip(1) {
        // Predict step: F = [[1,1],[0,1]]
        let level_pred = level + slope;
        let slope_pred = slope;
        // P⁻ = F P Fᵀ + Q
        //     = [[p00 + 2·p01 + p11, p01 + p11],
        //        [p01 + p11,        p11      ]] + diag(Q_level, Q_slope)
        let p00_pred = p00 + 2.0 * p01 + p11 + q_level;
        let p01_pred = p01 + p11;
        let p11_pred = p11 + q_slope;

        // Update step:
        // Innovation:  y = z - H x⁻ = z - level_pred
        // S = H P⁻ Hᵀ + R = p00_pred + R
        // K = P⁻ Hᵀ / S = [p00_pred/S, p01_pred/S]
        let s = p00_pred + r;
        let k0 = p00_pred / s;
        let k1 = p01_pred / s;
        let innovation = z - level_pred;

        level = level_pred + k0 * innovation;
        slope = slope_pred + k1 * innovation;

        // P = (I - K H) P⁻
        //   = [[(1-k0)·p00_pred,  (1-k0)·p01_pred],
        //      [-k1·p00_pred + p01_pred,  -k1·p01_pred + p11_pred]]
        // Symmetric form preferred:
        let new_p00 = (1.0 - k0) * p00_pred;
        let new_p01 = (1.0 - k0) * p01_pred;
        let new_p11 = p11_pred - k1 * p01_pred;
        p00 = new_p00;
        p01 = new_p01;
        p11 = new_p11;
    }

    (level, slope, (p00, p01, p11), q_level, q_slope, r)
}

/// Project h-step ahead from filtered state(x_t|t, P_t|t).
///
/// Returns (mean, variance) for the level component at t+h.
///
/// LLT formulas:
///   mean(t+h) = level + h · slope
///   var (t+h) = p00 + 2h·p01 + h²·p11
///             + h·Q_level + Q_slope · h(h-1)(2h-1)/6
fn project_h_steps(
    level: f64,
    slope: f64,
    p00: f64,
    p01: f64,
    p11: f64,
    q_level: f64,
    q_slope: f64,
    h: u32,
) -> (f64, f64) {
    let hh = h as f64;
    let mean = level + hh * slope;

    // Closed-form variance accumulation
    let trend_var = p00 + 2.0 * hh * p01 + hh * hh * p11;

    // Process noise accumulation
    // sum_{i=0..h-1} i² = (h-1)·h·(2h-1) / 6
    let slope_acc = if h > 0 {
        let sum_i_sq = ((h as f64 - 1.0) * hh * (2.0 * hh - 1.0)) / 6.0;
        q_slope * sum_i_sq
    } else {
        0.0
    };
    let level_acc = hh * q_level;

    let var = trend_var + level_acc + slope_acc;
    (mean, var.max(1e-12))
}

/// Inverse-normal CDF (probit) for two-sided confidence interval.
///
/// Returns z such that P(|N(0,1)| ≤ z) = confidence.
/// Uses standard Acklam (2003) approximation, accurate to ~1.15e-9.
fn confidence_to_z(confidence: f64) -> f64 {
    // For two-sided c, we need quantile at p = 0.5 + c/2
    let p = 0.5 + confidence.clamp(0.001, 0.999) / 2.0;
    inverse_normal_cdf(p)
}

/// Acklam 2003 — inverse of standard normal CDF.
fn inverse_normal_cdf(p: f64) -> f64 {
    // Coefficients
    const A: [f64; 6] = [
        -3.969_683_028_665_376e+1,
         2.209_460_984_245_205e+2,
        -2.759_285_104_469_687e+2,
         1.383_577_518_672_690e+2,
        -3.066_479_806_614_716e+1,
         2.506_628_277_459_239e+0,
    ];
    const B: [f64; 5] = [
        -5.447_609_879_822_406e+1,
         1.615_858_368_580_409e+2,
        -1.556_989_798_598_866e+2,
         6.680_131_188_771_972e+1,
        -1.328_068_155_288_572e+1,
    ];
    const C: [f64; 6] = [
        -7.784_894_002_430_293e-3,
        -3.223_964_580_411_365e-1,
        -2.400_758_277_161_838e+0,
        -2.549_732_539_343_734e+0,
         4.374_664_141_464_968e+0,
         2.938_163_982_698_783e+0,
    ];
    const D: [f64; 4] = [
         7.784_695_709_041_462e-3,
         3.224_671_290_700_398e-1,
         2.445_134_137_142_996e+0,
         3.754_408_661_907_416e+0,
    ];
    const P_LOW: f64 = 0.02425;
    const P_HIGH: f64 = 1.0 - P_LOW;

    if p < P_LOW {
        let q = (-2.0 * p.ln()).sqrt();
        (((((C[0]*q+C[1])*q+C[2])*q+C[3])*q+C[4])*q+C[5])
            / ((((D[0]*q+D[1])*q+D[2])*q+D[3])*q+1.0)
    } else if p <= P_HIGH {
        let q = p - 0.5;
        let r = q * q;
        (((((A[0]*r+A[1])*r+A[2])*r+A[3])*r+A[4])*r+A[5]) * q
            / (((((B[0]*r+B[1])*r+B[2])*r+B[3])*r+B[4])*r+1.0)
    } else {
        let q = (-2.0 * (1.0 - p).ln()).sqrt();
        -(((((C[0]*q+C[1])*q+C[2])*q+C[3])*q+C[4])*q+C[5])
            / ((((D[0]*q+D[1])*q+D[2])*q+D[3])*q+1.0)
    }
}

impl IndicatorCore for KalmanForecastCore {
    type Input = OhlcvSeries;
    type Params = KalmanForecastParams;
    type Output = KalmanForecastOutput;

    fn name(&self) -> &'static str {
        "kalman_forecast_core"
    }

    fn version(&self) -> &'static str {
        "0.1.0"
    }

    /// Minimum bars to fit LLT + project meaningfully:
    /// Need enough history for filter to converge + cover longest horizon.
    /// Daily 30 trading bars(~6 weeks)is conservative for LLT convergence;
    /// loader will fetch 6× this for safety(對齊 §7.3 1.2× warmup convention).
    fn warmup_periods(&self, _params: &Self::Params) -> usize {
        30
    }

    fn compute(&self, input: &Self::Input, params: Self::Params) -> Result<Self::Output> {
        if input.bars.is_empty() {
            anyhow::bail!("kalman_forecast_core: empty input series for {}", input.stock_id);
        }
        if input.bars.len() < self.warmup_periods(&params) {
            anyhow::bail!(
                "kalman_forecast_core: insufficient bars ({}) for {} (need ≥ {})",
                input.bars.len(),
                input.stock_id,
                self.warmup_periods(&params)
            );
        }

        let prices: Vec<f64> = input.bars.iter().map(|b| b.close).collect();
        let last_bar = input.bars.last().unwrap();

        let (level, slope, (p00, p01, p11), effective_q_level, effective_q_slope, effective_r) =
            kalman_llt_filter(&prices, params.q_level, params.q_slope, params.r);

        // Multi-horizon × multi-confidence cartesian
        let mut forecasts = Vec::with_capacity(
            params.horizons_days.len() * params.confidence_levels.len(),
        );
        for &h_days in &params.horizons_days {
            // Convert calendar days to filter-step count.  Spec uses calendar
            // days(21 / 63 / 126)but the Kalman filter steps in trading bars
            // (~1.4 calendar days per trading bar on a 5-day week, but for
            // forecast horizons we approximate 1 step = 1 calendar day).  The
            // forecast_date will be forecast_date(last bar)— horizon_days
            // remains the spec-aligned calendar day count.
            //
            // For the projection we use h ≈ h_days × (trading bars / calendar)
            // ≈ h_days × 5/7.  This makes the analytical variance scale
            // correctly with trading-bar units.
            let h_steps = ((h_days as f64) * 5.0 / 7.0).round().max(1.0) as u32;
            // Use EFFECTIVE Q values (adapted) for projection variance, not raw params
            // (which could be 0 sentinels)
            let (mean, var) = project_h_steps(
                level, slope, p00, p01, p11, effective_q_level, effective_q_slope, h_steps,
            );
            let std = var.sqrt();
            for &c in &params.confidence_levels {
                let z = confidence_to_z(c);
                let half_width = z * std;
                forecasts.push(ForecastInterval {
                    horizon_days: h_days,
                    confidence: c,
                    lower: round_4(mean - half_width),
                    upper: round_4(mean + half_width),
                    point: round_4(mean),
                });
            }
        }

        Ok(KalmanForecastOutput {
            stock_id: input.stock_id.clone(),
            timeframe: input.timeframe,
            forecast_date: last_bar.date,
            final_level: level,
            final_slope: slope,
            p_00: p00,
            p_01: p01,
            p_11: p11,
            effective_q_level,
            effective_q_slope,
            effective_r,
            forecasts,
        })
    }

    /// 不寫 facts 表 — 預測寫 forecast_log(走 dispatch_forecast → write_forecast_log)。
    fn produce_facts(&self, _output: &Self::Output) -> Vec<Fact> {
        Vec::new()
    }
}

fn round_4(v: f64) -> f64 {
    (v * 10000.0).round() / 10000.0
}

// ─── Unit tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use ohlcv_loader::OhlcvBar;

    fn make_series(closes: &[f64]) -> OhlcvSeries {
        let bars = closes
            .iter()
            .enumerate()
            .map(|(i, &c)| OhlcvBar {
                date: NaiveDate::from_ymd_opt(2024, 1, 1).unwrap()
                    + chrono::Duration::days(i as i64),
                open: c,
                high: c,
                low: c,
                close: c,
                volume: Some(1000),
            })
            .collect();
        OhlcvSeries {
            stock_id: "2330".to_string(),
            timeframe: Timeframe::Daily,
            bars,
        }
    }

    #[test]
    fn flat_series_zero_slope_constant_level() {
        let s = make_series(&vec![100.0; 100]);
        let core = KalmanForecastCore::new();
        let out = core.compute(&s, KalmanForecastParams::default()).unwrap();
        // For perfectly flat series, slope should converge to ≈ 0
        assert!(
            out.final_slope.abs() < 0.5,
            "expected slope ~0 for flat series, got {}",
            out.final_slope
        );
        // Level should converge near 100
        assert!(
            (out.final_level - 100.0).abs() < 5.0,
            "expected level ~100, got {}",
            out.final_level
        );
        // Point forecast at any horizon should be near 100
        for f in &out.forecasts {
            assert!(
                (f.point - 100.0).abs() < 10.0,
                "horizon={} confidence={} point={}",
                f.horizon_days, f.confidence, f.point
            );
        }
    }

    #[test]
    fn linear_trend_picks_up_slope() {
        // closes go up by 1.0 every day for 100 days
        let closes: Vec<f64> = (0..100).map(|i| 100.0 + i as f64).collect();
        let s = make_series(&closes);
        let core = KalmanForecastCore::new();
        let out = core.compute(&s, KalmanForecastParams::default()).unwrap();
        assert!(
            out.final_slope > 0.5,
            "expected positive slope, got {}",
            out.final_slope
        );
        // 21-day point forecast should be above current level
        let f21 = out
            .forecasts
            .iter()
            .find(|f| f.horizon_days == 21 && (f.confidence - 0.5).abs() < 1e-6)
            .unwrap();
        assert!(
            f21.point > out.final_level,
            "21d projection {} should exceed last level {}",
            f21.point, out.final_level
        );
    }

    #[test]
    fn confidence_widens_interval() {
        let s = make_series(&vec![100.0; 100]);
        let out = KalmanForecastCore::new()
            .compute(&s, KalmanForecastParams::default())
            .unwrap();
        // For same horizon, c=0.95 width > c=0.80 width > c=0.50 width
        let f50 = out
            .forecasts
            .iter()
            .find(|f| f.horizon_days == 21 && (f.confidence - 0.50).abs() < 1e-6)
            .unwrap();
        let f80 = out
            .forecasts
            .iter()
            .find(|f| f.horizon_days == 21 && (f.confidence - 0.80).abs() < 1e-6)
            .unwrap();
        let f95 = out
            .forecasts
            .iter()
            .find(|f| f.horizon_days == 21 && (f.confidence - 0.95).abs() < 1e-6)
            .unwrap();
        assert!(f80.upper - f80.lower > f50.upper - f50.lower);
        assert!(f95.upper - f95.lower > f80.upper - f80.lower);
    }

    #[test]
    fn horizon_widens_interval() {
        let s = make_series(&vec![100.0; 100]);
        let out = KalmanForecastCore::new()
            .compute(&s, KalmanForecastParams::default())
            .unwrap();
        let h21 = out
            .forecasts
            .iter()
            .find(|f| f.horizon_days == 21 && (f.confidence - 0.80).abs() < 1e-6)
            .unwrap();
        let h126 = out
            .forecasts
            .iter()
            .find(|f| f.horizon_days == 126 && (f.confidence - 0.80).abs() < 1e-6)
            .unwrap();
        assert!(
            h126.upper - h126.lower > h21.upper - h21.lower,
            "longer horizon should widen interval"
        );
    }

    #[test]
    fn empty_input_errors() {
        let s = OhlcvSeries {
            stock_id: "X".to_string(),
            timeframe: Timeframe::Daily,
            bars: vec![],
        };
        let res = KalmanForecastCore::new().compute(&s, KalmanForecastParams::default());
        assert!(res.is_err());
    }

    #[test]
    fn produce_facts_returns_empty() {
        let s = make_series(&vec![100.0; 100]);
        let core = KalmanForecastCore::new();
        let out = core.compute(&s, KalmanForecastParams::default()).unwrap();
        assert_eq!(core.produce_facts(&out).len(), 0);
    }

    #[test]
    fn adaptive_q_scales_with_price() {
        // Two flat series at different price scales:
        // p=100 stock vs p=2000 stock should get Q values 400x apart
        // (since Q ∝ p²)
        let s_low = make_series(&vec![100.0; 200]);
        let s_high = make_series(&vec![2000.0; 200]);
        let core = KalmanForecastCore::new();
        let out_low = core.compute(&s_low, KalmanForecastParams::default()).unwrap();
        let out_high = core.compute(&s_high, KalmanForecastParams::default()).unwrap();
        // q_level should scale as (0.005 × p)² → 400x for 20x price ratio
        let ratio = out_high.effective_q_level / out_low.effective_q_level;
        assert!(
            (ratio - 400.0).abs() < 1.0,
            "expected ~400x scaling, got {}",
            ratio
        );
        // Similar for r and q_slope
        assert!((out_high.effective_r / out_low.effective_r - 400.0).abs() < 1.0);
        assert!((out_high.effective_q_slope / out_low.effective_q_slope - 400.0).abs() < 1.0);
    }

    #[test]
    fn explicit_q_overrides_adaptive() {
        // User-provided non-zero Q should be used, not adapted
        let s = make_series(&vec![100.0; 100]);
        let mut params = KalmanForecastParams::default();
        params.q_level = 12.34;
        params.q_slope = 5.67;
        params.r = 89.0;
        let out = KalmanForecastCore::new().compute(&s, params).unwrap();
        assert_eq!(out.effective_q_level, 12.34);
        assert_eq!(out.effective_q_slope, 5.67);
        assert_eq!(out.effective_r, 89.0);
    }

    #[test]
    fn confidence_to_z_matches_known_quantiles() {
        // z(0.95) ≈ 1.959964
        assert!((confidence_to_z(0.95) - 1.959964).abs() < 1e-3);
        // z(0.80) ≈ 1.281552
        assert!((confidence_to_z(0.80) - 1.281552).abs() < 1e-3);
        // z(0.50) ≈ 0.674490
        assert!((confidence_to_z(0.50) - 0.674490).abs() < 1e-3);
    }

    #[test]
    fn point_within_interval() {
        // Property test: lower ≤ point ≤ upper for all forecasts
        let closes: Vec<f64> = (0..100).map(|i| 100.0 + (i as f64).sin() * 5.0).collect();
        let s = make_series(&closes);
        let out = KalmanForecastCore::new()
            .compute(&s, KalmanForecastParams::default())
            .unwrap();
        for f in &out.forecasts {
            assert!(
                f.lower <= f.point && f.point <= f.upper,
                "interval violated: [{}, {}] point={} (h={}, c={})",
                f.lower, f.upper, f.point, f.horizon_days, f.confidence
            );
        }
    }
}
