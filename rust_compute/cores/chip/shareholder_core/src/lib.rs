// shareholder_core(P2)— Chip Core(週頻)
//
// 對齊 m3Spec/chip_cores.md §六 shareholder_core(集保中心週頻發布)。
//
// **本 PR 範圍**:
//   - 完整 ShareholderParams + Output + 6 EventKind(對齊 §6.5)
//   - compute():iterate Silver `holding_shares_per_derived.detail` 17 levels →
//     合成 small/mid/large 三類分布
//   - SmallHoldersDecreasing / Increasing / LargeHoldersAccumulating / Reducing(streak)
//   - ConcentrationRising / Decreasing(threshold-based)
//
// **detail JSONB key**:對齊 Silver 真實結構(2026-05-10 fix):
//   Silver builder `holding_shares_per.py:53-54` pack 結構為
//     `{level_str: {people, percent, unit}, ...}`
//   17 levels:1-999 / 1,000-5,000 / 5,001-10,000 / ... / more than 1,000,001 /
//             total / 差異數調整(說明4)
//   非預期的 flat key (small_holders_count 等),需 iterate level dict 加總合成。
//
// **best-guess 邊界(等 user m3Spec/ 拍版校準)**:
//   small  ≤ 5,000 股   ← 散戶
//   mid    ≤ 50,000 股  ← 中實戶
//   large  > 50,000 股  ← 大戶
//   concentration_index = large_holders_pct(預設大戶集中度公式)
//
// 詳見 docs/m3_cores_spec_pending.md §3.3 待校準項目。

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

/// 連續 streak 最小週數(spec §6.5 EventKind 列出 streak 事件,§6.3 Params 未列;寫死 const)
const STREAK_MIN_WEEKS: usize = 4;

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
    pub small_holders_count: usize,
    pub small_holders_pct: f64,
    pub mid_holders_count: usize,
    pub mid_holders_pct: f64,
    pub large_holders_count: usize,
    pub large_holders_pct: f64,
    pub total_holders: usize,
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
    LargeHoldersAccumulating,
    LargeHoldersReducing,
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
// 17 持股級距 → small/mid/large 合成
// ---------------------------------------------------------------------------

/// FinMind 集保中心 17 個 level string(對齊 Bronze `holding_shares_per.holding_shares_level`)
/// 順序:從小到大 + total + 異常 row;邊界以「股數上限」分類(5,000 / 50,000)
const SMALL_LEVELS: &[&str] = &["1-999", "1,000-5,000"];
const MID_LEVELS: &[&str] = &[
    "5,001-10,000", "10,001-15,000", "15,001-20,000",
    "20,001-30,000", "30,001-40,000", "40,001-50,000",
];
const LARGE_LEVELS: &[&str] = &[
    "50,001-100,000", "100,001-200,000", "200,001-400,000",
    "400,001-600,000", "600,001-800,000", "800,001-1,000,000",
    "more than 1,000,001",
];
const TOTAL_LEVEL: &str = "total";
// "差異數調整(說明4)" 為 FinMind 異常 row,iterate 時 skip(不在三類任一)

/// 從 17 level dict 加總 people / percent 合成 ShareholderPoint
fn synthesize_point(date: NaiveDate, detail: &serde_json::Value) -> ShareholderPoint {
    let (small_count, small_pct) = sum_levels(detail, SMALL_LEVELS);
    let (mid_count, mid_pct) = sum_levels(detail, MID_LEVELS);
    let (large_count, large_pct) = sum_levels(detail, LARGE_LEVELS);
    let total_holders = detail.get(TOTAL_LEVEL)
        .and_then(|v| v.get("people"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    // best-guess concentration_index = large_holders_pct(大戶集中度);等 user spec 拍板
    let concentration_index = large_pct;
    ShareholderPoint {
        date,
        small_holders_count: small_count,
        small_holders_pct: small_pct,
        mid_holders_count: mid_count,
        mid_holders_pct: mid_pct,
        large_holders_count: large_count,
        large_holders_pct: large_pct,
        total_holders,
        concentration_index,
    }
}

/// 在 levels list 上加總每個 level dict 的 people / percent
fn sum_levels(detail: &serde_json::Value, levels: &[&str]) -> (usize, f64) {
    let mut count: usize = 0;
    let mut pct: f64 = 0.0;
    for level in levels {
        if let Some(level_data) = detail.get(*level) {
            if let Some(p) = level_data.get("people").and_then(|v| v.as_u64()) {
                count += p as usize;
            }
            if let Some(pc) = level_data.get("percent").and_then(|v| v.as_f64()) {
                pct += pc;
            }
        }
    }
    (count, pct)
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
        ShareholderEventKind::ConcentrationRising => format!(
            "Concentration index up {:.2} on {}(week)", e.value, e.date),
        ShareholderEventKind::ConcentrationDecreasing => format!(
            "Concentration index down {:.2} on {}(week)", e.value.abs(), e.date),
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
    /// `small_count` 全部塞進 `1-999` level(對齊 SMALL_LEVELS 第一個);其他 levels
    /// 用固定 mid + large 數值,測試專注於 small_holders 遞減 streak。
    fn raw(d: &str, small_count: u64, large_pct: f64) -> HoldingSharesPerRaw {
        HoldingSharesPerRaw {
            date: NaiveDate::parse_from_str(d, "%Y-%m-%d").unwrap(),
            detail: json!({
                "1-999":              {"people": small_count, "percent": 30.0, "unit": 0},
                "1,000-5,000":        {"people": 0,           "percent": 0.0,  "unit": 0},
                "5,001-10,000":       {"people": 5_000,       "percent": 30.0, "unit": 0},
                "10,001-15,000":      {"people": 0,           "percent": 0.0,  "unit": 0},
                "15,001-20,000":      {"people": 0,           "percent": 0.0,  "unit": 0},
                "20,001-30,000":      {"people": 0,           "percent": 0.0,  "unit": 0},
                "30,001-40,000":      {"people": 0,           "percent": 0.0,  "unit": 0},
                "40,001-50,000":      {"people": 0,           "percent": 0.0,  "unit": 0},
                "50,001-100,000":     {"people": 200,         "percent": large_pct, "unit": 0},
                "100,001-200,000":    {"people": 0,           "percent": 0.0,  "unit": 0},
                "200,001-400,000":    {"people": 0,           "percent": 0.0,  "unit": 0},
                "400,001-600,000":    {"people": 0,           "percent": 0.0,  "unit": 0},
                "600,001-800,000":    {"people": 0,           "percent": 0.0,  "unit": 0},
                "800,001-1,000,000":  {"people": 0,           "percent": 0.0,  "unit": 0},
                "more than 1,000,001":{"people": 0,           "percent": 0.0,  "unit": 0},
                "total":              {"people": small_count + 5_000 + 200, "percent": 100.0, "unit": 0},
            }),
        }
    }

    #[test]
    fn small_holders_streak_detected() {
        // 5 連續週數小戶遞減
        let points = vec![
            raw("2026-04-04", 50_000, 40.0),
            raw("2026-04-11", 49_000, 40.0),
            raw("2026-04-18", 48_000, 40.0),
            raw("2026-04-25", 47_000, 40.0),
            raw("2026-05-02", 46_000, 40.0),
        ];
        let series = HoldingSharesPerSeries { stock_id: "2330".to_string(), points };
        let core = ShareholderCore::new();
        let out = core.compute(&series, ShareholderParams::default()).unwrap();
        assert!(out.events.iter().any(|e| e.kind == ShareholderEventKind::SmallHoldersDecreasing));
    }

    #[test]
    fn synthesize_aggregates_levels() {
        // 1 row,small = 50_000(全在 1-999) + 0(1,000-5,000) = 50_000
        let row = raw("2026-04-04", 50_000, 40.0);
        let series = HoldingSharesPerSeries { stock_id: "2330".to_string(), points: vec![row] };
        let out = ShareholderCore::new().compute(&series, ShareholderParams::default()).unwrap();
        let p = &out.series[0];
        assert_eq!(p.small_holders_count, 50_000);
        assert_eq!(p.large_holders_count, 200);
        assert!((p.large_holders_pct - 40.0).abs() < 1e-9);
        // concentration_index = large_holders_pct(預設公式)
        assert!((p.concentration_index - 40.0).abs() < 1e-9);
        // total_holders 取自 "total" level 的 people
        assert_eq!(p.total_holders, 55_200);
    }

    #[test]
    fn warmup_8_weeks() {
        assert_eq!(ShareholderCore::new().warmup_periods(&ShareholderParams::default()), 8);
    }
}
