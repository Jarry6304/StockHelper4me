// shareholder_core(P2)— Chip Core(週頻)
//
// 對齊 m3Spec/chip_cores.md §六 shareholder_core(集保中心週頻發布)。
//
// **本 PR 範圍**:
//   - 完整 ShareholderParams + Output + 6 EventKind(對齊 §6.5)
//   - compute():parse holding_shares_per_derived.detail JSONB → series
//   - 簡化:小戶 / 中實 / 大戶分類靠 holder count 或 share buckets
//   - SmallHoldersDecreasing / Increasing / LargeHoldersAccumulating / Reducing(streak)
//   - ConcentrationRising / Decreasing(threshold-based)
//
// TODO(後續討論):
//   - Silver `holding_shares_per_derived.detail` JSONB schema 沒明文約定
//     (對齊 layered_schema_post_refactor.md 但細節 user 沒寫死)。
//     parse 邏輯用 best-guess key:`small_holders_count` / `small_holders_pct` /
//     `mid_holders_count` / `mid_holders_pct` / `large_holders_count` /
//     `large_holders_pct` / `total_holders` / `concentration_index`
//     若 Silver builder 用其他 key,parse 會 fallback 0.0 / 0,user 本機跑會發現
//   - concentration_index 算法(Gini vs 自定義)留 P0 後校準
//   - 「持股級距(張數 buckets)」spec 未列具體 thresholds(預設 5 / 1000)— 對齊 §6.3

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
        let series: Vec<ShareholderPoint> = input.points.iter().map(|p| {
            let d = &p.detail;
            ShareholderPoint {
                date: p.date,
                small_holders_count: d["small_holders_count"].as_u64().unwrap_or(0) as usize,
                small_holders_pct: d["small_holders_pct"].as_f64().unwrap_or(0.0),
                mid_holders_count: d["mid_holders_count"].as_u64().unwrap_or(0) as usize,
                mid_holders_pct: d["mid_holders_pct"].as_f64().unwrap_or(0.0),
                large_holders_count: d["large_holders_count"].as_u64().unwrap_or(0) as usize,
                large_holders_pct: d["large_holders_pct"].as_f64().unwrap_or(0.0),
                total_holders: d["total_holders"].as_u64().unwrap_or(0) as usize,
                concentration_index: d["concentration_index"].as_f64().unwrap_or(0.0),
            }
        }).collect();

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

    fn raw(d: &str, small: u64, large_pct: f64, conc: f64) -> HoldingSharesPerRaw {
        HoldingSharesPerRaw {
            date: NaiveDate::parse_from_str(d, "%Y-%m-%d").unwrap(),
            detail: json!({
                "small_holders_count": small,
                "small_holders_pct": 30.0,
                "mid_holders_count": 5_000,
                "mid_holders_pct": 30.0,
                "large_holders_count": 200,
                "large_holders_pct": large_pct,
                "total_holders": small + 5_000 + 200,
                "concentration_index": conc,
            }),
        }
    }

    #[test]
    fn small_holders_streak_detected() {
        // 5 連續週數小戶遞減
        let points = vec![
            raw("2026-04-04", 50_000, 40.0, 30.0),
            raw("2026-04-11", 49_000, 40.0, 30.0),
            raw("2026-04-18", 48_000, 40.0, 30.0),
            raw("2026-04-25", 47_000, 40.0, 30.0),
            raw("2026-05-02", 46_000, 40.0, 30.0),
        ];
        let series = HoldingSharesPerSeries { stock_id: "2330".to_string(), points };
        let core = ShareholderCore::new();
        let out = core.compute(&series, ShareholderParams::default()).unwrap();
        assert!(out.events.iter().any(|e| e.kind == ShareholderEventKind::SmallHoldersDecreasing));
    }

    #[test]
    fn warmup_8_weeks() {
        assert_eq!(ShareholderCore::new().warmup_periods(&ShareholderParams::default()), 8);
    }
}
