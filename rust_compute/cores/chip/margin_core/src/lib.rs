// margin_core(P2)— Chip Core
//
// 對齊 m3Spec/chip_cores.md §四 margin_core(個股級融資融券)。
// 命名注意:本 Core 個股級;市場整體融資維持率為獨立 Core market_margin_core(Environment)
//
// **Reference(2026-05-10 加)**:
//   MAINTENANCE_LOW_THRESHOLD=145:**證交所《有價證券借貸辦法》§39** 追繳線 130%
//                                  + 證券商實務預警 145%(金管會公告);監管文件依據
//   margin_change_pct=5.0 / short_change_pct=10.0:無學術,集保中心統計
//                                                   ~3-5% 為「顯著」業界共識
//   short_to_margin_ratio_high=30 / low=5:無學術,業界經驗值「25-30% 偏高」
//
// **本 PR 範圍**:
//   - MarginParams + 7 個 EventKind(完整 §4.5)
//   - compute():逐筆組 series + day-over-day change_pct + short_to_margin_ratio
//   - detect:MarginSurge / Crash / ShortSqueeze / ShortBuildUp(threshold-based)
//   - ShortRatioExtremeHigh / Low(threshold-based)
//   - MaintenanceLow(只在 margin_maintenance 有值時觸發)
//
// historical_high 標籤(2026-05-11 加):EnteredShortRatioExtremeHigh event metadata
// 帶 `historical_high: bool`,標記當下 short_to_margin_ratio 是否是 series 內歷史新高
// (對齊 spec §4.6 範例「reached 32% on 2026-04-20(historical high)」)。
//
// TODO(後續討論):
//   - MaintenanceLow 閾值寫死 145(實務常見預警線)— 可外部化但 spec 沒列

use anyhow::Result;
use chip_loader::MarginDailySeries;
use chrono::NaiveDate;
use fact_schema::{Fact, IndicatorCore, Timeframe};
use serde::Serialize;
use serde_json::json;

inventory::submit! {
    core_registry::CoreRegistration::new(
        "margin_core",
        "0.1.0",
        core_registry::CoreKind::Chip,
        "P2",
        "Margin Core(個股級融資融券事實萃取)",
    )
}

#[derive(Debug, Clone, Serialize)]
pub struct MarginParams {
    pub timeframe: Timeframe,
    pub margin_change_pct_threshold: f64,
    pub short_change_pct_threshold: f64,
    pub short_to_margin_ratio_high: f64,
    pub short_to_margin_ratio_low: f64,
}

impl Default for MarginParams {
    fn default() -> Self {
        Self {
            timeframe: Timeframe::Daily,
            margin_change_pct_threshold: 5.0,
            short_change_pct_threshold: 10.0,
            short_to_margin_ratio_high: 30.0,
            short_to_margin_ratio_low: 5.0,
        }
    }
}

/// MaintenanceLow 警戒閾值(spec §4.5 EventKind 列出 MaintenanceLow,但 §4.3 Params 未列;寫死 const)
/// 145% 為融資維持率實務預警線(對齊 market_margin_core 同 const)
const MAINTENANCE_LOW_THRESHOLD: f64 = 145.0;

/// v3.11 Round 7 calibration(2026-05-16):MarginSurge/Crash/ShortSqueeze/ShortBuildUp
/// 4 個 day-over-day pct event 加最小間距。production 1266 stocks 跑出 13-24/yr,
/// 個股密集事件期(法人連續進出)內每天 day-over-day pct 大於 threshold → 反覆觸發。
/// 加 MIN_MARGIN_EVENT_SPACING_BARS = 20(~1 個月),預期 → ~6-10/yr。
/// 對齊 adx_core MIN_ADX_PEAK_SPACING_BARS 同款設計 / Brown & Warner 1985 事件研究
/// 「事件期內連續 trigger 應 dedup 為一事件群組」。
const MIN_MARGIN_EVENT_SPACING_BARS: usize = 20;

#[derive(Debug, Clone, Serialize)]
pub struct MarginOutput {
    pub stock_id: String,
    pub timeframe: Timeframe,
    pub series: Vec<MarginPoint>,
    pub events: Vec<MarginEvent>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MarginPoint {
    pub date: NaiveDate,
    pub margin_balance: i64,
    pub short_balance: i64,
    pub margin_change_pct: f64,
    pub short_change_pct: f64,
    pub short_to_margin_ratio: f64,
    /// 融資維持率 %(NULL 表示 Silver 沒提供;對齊 spec §4.5)
    pub margin_maintenance: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MarginEvent {
    pub date: NaiveDate,
    pub kind: MarginEventKind,
    pub value: f64,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum MarginEventKind {
    // 既有 day-over-day pattern(無需改動)
    MarginSurge,
    MarginCrash,
    ShortSqueeze,
    ShortBuildUp,
    // Round 4 transition pattern(2026-05-10):3 個 stay-in-zone → 6 個 Entered/Exited
    EnteredShortRatioExtremeHigh,
    ExitedShortRatioExtremeHigh,
    EnteredShortRatioExtremeLow,
    ExitedShortRatioExtremeLow,
    EnteredMaintenanceLow,
    ExitedMaintenanceLow,
}

pub struct MarginCore;

impl MarginCore {
    pub fn new() -> Self { MarginCore }
}
impl Default for MarginCore { fn default() -> Self { MarginCore::new() } }

impl IndicatorCore for MarginCore {
    type Input = MarginDailySeries;
    type Params = MarginParams;
    type Output = MarginOutput;

    fn name(&self) -> &'static str { "margin_core" }
    fn version(&self) -> &'static str { "0.1.0" }

    fn compute(&self, input: &Self::Input, params: Self::Params) -> Result<Self::Output> {
        let mut series = Vec::with_capacity(input.points.len());
        let mut prev_margin: Option<i64> = None;
        let mut prev_short: Option<i64> = None;
        for p in &input.points {
            // Skip rows with missing margin/short balance — Bronze 未收齊或假日尚未結算。
            // 不 skip 會踩 unwrap_or(0) → 真實 X 變 0 = 「down 100%」false positive(2026-05-09 dev DB 揭露)。
            let (mb, sb) = match (p.margin_balance, p.short_balance) {
                (Some(m), Some(s)) => (m, s),
                _ => continue,
            };
            let m_pct = pct_change(prev_margin, mb);
            let s_pct = pct_change(prev_short, sb);
            let ratio = if mb > 0 { sb as f64 / mb as f64 * 100.0 } else { 0.0 };
            series.push(MarginPoint {
                date: p.date,
                margin_balance: mb,
                short_balance: sb,
                margin_change_pct: m_pct,
                short_change_pct: s_pct,
                short_to_margin_ratio: ratio,
                margin_maintenance: p.margin_maintenance,
            });
            prev_margin = Some(mb);
            prev_short = Some(sb);
        }
        let events = detect_events(&series, &params);
        Ok(MarginOutput {
            stock_id: input.stock_id.clone(),
            timeframe: params.timeframe,
            series,
            events,
        })
    }

    fn produce_facts(&self, output: &Self::Output) -> Vec<Fact> {
        output.events.iter().map(|e| event_to_fact(output, e)).collect()
    }

    fn warmup_periods(&self, _: &Self::Params) -> usize { 20 }
}

fn pct_change(prev: Option<i64>, cur: i64) -> f64 {
    match prev {
        Some(p) if p > 0 => (cur - p) as f64 / p as f64 * 100.0,
        _ => 0.0,
    }
}

fn detect_events(series: &[MarginPoint], params: &MarginParams) -> Vec<MarginEvent> {
    let mut events = Vec::new();
    // Round 4 transition tracking(2026-05-10):3 個 bool prev_in_zone
    let mut prev_short_ratio_extreme_high: bool = false;
    let mut prev_short_ratio_extreme_low: bool = false;
    let mut prev_maintenance_low: bool = false;
    // historical_high 追蹤(2026-05-11):series 內 short_to_margin_ratio 累計最高值,
    // 用於 EnteredShortRatioExtremeHigh event metadata 的 historical_high 標籤
    let mut max_short_ratio_so_far: f64 = 0.0;
    // v3.11 Round 7:per-EventKind 最後觸發 idx,enforce MIN_MARGIN_EVENT_SPACING_BARS
    let mut last_margin_surge_idx: Option<usize> = None;
    let mut last_margin_crash_idx: Option<usize> = None;
    let mut last_short_squeeze_idx: Option<usize> = None;
    let mut last_short_build_up_idx: Option<usize> = None;
    for (i, p) in series.iter().enumerate() {
        // Day-over-day(v3.11 Round 7:加 spacing)
        if p.margin_change_pct >= params.margin_change_pct_threshold {
            let ok = last_margin_surge_idx
                .map_or(true, |last| i - last >= MIN_MARGIN_EVENT_SPACING_BARS);
            if ok {
                events.push(MarginEvent {
                    date: p.date,
                    kind: MarginEventKind::MarginSurge,
                    value: p.margin_change_pct,
                    metadata: json!({ "change_pct": p.margin_change_pct, "balance": p.margin_balance }),
                });
                last_margin_surge_idx = Some(i);
            }
        } else if p.margin_change_pct <= -params.margin_change_pct_threshold {
            let ok = last_margin_crash_idx
                .map_or(true, |last| i - last >= MIN_MARGIN_EVENT_SPACING_BARS);
            if ok {
                events.push(MarginEvent {
                    date: p.date,
                    kind: MarginEventKind::MarginCrash,
                    value: p.margin_change_pct,
                    metadata: json!({ "change_pct": p.margin_change_pct, "balance": p.margin_balance }),
                });
                last_margin_crash_idx = Some(i);
            }
        }
        if p.short_change_pct <= -params.short_change_pct_threshold {
            let ok = last_short_squeeze_idx
                .map_or(true, |last| i - last >= MIN_MARGIN_EVENT_SPACING_BARS);
            if ok {
                events.push(MarginEvent {
                    date: p.date,
                    kind: MarginEventKind::ShortSqueeze,
                    value: p.short_change_pct,
                    metadata: json!({ "change_pct": p.short_change_pct, "balance": p.short_balance }),
                });
                last_short_squeeze_idx = Some(i);
            }
        } else if p.short_change_pct >= params.short_change_pct_threshold {
            let ok = last_short_build_up_idx
                .map_or(true, |last| i - last >= MIN_MARGIN_EVENT_SPACING_BARS);
            if ok {
                events.push(MarginEvent {
                    date: p.date,
                    kind: MarginEventKind::ShortBuildUp,
                    value: p.short_change_pct,
                    metadata: json!({ "change_pct": p.short_change_pct, "balance": p.short_balance }),
                });
                last_short_build_up_idx = Some(i);
            }
        }
        // ShortRatio zone(transition pattern,Round 4)
        let cur_short_ratio_extreme_high = p.short_to_margin_ratio >= params.short_to_margin_ratio_high;
        let cur_short_ratio_extreme_low = p.short_to_margin_ratio > 0.0
            && p.short_to_margin_ratio <= params.short_to_margin_ratio_low;
        // historical_high 判斷在 max 更新前比較,確保新高觸發時 flag=true
        let is_historical_high = p.short_to_margin_ratio > max_short_ratio_so_far;
        if p.short_to_margin_ratio > max_short_ratio_so_far {
            max_short_ratio_so_far = p.short_to_margin_ratio;
        }
        if !prev_short_ratio_extreme_high && cur_short_ratio_extreme_high {
            events.push(MarginEvent {
                date: p.date,
                kind: MarginEventKind::EnteredShortRatioExtremeHigh,
                value: p.short_to_margin_ratio,
                metadata: json!({
                    "ratio": p.short_to_margin_ratio,
                    "threshold": params.short_to_margin_ratio_high,
                    "historical_high": is_historical_high,
                }),
            });
        } else if prev_short_ratio_extreme_high && !cur_short_ratio_extreme_high {
            events.push(MarginEvent {
                date: p.date,
                kind: MarginEventKind::ExitedShortRatioExtremeHigh,
                value: p.short_to_margin_ratio,
                metadata: json!({ "ratio": p.short_to_margin_ratio, "threshold": params.short_to_margin_ratio_high }),
            });
        }
        if !prev_short_ratio_extreme_low && cur_short_ratio_extreme_low {
            events.push(MarginEvent {
                date: p.date,
                kind: MarginEventKind::EnteredShortRatioExtremeLow,
                value: p.short_to_margin_ratio,
                metadata: json!({ "ratio": p.short_to_margin_ratio, "threshold": params.short_to_margin_ratio_low }),
            });
        } else if prev_short_ratio_extreme_low && !cur_short_ratio_extreme_low {
            events.push(MarginEvent {
                date: p.date,
                kind: MarginEventKind::ExitedShortRatioExtremeLow,
                value: p.short_to_margin_ratio,
                metadata: json!({ "ratio": p.short_to_margin_ratio, "threshold": params.short_to_margin_ratio_low }),
            });
        }
        prev_short_ratio_extreme_high = cur_short_ratio_extreme_high;
        prev_short_ratio_extreme_low = cur_short_ratio_extreme_low;
        // MaintenanceLow zone(transition pattern,Round 4)— 只對有 margin_maintenance 值
        let cur_maintenance_low = matches!(p.margin_maintenance,
            Some(m) if m > 0.0 && m < MAINTENANCE_LOW_THRESHOLD);
        if !prev_maintenance_low && cur_maintenance_low {
            if let Some(m) = p.margin_maintenance {
                events.push(MarginEvent {
                    date: p.date,
                    kind: MarginEventKind::EnteredMaintenanceLow,
                    value: m,
                    metadata: json!({ "maintenance": m, "threshold": MAINTENANCE_LOW_THRESHOLD }),
                });
            }
        } else if prev_maintenance_low && !cur_maintenance_low {
            // exited:用當前 maintenance(可能 None,則用 0)
            let val = p.margin_maintenance.unwrap_or(0.0);
            events.push(MarginEvent {
                date: p.date,
                kind: MarginEventKind::ExitedMaintenanceLow,
                value: val,
                metadata: json!({ "maintenance": val, "threshold": MAINTENANCE_LOW_THRESHOLD }),
            });
        }
        prev_maintenance_low = cur_maintenance_low;
    }
    events
}

fn event_to_fact(output: &MarginOutput, e: &MarginEvent) -> Fact {
    let statement = match e.kind {
        MarginEventKind::MarginSurge => format!("Margin balance up {:.1}% on {}", e.value, e.date),
        MarginEventKind::MarginCrash => format!("Margin balance down {:.1}% on {}", e.value.abs(), e.date),
        MarginEventKind::ShortSqueeze => format!("Short balance down {:.1}% on {}(short squeeze)", e.value.abs(), e.date),
        MarginEventKind::ShortBuildUp => format!("Short balance up {:.1}% on {}(short build-up)", e.value, e.date),
        // Round 4 transition statements
        MarginEventKind::EnteredShortRatioExtremeHigh => {
            let suffix = if e.metadata["historical_high"].as_bool().unwrap_or(false) {
                "(historical high)"
            } else {
                ""
            };
            format!("Short-to-margin ratio entered ExtremeHigh zone on {}: ratio={:.1}%{}", e.date, e.value, suffix)
        }
        MarginEventKind::ExitedShortRatioExtremeHigh => format!("Short-to-margin ratio exited ExtremeHigh zone on {}: ratio={:.1}%", e.date, e.value),
        MarginEventKind::EnteredShortRatioExtremeLow => format!("Short-to-margin ratio entered ExtremeLow zone on {}: ratio={:.1}%", e.date, e.value),
        MarginEventKind::ExitedShortRatioExtremeLow => format!("Short-to-margin ratio exited ExtremeLow zone on {}: ratio={:.1}%", e.date, e.value),
        MarginEventKind::EnteredMaintenanceLow => format!("Margin maintenance entered Low zone on {}: maintenance={:.1}%", e.date, e.value),
        MarginEventKind::ExitedMaintenanceLow => format!("Margin maintenance exited Low zone on {}: maintenance={:.1}%", e.date, e.value),
    };
    Fact {
        stock_id: output.stock_id.clone(),
        fact_date: e.date,
        timeframe: output.timeframe,
        source_core: "margin_core".to_string(),
        source_version: "0.1.0".to_string(),
        params_hash: None,
        statement,
        metadata: fact_schema::with_event_kind(e.metadata.clone(), &e.kind),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chip_loader::MarginDailyRaw;

    fn raw(d: &str, mb: i64, sb: i64) -> MarginDailyRaw {
        MarginDailyRaw {
            date: NaiveDate::parse_from_str(d, "%Y-%m-%d").unwrap(),
            margin_purchase: Some(0),
            margin_sell: Some(0),
            margin_balance: Some(mb),
            short_sale: Some(0),
            short_cover: Some(0),
            short_balance: Some(sb),
            margin_maintenance: None,
        }
    }

    #[test]
    fn margin_surge_emitted() {
        let series = MarginDailySeries {
            stock_id: "2330".to_string(),
            points: vec![raw("2026-04-21", 10_000, 100), raw("2026-04-22", 11_200, 100)],
        };
        let core = MarginCore::new();
        let out = core.compute(&series, MarginParams::default()).unwrap();
        assert!(out.events.iter().any(|e| e.kind == MarginEventKind::MarginSurge));
    }

    #[test]
    fn short_squeeze_emitted() {
        let series = MarginDailySeries {
            stock_id: "2330".to_string(),
            points: vec![raw("2026-04-21", 10_000, 5_000), raw("2026-04-22", 10_000, 3_200)],
        };
        let core = MarginCore::new();
        let out = core.compute(&series, MarginParams::default()).unwrap();
        assert!(out.events.iter().any(|e| e.kind == MarginEventKind::ShortSqueeze));
    }

    #[test]
    fn warmup_is_20() {
        assert_eq!(MarginCore::new().warmup_periods(&MarginParams::default()), 20);
    }

    /// Round 4 transition test:short_to_margin_ratio 從 normal 進 ExtremeHigh 觸發 1 次,
    /// 連日 stay 不重複,離開觸發 ExitedShortRatioExtremeHigh 1 次。
    #[test]
    fn short_ratio_zone_transition() {
        let series = MarginDailySeries {
            stock_id: "2330".to_string(),
            points: vec![
                raw("2026-04-21", 10_000, 1_000), // ratio=10%(normal)
                raw("2026-04-22", 10_000, 4_000), // ratio=40%(entered ExtremeHigh)
                raw("2026-04-23", 10_000, 4_500), // ratio=45%(stay,不該重複)
                raw("2026-04-24", 10_000, 1_500), // ratio=15%(exited)
            ],
        };
        let out = MarginCore::new().compute(&series, MarginParams::default()).unwrap();
        let entered = out.events.iter().filter(|e| e.kind == MarginEventKind::EnteredShortRatioExtremeHigh).count();
        let exited = out.events.iter().filter(|e| e.kind == MarginEventKind::ExitedShortRatioExtremeHigh).count();
        assert_eq!(entered, 1, "EnteredShortRatioExtremeHigh 應只 1 次");
        assert_eq!(exited, 1, "ExitedShortRatioExtremeHigh 應只 1 次");
    }

    /// historical_high metadata flag(2026-05-11):EnteredShortRatioExtremeHigh 第 1 次觸發
    /// 必為 historical_high=true(series 起始 max=0);後續若再進 ExtremeHigh 但 ratio 未超
    /// 既有最高則 historical_high=false。
    #[test]
    fn historical_high_flag_in_entered_extreme_high() {
        let series = MarginDailySeries {
            stock_id: "2330".to_string(),
            points: vec![
                raw("2026-04-21", 10_000, 1_000),  // ratio=10%(normal)
                raw("2026-04-22", 10_000, 5_000),  // ratio=50%(entered ExtremeHigh — historical_high=true)
                raw("2026-04-23", 10_000, 1_500),  // ratio=15%(exited)
                raw("2026-04-24", 10_000, 4_000),  // ratio=40%(entered again,但 < 50 → historical_high=false)
            ],
        };
        let out = MarginCore::new().compute(&series, MarginParams::default()).unwrap();
        let entered: Vec<&MarginEvent> = out
            .events
            .iter()
            .filter(|e| e.kind == MarginEventKind::EnteredShortRatioExtremeHigh)
            .collect();
        assert_eq!(entered.len(), 2, "應觸發 2 次 EnteredShortRatioExtremeHigh");
        assert_eq!(
            entered[0].metadata["historical_high"].as_bool(),
            Some(true),
            "第 1 次進 zone 必為 historical_high=true(50% > max=0)"
        );
        assert_eq!(
            entered[1].metadata["historical_high"].as_bool(),
            Some(false),
            "第 2 次 40% < 既有最高 50% → historical_high=false"
        );
        // statement 對 historical_high=true 應含 "(historical high)" 後綴
        let stmt0 = event_to_fact(&out, entered[0]).statement;
        assert!(stmt0.ends_with("(historical high)"), "statement: {}", stmt0);
        let stmt1 = event_to_fact(&out, entered[1]).statement;
        assert!(!stmt1.ends_with("(historical high)"), "statement: {}", stmt1);
    }

    /// v3.11 Round 7 regression(2026-05-16):day-over-day MarginSurge spacing。
    /// 連續 3 天 +6% margin_balance 應只觸發 1 次 MarginSurge(spacing 20 bars)。
    #[test]
    fn margin_surge_spacing_blocks_consecutive_days() {
        // 4 個連續 +6% day(margin balance 1000 → 1060 → 1124 → 1192 → 1264)
        let series = MarginDailySeries {
            stock_id: "2330".to_string(),
            points: vec![
                raw("2026-04-21", 1000, 0),
                raw("2026-04-22", 1060, 0),
                raw("2026-04-23", 1124, 0),
                raw("2026-04-24", 1192, 0),
                raw("2026-04-25", 1264, 0),
            ],
        };
        let out = MarginCore::new()
            .compute(&series, MarginParams::default())
            .unwrap();
        let surges: Vec<_> = out
            .events
            .iter()
            .filter(|e| e.kind == MarginEventKind::MarginSurge)
            .collect();
        assert_eq!(
            surges.len(),
            1,
            "4 連續 surge day 應只觸發 1 次(spacing=20 bars 內 dedup),實際 = {:?}",
            surges.iter().map(|e| e.date).collect::<Vec<_>>()
        );
    }

    /// Regression(2026-05-09 dev DB):假日 / Bronze 未收齊 row(margin_balance / short_balance NULL)
    /// 之前 unwrap_or(0) 會把 28995 → 0 算成「down 100%」false positive。
    /// 修法:整個 NULL row skip,series 不寫,events 不誤觸發。
    #[test]
    fn null_row_skipped_no_false_drop() {
        let series = MarginDailySeries {
            stock_id: "2330".to_string(),
            points: vec![
                raw("2026-04-29", 28995, 97),
                MarginDailyRaw {
                    date: NaiveDate::parse_from_str("2026-04-30", "%Y-%m-%d").unwrap(),
                    margin_purchase: None,
                    margin_sell: None,
                    margin_balance: None, // ← 假日,Bronze 未收齊
                    short_sale: None,
                    short_cover: None,
                    short_balance: None,
                    margin_maintenance: None,
                },
                raw("2026-05-02", 29100, 102), // 真實復原
            ],
        };
        let core = MarginCore::new();
        let out = core.compute(&series, MarginParams::default()).unwrap();
        // NULL row skip → series 只有 2 個 point(2026-04-29, 2026-05-02)
        assert_eq!(out.series.len(), 2);
        // 2026-04-29 → 2026-05-02 真實變化僅 +0.36%,不該觸發任何 MarginCrash
        assert!(out
            .events
            .iter()
            .all(|e| e.kind != MarginEventKind::MarginCrash),
            "MarginCrash false positive (NULL→0 unwrap bug regression),events = {:?}",
            out.events.iter().map(|e| e.kind).collect::<Vec<_>>()
        );
    }
}
