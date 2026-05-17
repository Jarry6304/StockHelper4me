// foreign_holding_core(P2)— Chip Core
//
// 對齊 m3Spec/chip_cores.md §五 foreign_holding_core(外資持股比率)。
//
// **本 PR 範圍**:
//   - 完整 ForeignHoldingParams + Output + 4 EventKind(對齊 §5.5)
//   - compute():逐筆組 series + day-over-day change_pct
//   - LimitNearAlert / SignificantSingleDayChange threshold-based
//   - HoldingMilestoneHigh / Low(N 期最高 / 最低,使用 series 內 lookback)
//
// foreign_limit_pct(2026-05-10 commit 458a45a 解):chip_loader 從 Silver
// `foreign_holding_derived.detail->>'upper_limit_ratio'` 取;LimitNearAlert
// 在 Bronze 有料時觸發,無料時 limit_pct=0 → 略過(line 147 防衛條件)。
// Reference(2026-05-12 校準): George & Hwang (2004) JF 59(5):2145-2176 — 52-week high
// 動能指標以 252 交易日（年新高）為 lookback 標準。此處同時保留季新高（60d）與
// 年新高（252d）兩種語意，各自對應一組 EventKind。

use anyhow::Result;
use chip_loader::ForeignHoldingSeries;
use chrono::NaiveDate;
use fact_schema::{Fact, IndicatorCore, Timeframe};
use serde::Serialize;
use serde_json::json;

inventory::submit! {
    core_registry::CoreRegistration::new(
        "foreign_holding_core",
        "0.1.0",
        core_registry::CoreKind::Chip,
        "P2",
        "Foreign Holding Core(外資持股比率變化 / 接近上限警訊)",
    )
}

const MILESTONE_LOOKBACK_QUARTERLY: usize = 60;
const MILESTONE_LOOKBACK_ANNUAL: usize = 252;

/// Milestone events 最小間距(交易日),預設 5(~1 週)
/// Reference(2026-05-16 v3.15 Round 8 → v3.16 Round 8.1): Lucas & LeBeau (1992)
/// "Technical Traders Guide to Computer Analysis" Ch.7 — pivot 確認需要 N-bar holding 期間。
/// v3.15(2026-05-16): production data 揭露 HoldingMilestoneLow 觸發率 15.46/yr/stock 超 v1.32
/// ≤ 12/yr/stock 標準;首試 spacing=10 對稱套 4 variants。
/// v3.16 Round 8.1(2026-05-17): spacing=10 過嚴 — 4 variants 全 collapse 到 1.5-4/yr
/// (Low 3.97,High 3.26,LowAnn 2.03,HighAnn 1.49)。production-data-driven 觀察:
/// 台股外資持股 cluster 平均 ≈ 4-event(連續探低/探高 monotonic drift),spacing=10
/// 達 25% retention(過嚴 2×)。縮 5 達 ~50% retention,Low 預期 ~7-9/yr ✅。
const MIN_MILESTONE_SPACING_DAYS: usize = 5;

#[derive(Debug, Clone, Serialize)]
pub struct ForeignHoldingParams {
    pub timeframe: Timeframe,
    /// SignificantSingleDayChange 的 rolling z-score 閾值,預設 2.1
    /// Reference(2026-05-12 P2 阻塞 6): Fama, Fisher, Jensen & Roll (1969) IER 10(1):1-21
    /// 現代事件研究方法論 —「顯著」= 超過個股歷史波動度 2σ,而非跨股票固定百分比閾值。
    /// v3.15 Round 8(2026-05-16): production data 揭露 z=2.0 觸發率 12.88/yr/stock
    /// 微超 v1.32 ≤ 12/yr/stock 標準;tighten 2.0→2.1(97.86th percentile)後預期 ~10/yr。
    pub change_z_threshold: f64,
    /// rolling z-score 的回看窗口,預設 60 天
    pub change_lookback: usize,
    /// LimitNearAlert 剩餘空間 threshold(%),預設 5.0
    pub limit_alert_remaining: f64,
}

impl Default for ForeignHoldingParams {
    fn default() -> Self {
        Self {
            timeframe: Timeframe::Daily,
            change_z_threshold: 2.1,
            change_lookback: 60,
            limit_alert_remaining: 5.0,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ForeignHoldingOutput {
    pub stock_id: String,
    pub timeframe: Timeframe,
    pub series: Vec<ForeignHoldingPoint>,
    pub events: Vec<ForeignHoldingEvent>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ForeignHoldingPoint {
    pub date: NaiveDate,
    pub foreign_holding_pct: f64,
    pub foreign_limit_pct: f64,
    pub remaining_pct: f64,
    pub change_pct: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ForeignHoldingEvent {
    pub date: NaiveDate,
    pub kind: ForeignHoldingEventKind,
    pub value: f64,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum ForeignHoldingEventKind {
    HoldingMilestoneHigh,        // 60d 季新高
    HoldingMilestoneLow,         // 60d 季新低
    HoldingMilestoneHighAnnual,  // 252d 年新高（George & Hwang 2004 標準）
    HoldingMilestoneLowAnnual,   // 252d 年新低
    LimitNearAlert,
    SignificantSingleDayChange,
}

pub struct ForeignHoldingCore;

impl ForeignHoldingCore { pub fn new() -> Self { ForeignHoldingCore } }
impl Default for ForeignHoldingCore { fn default() -> Self { ForeignHoldingCore::new() } }

impl IndicatorCore for ForeignHoldingCore {
    type Input = ForeignHoldingSeries;
    type Params = ForeignHoldingParams;
    type Output = ForeignHoldingOutput;

    fn name(&self) -> &'static str { "foreign_holding_core" }
    fn version(&self) -> &'static str { "0.1.0" }

    fn compute(&self, input: &Self::Input, params: Self::Params) -> Result<Self::Output> {
        let mut series = Vec::with_capacity(input.points.len());
        let mut prev_pct: Option<f64> = None;
        for p in &input.points {
            let holding_pct = p.foreign_holding_ratio.unwrap_or(0.0);
            let limit_pct = p.foreign_limit_pct.unwrap_or(0.0);
            let remaining = (limit_pct - holding_pct).max(0.0);
            let change = match prev_pct {
                Some(pp) => holding_pct - pp,
                None => 0.0,
            };
            series.push(ForeignHoldingPoint {
                date: p.date,
                foreign_holding_pct: holding_pct,
                foreign_limit_pct: limit_pct,
                remaining_pct: remaining,
                change_pct: change,
            });
            prev_pct = Some(holding_pct);
        }
        let events = detect_events(&series, &params);
        Ok(ForeignHoldingOutput {
            stock_id: input.stock_id.clone(),
            timeframe: params.timeframe,
            series,
            events,
        })
    }

    fn produce_facts(&self, output: &Self::Output) -> Vec<Fact> {
        output.events.iter().map(|e| event_to_fact(output, e)).collect()
    }

    fn warmup_periods(&self, params: &Self::Params) -> usize { params.change_lookback.max(20) }
}

fn mean_std_f64(values: &[f64]) -> (f64, f64) {
    if values.is_empty() { return (0.0, 0.0); }
    let n = values.len() as f64;
    let mean = values.iter().sum::<f64>() / n;
    let var = values.iter().map(|&x| { let d = x - mean; d * d }).sum::<f64>() / n;
    (mean, var.sqrt())
}

fn detect_events(series: &[ForeignHoldingPoint], params: &ForeignHoldingParams) -> Vec<ForeignHoldingEvent> {
    let mut events = Vec::new();
    let mut was_near_limit = false;
    // v3.15 Round 8(2026-05-16): MIN_MILESTONE_SPACING_DAYS 控連續探低/探高 cluster
    // 視為同一事件;對齊 Lucas & LeBeau (1992) pivot 確認 N-bar holding。
    let mut last_quarterly_high_idx: Option<usize> = None;
    let mut last_quarterly_low_idx: Option<usize> = None;
    let mut last_annual_high_idx: Option<usize> = None;
    let mut last_annual_low_idx: Option<usize> = None;
    for (i, p) in series.iter().enumerate() {
        // SignificantSingleDayChange — rolling z-score(2026-05-12 P2 阻塞 6 修)
        // 原固定 0.5% 閾值不適應個股波動度(大型股 0.5%/day 是常態)→ 34.87/yr 噪音。
        // 改用個股 60-day rolling mean+std,|z| >= 2.0 才 fire。目標 10–15/yr 🟢。
        // Verification: scripts/p2_calibration_data.sql §2 (foreign_holding_core / SignificantSingleDayChange)。
        // Reference: Fama, Fisher, Jensen & Roll (1969) IER 10(1):1-21 — 顯著事件 = 個股歷史 2σ 標準；
        //            Brown & Warner (1985) JFE 14:3-31 — rolling estimation window 方法論。
        if i >= params.change_lookback {
            let window: Vec<f64> = series[i - params.change_lookback..i]
                .iter()
                .map(|q| q.change_pct)
                .collect();
            let (mean, std) = mean_std_f64(&window);
            if std > 0.0 {
                let z = (p.change_pct - mean) / std;
                if z.abs() >= params.change_z_threshold {
                    events.push(ForeignHoldingEvent {
                        date: p.date,
                        kind: ForeignHoldingEventKind::SignificantSingleDayChange,
                        value: p.change_pct,
                        metadata: json!({
                            "change": p.change_pct,
                            "z_score": z,
                            "lookback": params.change_lookback,
                        }),
                    });
                }
            }
        }
        // LimitNearAlert — edge trigger(2026-05-12 P2 阻塞 6 修)
        // 原 level trigger 每天 remaining <= 5% 都 fire(50.06/yr 噪音)→ 改為僅在
        // 「進入 near-limit zone」當日 fire。一年進出 zone 約 2-6 次,匹配真實訊號頻率。
        // Verification: scripts/p2_calibration_data.sql §2 (foreign_holding_core / LimitNearAlert)。
        // Reference: Sheingold (1978) "Analog-Digital Conversion Notes" — edge trigger vs level trigger。
        let is_near_limit = p.foreign_limit_pct > 0.0
            && p.remaining_pct > 0.0
            && p.remaining_pct <= params.limit_alert_remaining;
        if is_near_limit && !was_near_limit {
            events.push(ForeignHoldingEvent {
                date: p.date,
                kind: ForeignHoldingEventKind::LimitNearAlert,
                value: p.foreign_holding_pct,
                metadata: json!({
                    "holding": p.foreign_holding_pct,
                    "limit": p.foreign_limit_pct,
                    "remaining": p.remaining_pct,
                    "transition": "entering",
                }),
            });
        }
        was_near_limit = is_near_limit;
        // Milestone high / low — 季新高/低（60d）+ v3.15 MIN_MILESTONE_SPACING_DAYS spacing
        if i >= MILESTONE_LOOKBACK_QUARTERLY {
            let window = &series[i - MILESTONE_LOOKBACK_QUARTERLY..i];
            let max_prev = window.iter().map(|q| q.foreign_holding_pct).fold(f64::NEG_INFINITY, f64::max);
            let min_prev = window.iter().map(|q| q.foreign_holding_pct).fold(f64::INFINITY, f64::min);
            if p.foreign_holding_pct > max_prev
                && last_quarterly_high_idx.map_or(true, |last| i - last >= MIN_MILESTONE_SPACING_DAYS)
            {
                events.push(ForeignHoldingEvent {
                    date: p.date,
                    kind: ForeignHoldingEventKind::HoldingMilestoneHigh,
                    value: p.foreign_holding_pct,
                    metadata: json!({ "lookback": format!("{}d", MILESTONE_LOOKBACK_QUARTERLY), "value": p.foreign_holding_pct }),
                });
                last_quarterly_high_idx = Some(i);
            } else if p.foreign_holding_pct < min_prev
                && last_quarterly_low_idx.map_or(true, |last| i - last >= MIN_MILESTONE_SPACING_DAYS)
            {
                events.push(ForeignHoldingEvent {
                    date: p.date,
                    kind: ForeignHoldingEventKind::HoldingMilestoneLow,
                    value: p.foreign_holding_pct,
                    metadata: json!({ "lookback": format!("{}d", MILESTONE_LOOKBACK_QUARTERLY), "value": p.foreign_holding_pct }),
                });
                last_quarterly_low_idx = Some(i);
            }
        }
        // Milestone high / low — 年新高/低（252d，George & Hwang 2004 標準）+ v3.15 spacing
        if i >= MILESTONE_LOOKBACK_ANNUAL {
            let window = &series[i - MILESTONE_LOOKBACK_ANNUAL..i];
            let max_prev = window.iter().map(|q| q.foreign_holding_pct).fold(f64::NEG_INFINITY, f64::max);
            let min_prev = window.iter().map(|q| q.foreign_holding_pct).fold(f64::INFINITY, f64::min);
            if p.foreign_holding_pct > max_prev
                && last_annual_high_idx.map_or(true, |last| i - last >= MIN_MILESTONE_SPACING_DAYS)
            {
                events.push(ForeignHoldingEvent {
                    date: p.date,
                    kind: ForeignHoldingEventKind::HoldingMilestoneHighAnnual,
                    value: p.foreign_holding_pct,
                    metadata: json!({ "lookback": format!("{}d", MILESTONE_LOOKBACK_ANNUAL), "value": p.foreign_holding_pct }),
                });
                last_annual_high_idx = Some(i);
            } else if p.foreign_holding_pct < min_prev
                && last_annual_low_idx.map_or(true, |last| i - last >= MIN_MILESTONE_SPACING_DAYS)
            {
                events.push(ForeignHoldingEvent {
                    date: p.date,
                    kind: ForeignHoldingEventKind::HoldingMilestoneLowAnnual,
                    value: p.foreign_holding_pct,
                    metadata: json!({ "lookback": format!("{}d", MILESTONE_LOOKBACK_ANNUAL), "value": p.foreign_holding_pct }),
                });
                last_annual_low_idx = Some(i);
            }
        }
    }
    events
}

fn event_to_fact(output: &ForeignHoldingOutput, e: &ForeignHoldingEvent) -> Fact {
    let statement = match e.kind {
        ForeignHoldingEventKind::HoldingMilestoneHigh => format!(
            "Foreign holding 60d high at {:.2}% on {}", e.value, e.date
        ),
        ForeignHoldingEventKind::HoldingMilestoneLow => format!(
            "Foreign holding 60d low at {:.2}% on {}", e.value, e.date
        ),
        ForeignHoldingEventKind::HoldingMilestoneHighAnnual => format!(
            "Foreign holding 252d high at {:.2}% on {}", e.value, e.date
        ),
        ForeignHoldingEventKind::HoldingMilestoneLowAnnual => format!(
            "Foreign holding 252d low at {:.2}% on {}", e.value, e.date
        ),
        ForeignHoldingEventKind::LimitNearAlert => format!(
            "Foreign holding reached {:.2}% on {}, near {:.2}% limit",
            e.metadata["holding"].as_f64().unwrap_or(0.0),
            e.date,
            e.metadata["limit"].as_f64().unwrap_or(0.0)
        ),
        ForeignHoldingEventKind::SignificantSingleDayChange => format!(
            "Foreign holding {} {:.2}% in single day on {}",
            if e.value >= 0.0 { "rose" } else { "dropped" },
            e.value.abs(),
            e.date
        ),
    };
    Fact {
        stock_id: output.stock_id.clone(),
        fact_date: e.date,
        timeframe: output.timeframe,
        source_core: "foreign_holding_core".to_string(),
        source_version: "0.1.0".to_string(),
        params_hash: None,
        statement,
        metadata: fact_schema::with_event_kind(e.metadata.clone(), &e.kind),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chip_loader::ForeignHoldingRaw;

    fn raw(d: &str, ratio: f64) -> ForeignHoldingRaw {
        ForeignHoldingRaw {
            date: NaiveDate::parse_from_str(d, "%Y-%m-%d").unwrap(),
            foreign_holding_shares: Some(1_000_000),
            foreign_holding_ratio: Some(ratio),
            foreign_limit_pct: None,
        }
    }

    fn raw_with_limit(d: &str, ratio: f64, limit: f64) -> ForeignHoldingRaw {
        ForeignHoldingRaw {
            date: NaiveDate::parse_from_str(d, "%Y-%m-%d").unwrap(),
            foreign_holding_shares: Some(1_000_000),
            foreign_holding_ratio: Some(ratio),
            foreign_limit_pct: Some(limit),
        }
    }

    /// SignificantSingleDayChange rolling z-score(2026-05-12 P2 阻塞 6):
    /// 60 baseline 日變化 ±0.05%(std 約 0.05),第 61 天變化 +1.0% → z ≈ 20 → fire。
    #[test]
    fn significant_change_rolling_z_triggers_on_spike() {
        let mut points = Vec::with_capacity(62);
        let mut ratio = 50.0;
        for i in 0..61 {
            let delta = if i % 2 == 0 { 0.05 } else { -0.05 };
            ratio += delta;
            let date = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap()
                + chrono::Duration::days(i as i64);
            let mut r = raw("2026-01-01", ratio);
            r.date = date;
            points.push(r);
        }
        // spike +1.0%
        ratio += 1.0;
        let spike_date = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap()
            + chrono::Duration::days(61);
        let mut r = raw("2026-01-01", ratio);
        r.date = spike_date;
        points.push(r);

        let series = ForeignHoldingSeries { stock_id: "2330".to_string(), points };
        let out = ForeignHoldingCore::new()
            .compute(&series, ForeignHoldingParams::default())
            .unwrap();
        let sig: Vec<_> = out.events.iter()
            .filter(|e| e.kind == ForeignHoldingEventKind::SignificantSingleDayChange)
            .collect();
        assert!(!sig.is_empty(), "spike 應觸發 SignificantSingleDayChange");
        assert_eq!(sig.last().unwrap().date, spike_date);
    }

    /// rolling z-score 不在常態日變化內觸發(舊版固定 0.5% 會誤觸大型股日常波動)。
    #[test]
    fn significant_change_no_false_positive_on_normal_volatility() {
        // 100 天日變化 ±0.3%(大型股常態),不該觸發
        let mut points = Vec::with_capacity(100);
        let mut ratio = 70.0;
        for i in 0..100 {
            let delta = if i % 2 == 0 { 0.3 } else { -0.3 };
            ratio += delta;
            let date = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap()
                + chrono::Duration::days(i as i64);
            let mut r = raw("2026-01-01", ratio);
            r.date = date;
            points.push(r);
        }
        let series = ForeignHoldingSeries { stock_id: "2330".to_string(), points };
        let out = ForeignHoldingCore::new()
            .compute(&series, ForeignHoldingParams::default())
            .unwrap();
        let sig: Vec<_> = out.events.iter()
            .filter(|e| e.kind == ForeignHoldingEventKind::SignificantSingleDayChange)
            .collect();
        assert!(sig.is_empty(),
            "日變化 0.3% 是個股常態(z ≈ 0)不該觸發,實際 fire {} 次", sig.len());
    }

    /// LimitNearAlert edge trigger(2026-05-12 P2 阻塞 6):連續 30 天在 near-limit
    /// zone 只 fire 1 次(進入當日),不再每天 fire。
    #[test]
    fn limit_near_alert_edge_trigger_no_duplicate_fire() {
        let mut points = Vec::with_capacity(40);
        // 前 10 天 holding 60%(離 limit 75% 還很遠,is_near=false)
        for i in 0..10 {
            let mut r = raw_with_limit("2026-01-01", 60.0, 75.0);
            r.date = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap()
                + chrono::Duration::days(i as i64);
            points.push(r);
        }
        // 後 30 天 holding 72%(remaining=3% <= 5%,is_near=true,持續 30 天)
        for i in 10..40 {
            let mut r = raw_with_limit("2026-01-01", 72.0, 75.0);
            r.date = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap()
                + chrono::Duration::days(i as i64);
            points.push(r);
        }
        let series = ForeignHoldingSeries { stock_id: "2330".to_string(), points };
        let out = ForeignHoldingCore::new()
            .compute(&series, ForeignHoldingParams::default())
            .unwrap();
        let alerts: Vec<_> = out.events.iter()
            .filter(|e| e.kind == ForeignHoldingEventKind::LimitNearAlert)
            .collect();
        assert_eq!(alerts.len(), 1,
            "edge trigger 連續 30 天 near-limit 應只 fire 1 次,實際 {} 次", alerts.len());
        assert_eq!(alerts[0].date,
            NaiveDate::from_ymd_opt(2026, 1, 1).unwrap() + chrono::Duration::days(10),
            "fire date 應為進入 near-limit zone 當日");
    }

    /// v3.16 Round 8.1(2026-05-17):MIN_MILESTONE_SPACING_DAYS=5 防連續探低 cluster。
    /// 連續 4 天每日新低(在 spacing=5 window 內),應只 fire 1 次(進入新低當日)。
    #[test]
    fn milestone_spacing_prevents_consecutive_low_fires() {
        let mut points = Vec::with_capacity(80);
        let base = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        // baseline 60 天 @ 50%
        for i in 0..60 {
            let mut r = raw("2026-01-01", 50.0);
            r.date = base + chrono::Duration::days(i);
            points.push(r);
        }
        // 連續 4 天每日新低 50→49.5→49.0→48.5→48.0(壓進 spacing=5 內)
        let mut ratio = 50.0;
        for i in 60..64 {
            ratio -= 0.5;
            let mut r = raw("2026-01-01", ratio);
            r.date = base + chrono::Duration::days(i as i64);
            points.push(r);
        }
        let series = ForeignHoldingSeries { stock_id: "2330".to_string(), points };
        let out = ForeignHoldingCore::new()
            .compute(&series, ForeignHoldingParams::default())
            .unwrap();
        let lows: Vec<_> = out.events.iter()
            .filter(|e| e.kind == ForeignHoldingEventKind::HoldingMilestoneLow)
            .collect();
        assert_eq!(
            lows.len(),
            1,
            "MIN_MILESTONE_SPACING_DAYS=5 連續 4 天探低應只 fire 1 次(實際 {} 次)",
            lows.len()
        );
        // 第一次 fire 應在第 60 天(進入新低當日)
        assert_eq!(lows[0].date, base + chrono::Duration::days(60));
    }

    /// v3.16 Round 8.1:spacing 過後可再次 fire(隔 >= 5 trading day 的二次探低)
    #[test]
    fn milestone_spacing_allows_refire_after_gap() {
        let mut points = Vec::with_capacity(100);
        let base = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        // baseline 60 天 @ 50%
        for i in 0..60 {
            let mut r = raw("2026-01-01", 50.0);
            r.date = base + chrono::Duration::days(i);
            points.push(r);
        }
        // 第 60 天 探低 49.0(第 1 次 fire)
        let mut r = raw("2026-01-01", 49.0);
        r.date = base + chrono::Duration::days(60);
        points.push(r);
        // 第 61-66 天 持平 49.0(無新低,7 天 gap >= spacing=5)
        for i in 61..67 {
            let mut r = raw("2026-01-01", 49.0);
            r.date = base + chrono::Duration::days(i as i64);
            points.push(r);
        }
        // 第 67 天 再探低 48.0(spacing >= 5,應再 fire)
        let mut r = raw("2026-01-01", 48.0);
        r.date = base + chrono::Duration::days(67);
        points.push(r);

        let series = ForeignHoldingSeries { stock_id: "2330".to_string(), points };
        let out = ForeignHoldingCore::new()
            .compute(&series, ForeignHoldingParams::default())
            .unwrap();
        let lows: Vec<_> = out.events.iter()
            .filter(|e| e.kind == ForeignHoldingEventKind::HoldingMilestoneLow)
            .collect();
        assert_eq!(
            lows.len(),
            2,
            "spacing 過後應再 fire(實際 {} 次)",
            lows.len()
        );
    }

    #[test]
    fn name_version() {
        let core = ForeignHoldingCore::new();
        assert_eq!(core.name(), "foreign_holding_core");
        assert_eq!(core.version(), "0.1.0");
    }
}
