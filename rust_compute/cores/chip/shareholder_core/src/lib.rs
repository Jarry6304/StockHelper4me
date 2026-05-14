// shareholder_core(P2)— Chip Core(週頻)
//
// 對齊 m3Spec/chip_cores.md §六 shareholder_core(集保中心週頻發布)。
//
// **2026-05-10 Round 1 user 拍板邊界 + 公式**:
//   small(散戶):≤ 50 張(8 levels)
//   mid(中實戶):50 ~ 400 張(3 levels)
//   large(大戶):400 ~ 1000 張(3 levels)
//   super_large(千張):> 1000 張(1 level)
//   來源:Money 錢雜誌 50/400 散戶/中實/大戶 + 凱基/集保中心 1000 張大戶
//   concentration_index = (large.unit + super_large.unit) / total.unit
//   來源:業務「籌碼集中度」標準定義,採 unit (股數) 非 percent (人數)
//   STREAK_MIN_WEEKS = 8 週（實務值，缺直接學術根據，見 STREAK_MIN_WEEKS const 說明）
//
// detail JSONB key:對齊 Silver `holding_shares_per_derived.detail` 真結構:
//   17 levels:1-999 / 1,000-5,000 / 5,001-10,000 / ... / more than 1,000,001 /
//             total / 差異數調整(說明4)
//   skip 差異數調整(說明4) 異常 row。

use anyhow::Result;
use chip_loader::HoldingSharesPerSeries;
use chrono::NaiveDate;
use fact_schema::{Fact, IndicatorCore, Timeframe};
use serde::Serialize;
use serde_json::json;

inventory::submit! {
    core_registry::CoreRegistration::new(
        "shareholder_core",
        "0.1.0",
        core_registry::CoreKind::Chip,
        "P2",
        "Shareholder Core(持股級距分布 / 籌碼集中度,週頻)",
    )
}

#[derive(Debug, Clone, Serialize)]
pub struct ShareholderParams {
    pub timeframe: Timeframe,
    pub small_holder_threshold: usize,
    pub large_holder_threshold: usize,
    pub concentration_change_threshold: f64,
    pub small_holder_count_change_threshold: usize,
}

impl Default for ShareholderParams {
    fn default() -> Self {
        Self {
            timeframe: Timeframe::Weekly,
            small_holder_threshold: 5,
            large_holder_threshold: 1000,
            concentration_change_threshold: 1.0,
            small_holder_count_change_threshold: 500,
        }
    }
}

/// 連續 streak 最小週數
/// Reference(2026-05-12 校準): Moskowitz, Ooi & Pedersen (2012) JFE 104(2):228-250
/// 原文 lookback = 12 個月（≈52週）、holding = 1 個月，為「價格報酬動能」研究。
/// ⚠️ 跨領域援引：TSMOM 的 12 個月 lookback 是針對收益率序列，非持股集中度，
/// 自相關結構不同。此處 8 週（≈2 個月）屬實務經驗值，缺乏直接學術根據，
/// 需 production data 回測驗證（p2_calibration_data.sql C-4 觸發率）。
const STREAK_MIN_WEEKS: usize = 8;

#[derive(Debug, Clone, Serialize)]
pub struct ShareholderOutput {
    pub stock_id: String,
    pub timeframe: Timeframe,
    pub series: Vec<ShareholderPoint>,
    pub events: Vec<ShareholderEvent>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ShareholderPoint {
    pub date: NaiveDate,
    // 散戶 ≤ 50 張(2026-05-10 user 拍板 4-level)
    pub small_holders_count: usize,
    pub small_holders_pct: f64,
    // 中實戶 50 ~ 400 張
    pub mid_holders_count: usize,
    pub mid_holders_pct: f64,
    // 大戶 400 ~ 1000 張
    pub large_holders_count: usize,
    pub large_holders_pct: f64,
    // 千張大戶 > 1000 張
    pub super_large_holders_count: usize,
    pub super_large_holders_pct: f64,
    pub total_holders: usize,
    /// concentration_index = (large.unit + super_large.unit) / total.unit
    /// 籌碼集中度,採 unit (股數) 非 percent (人數)
    pub concentration_index: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ShareholderEvent {
    pub date: NaiveDate,
    pub kind: ShareholderEventKind,
    pub value: f64,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum ShareholderEventKind {
    SmallHoldersDecreasing,
    SmallHoldersIncreasing,
    // 4-level Round 1(2026-05-10):分大戶 / 千張大戶
    LargeHoldersAccumulating,
    LargeHoldersReducing,
    SuperLargeHoldersAccumulating,
    SuperLargeHoldersReducing,
    ConcentrationRising,
    ConcentrationDecreasing,
}

pub struct ShareholderCore;

impl ShareholderCore { pub fn new() -> Self { ShareholderCore } }
impl Default for ShareholderCore { fn default() -> Self { ShareholderCore::new() } }

impl IndicatorCore for ShareholderCore {
    type Input = HoldingSharesPerSeries;
    type Params = ShareholderParams;
    type Output = ShareholderOutput;

    fn name(&self) -> &'static str { "shareholder_core" }
    fn version(&self) -> &'static str { "0.1.0" }

    fn compute(&self, input: &Self::Input, params: Self::Params) -> Result<Self::Output> {
        let series: Vec<ShareholderPoint> = input.points.iter().map(|p| synthesize_point(p.date, &p.detail)).collect();
        let events = detect_events(&series, &params);
        Ok(ShareholderOutput {
            stock_id: input.stock_id.clone(),
            timeframe: params.timeframe,
            series,
            events,
        })
    }

    fn produce_facts(&self, output: &Self::Output) -> Vec<Fact> {
        output.events.iter().map(|e| event_to_fact(output, e)).collect()
    }

    fn warmup_periods(&self, _: &Self::Params) -> usize { 8 }
}

// ---------------------------------------------------------------------------
// 17 持股級距 → 4-level 合成(2026-05-10 user 拍板)
// ---------------------------------------------------------------------------

// FinMind 集保中心 17 個 level string(對齊 Bronze `holding_shares_per.holding_shares_level`)
// 4-level 邊界:50 張 / 400 張 / 1000 張(對應股數 50,000 / 400,000 / 1,000,000)

/// 散戶:≤ 50 張(8 levels)
const SMALL_LEVELS: &[&str] = &[
    "1-999", "1,000-5,000", "5,001-10,000", "10,001-15,000",
    "15,001-20,000", "20,001-30,000", "30,001-40,000", "40,001-50,000",
];

/// 中實戶:50 ~ 400 張(3 levels)
const MID_LEVELS: &[&str] = &[
    "50,001-100,000", "100,001-200,000", "200,001-400,000",
];

/// 大戶:400 ~ 1000 張(3 levels)
const LARGE_LEVELS: &[&str] = &[
    "400,001-600,000", "600,001-800,000", "800,001-1,000,000",
];

/// 千張大戶:> 1000 張(1 level)
const SUPER_LARGE_LEVELS: &[&str] = &[
    "more than 1,000,001",
];

const TOTAL_LEVEL: &str = "total";
// "差異數調整(說明4)" 為 FinMind 異常 row,iterate 時 skip(不在四類任一)

/// 從 17 level dict 加總 people / percent / unit 合成 ShareholderPoint
fn synthesize_point(date: NaiveDate, detail: &serde_json::Value) -> ShareholderPoint {
    let (small_count, small_pct, _) = sum_levels(detail, SMALL_LEVELS);
    let (mid_count, mid_pct, _) = sum_levels(detail, MID_LEVELS);
    let (large_count, large_pct, large_unit) = sum_levels(detail, LARGE_LEVELS);
    let (super_large_count, super_large_pct, super_large_unit) = sum_levels(detail, SUPER_LARGE_LEVELS);
    let total_holders = detail.get(TOTAL_LEVEL)
        .and_then(|v| v.get("people"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    let total_unit = detail.get(TOTAL_LEVEL)
        .and_then(|v| v.get("unit"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as f64;
    // user 拍板:concentration_index = (large.unit + super_large.unit) / total.unit
    // 採 unit (股數) 非 percent (人數)— 對齊「籌碼集中度」業務定義
    let concentration_index = if total_unit > 0.0 {
        (large_unit + super_large_unit) / total_unit
    } else { 0.0 };
    ShareholderPoint {
        date,
        small_holders_count: small_count,
        small_holders_pct: small_pct,
        mid_holders_count: mid_count,
        mid_holders_pct: mid_pct,
        large_holders_count: large_count,
        large_holders_pct: large_pct,
        super_large_holders_count: super_large_count,
        super_large_holders_pct: super_large_pct,
        total_holders,
        concentration_index,
    }
}

/// 在 levels list 上加總每個 level dict 的 people / percent / unit
fn sum_levels(detail: &serde_json::Value, levels: &[&str]) -> (usize, f64, f64) {
    let mut count: usize = 0;
    let mut pct: f64 = 0.0;
    let mut unit: f64 = 0.0;
    for level in levels {
        if let Some(level_data) = detail.get(*level) {
            if let Some(p) = level_data.get("people").and_then(|v| v.as_u64()) {
                count += p as usize;
            }
            if let Some(pc) = level_data.get("percent").and_then(|v| v.as_f64()) {
                pct += pc;
            }
            if let Some(u) = level_data.get("unit").and_then(|v| v.as_u64()) {
                unit += u as f64;
            }
        }
    }
    (count, pct, unit)
}

fn detect_events(series: &[ShareholderPoint], params: &ShareholderParams) -> Vec<ShareholderEvent> {
    let mut events = Vec::new();
    if series.len() < 2 { return events; }

    // streak detection — 連續 N 週小戶減少 / 大戶累積
    streak(series, STREAK_MIN_WEEKS,
        |a, b| (b.small_holders_count as i64) < (a.small_holders_count as i64),
        ShareholderEventKind::SmallHoldersDecreasing, &mut events);
    streak(series, STREAK_MIN_WEEKS,
        |a, b| (b.small_holders_count as i64) > (a.small_holders_count as i64),
        ShareholderEventKind::SmallHoldersIncreasing, &mut events);
    streak(series, STREAK_MIN_WEEKS,
        |a, b| b.large_holders_pct > a.large_holders_pct,
        ShareholderEventKind::LargeHoldersAccumulating, &mut events);
    streak(series, STREAK_MIN_WEEKS,
        |a, b| b.large_holders_pct < a.large_holders_pct,
        ShareholderEventKind::LargeHoldersReducing, &mut events);
    // 千張大戶 streak(Round 1 user 拍板加 4th level)
    streak(series, STREAK_MIN_WEEKS,
        |a, b| b.super_large_holders_pct > a.super_large_holders_pct,
        ShareholderEventKind::SuperLargeHoldersAccumulating, &mut events);
    streak(series, STREAK_MIN_WEEKS,
        |a, b| b.super_large_holders_pct < a.super_large_holders_pct,
        ShareholderEventKind::SuperLargeHoldersReducing, &mut events);

    // ConcentrationRising / Decreasing — 與前一筆比較,變化超過 threshold
    for i in 1..series.len() {
        let diff = series[i].concentration_index - series[i - 1].concentration_index;
        if diff >= params.concentration_change_threshold {
            events.push(ShareholderEvent {
                date: series[i].date,
                kind: ShareholderEventKind::ConcentrationRising,
                value: diff,
                metadata: json!({ "change": diff, "value": series[i].concentration_index, "frequency": "weekly" }),
            });
        } else if diff <= -params.concentration_change_threshold {
            events.push(ShareholderEvent {
                date: series[i].date,
                kind: ShareholderEventKind::ConcentrationDecreasing,
                value: diff,
                metadata: json!({ "change": diff, "value": series[i].concentration_index, "frequency": "weekly" }),
            });
        }
    }

    events
}

fn streak(
    series: &[ShareholderPoint],
    min_weeks: usize,
    predicate: impl Fn(&ShareholderPoint, &ShareholderPoint) -> bool,
    kind: ShareholderEventKind,
    out: &mut Vec<ShareholderEvent>,
) {
    let mut start: Option<usize> = None;
    for i in 1..series.len() {
        if predicate(&series[i - 1], &series[i]) {
            if start.is_none() { start = Some(i - 1); }
        } else if let Some(s) = start.take() {
            let weeks = i - s;
            if weeks >= min_weeks {
                emit(series, s, i - 1, weeks, kind, out);
            }
        }
    }
    if let Some(s) = start {
        let weeks = series.len() - s;
        if weeks >= min_weeks {
            emit(series, s, series.len() - 1, weeks, kind, out);
        }
    }
}

fn emit(series: &[ShareholderPoint], start: usize, end: usize, weeks: usize, kind: ShareholderEventKind, out: &mut Vec<ShareholderEvent>) {
    out.push(ShareholderEvent {
        date: series[end].date,
        kind,
        value: weeks as f64,
        metadata: json!({
            "weeks": weeks,
            "start_date": series[start].date,
            "end_date": series[end].date,
            "frequency": "weekly",
        }),
    });
}

fn event_to_fact(output: &ShareholderOutput, e: &ShareholderEvent) -> Fact {
    let statement = match e.kind {
        ShareholderEventKind::SmallHoldersDecreasing => format!(
            "Small holders count decreased over {} consecutive weeks ending on {}", e.value as i64, e.date),
        ShareholderEventKind::SmallHoldersIncreasing => format!(
            "Small holders count increased over {} consecutive weeks ending on {}", e.value as i64, e.date),
        ShareholderEventKind::LargeHoldersAccumulating => format!(
            "Large holders accumulating for {} consecutive weeks ending on {}", e.value as i64, e.date),
        ShareholderEventKind::LargeHoldersReducing => format!(
            "Large holders reducing for {} consecutive weeks ending on {}", e.value as i64, e.date),
        ShareholderEventKind::SuperLargeHoldersAccumulating => format!(
            "Super-large (>1000 lots) holders accumulating for {} consecutive weeks ending on {}", e.value as i64, e.date),
        ShareholderEventKind::SuperLargeHoldersReducing => format!(
            "Super-large (>1000 lots) holders reducing for {} consecutive weeks ending on {}", e.value as i64, e.date),
        ShareholderEventKind::ConcentrationRising => format!(
            "Concentration index up {:.4} on {}(week)", e.value, e.date),
        ShareholderEventKind::ConcentrationDecreasing => format!(
            "Concentration index down {:.4} on {}(week)", e.value.abs(), e.date),
    };
    Fact {
        stock_id: output.stock_id.clone(),
        fact_date: e.date,
        timeframe: output.timeframe,
        source_core: "shareholder_core".to_string(),
        source_version: "0.1.0".to_string(),
        params_hash: None,
        statement,
        metadata: e.metadata.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chip_loader::HoldingSharesPerRaw;

    /// Mock raw 用真實 level dict 結構(對齊 Silver `holding_shares_per_derived.detail`)。
    /// 4-level 邊界(2026-05-10 user 拍板):
    ///   small 全塞 1-999 / mid 全塞 50,001-100,000 / large 全塞 400,001-600,000 /
    ///   super_large 全塞 more than 1,000,001
    fn raw(d: &str, small_count: u64, super_large_unit: u64) -> HoldingSharesPerRaw {
        HoldingSharesPerRaw {
            date: NaiveDate::parse_from_str(d, "%Y-%m-%d").unwrap(),
            detail: json!({
                // small (8 levels) — 全部 unit 0,people 全塞 1-999
                "1-999":              {"people": small_count, "percent": 30.0, "unit": 0_u64},
                "1,000-5,000":        {"people": 0_u64, "percent": 0.0, "unit": 0_u64},
                "5,001-10,000":       {"people": 0_u64, "percent": 0.0, "unit": 0_u64},
                "10,001-15,000":      {"people": 0_u64, "percent": 0.0, "unit": 0_u64},
                "15,001-20,000":      {"people": 0_u64, "percent": 0.0, "unit": 0_u64},
                "20,001-30,000":      {"people": 0_u64, "percent": 0.0, "unit": 0_u64},
                "30,001-40,000":      {"people": 0_u64, "percent": 0.0, "unit": 0_u64},
                "40,001-50,000":      {"people": 0_u64, "percent": 0.0, "unit": 0_u64},
                // mid (3 levels)
                "50,001-100,000":     {"people": 5_000_u64, "percent": 30.0, "unit": 100_000_u64},
                "100,001-200,000":    {"people": 0_u64, "percent": 0.0, "unit": 0_u64},
                "200,001-400,000":    {"people": 0_u64, "percent": 0.0, "unit": 0_u64},
                // large (3 levels)
                "400,001-600,000":    {"people": 200_u64, "percent": 5.0, "unit": 500_000_u64},
                "600,001-800,000":    {"people": 0_u64, "percent": 0.0, "unit": 0_u64},
                "800,001-1,000,000":  {"people": 0_u64, "percent": 0.0, "unit": 0_u64},
                // super_large (1 level)
                "more than 1,000,001":{"people": 50_u64, "percent": 0.5, "unit": super_large_unit},
                "total":              {"people": small_count + 5_000 + 200 + 50, "percent": 100.0, "unit": 1_000_000_u64},
            }),
        }
    }

    #[test]
    fn small_holders_streak_detected() {
        // 8 連續週數小戶遞減(對齊 STREAK_MIN_WEEKS=8)
        let mut points = Vec::new();
        for (i, count) in [50_000, 49_000, 48_000, 47_000, 46_000, 45_000, 44_000, 43_000, 42_000].iter().enumerate() {
            let date = NaiveDate::from_ymd_opt(2026, 4, 4).unwrap() + chrono::Duration::weeks(i as i64);
            points.push(raw(&date.to_string(), *count, 0));
        }
        let series = HoldingSharesPerSeries { stock_id: "2330".to_string(), points };
        let core = ShareholderCore::new();
        let out = core.compute(&series, ShareholderParams::default()).unwrap();
        assert!(out.events.iter().any(|e| e.kind == ShareholderEventKind::SmallHoldersDecreasing),
                "SmallHoldersDecreasing 8-week streak should fire");
    }

    #[test]
    fn synthesize_aggregates_4_level_with_concentration_unit() {
        // 1 row 驗 4-level 合成 + concentration_index = (large.unit + super_large.unit) / total.unit
        let row = raw("2026-04-04", 50_000, 300_000);
        let series = HoldingSharesPerSeries { stock_id: "2330".to_string(), points: vec![row] };
        let out = ShareholderCore::new().compute(&series, ShareholderParams::default()).unwrap();
        let p = &out.series[0];
        assert_eq!(p.small_holders_count, 50_000);
        assert_eq!(p.mid_holders_count, 5_000);
        assert_eq!(p.large_holders_count, 200);
        assert_eq!(p.super_large_holders_count, 50);
        // total_holders 取自 "total" level 的 people = 50_000 + 5_000 + 200 + 50 = 55_250
        assert_eq!(p.total_holders, 55_250);
        // concentration_index = (large.unit 500_000 + super_large.unit 300_000) / total.unit 1_000_000 = 0.8
        assert!((p.concentration_index - 0.8).abs() < 1e-9,
                "concentration_index 應 = 0.8(800K large+super 股 / 1M total 股),實際 {}", p.concentration_index);
    }

    #[test]
    fn warmup_8_weeks() {
        assert_eq!(ShareholderCore::new().warmup_periods(&ShareholderParams::default()), 8);
    }
}
