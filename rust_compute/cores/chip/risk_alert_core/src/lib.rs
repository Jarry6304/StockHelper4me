// risk_alert_core(P2)— 對齊 m3Spec/chip_cores.md §十二(v3.21 拍版)
//
// 章節歸位:原 environment_cores §十 proposal,2026-05-17 拍版搬到 chip §十二
// (per-stock signal 屬 chip,environment 限全市場層)。
//
// 上游:直讀 Bronze `disposition_securities_period_tw`(對齊 fear_greed_core
// 例外風格,事件性低頻無需 Silver derived)。
//
// 4 EventKind:Announced / Entered / Exited / Escalation
// metadata.severity:warning(注意)/ disposition(分盤撮合)/ cash_only(全額交割)
//   — Silver builder 端 regex parser 寫入,本 Core 透傳
//
// Reference:
// - escalation 60 天 ≥ 2 次:「證券交易所公布注意交易資訊處置作業要點」§4(2024 版)

use anyhow::Result;
use chip_loader::{RiskAlertRaw, RiskAlertSeries};
use chrono::NaiveDate;
use fact_schema::{Fact, IndicatorCore, Timeframe};
use serde::Serialize;
use serde_json::json;

inventory::submit! {
    core_registry::CoreRegistration::new(
        "risk_alert_core", "0.1.0", core_registry::CoreKind::Chip, "P2",
        "Risk Alert Core(處置股風險警示;Announced / Entered / Exited / Escalation)",
    )
}

#[derive(Debug, Clone, Serialize)]
pub struct RiskAlertParams {
    pub timeframe: Timeframe,
    pub escalation_window_days: i64,        // 預設 60(對齊監管 §4)
    pub escalation_min_count: usize,        // 預設 2
    pub include_warning_only: bool,         // 是否包含 warning 級別,預設 true
}

impl Default for RiskAlertParams {
    fn default() -> Self {
        Self {
            timeframe: Timeframe::Daily,
            escalation_window_days: 60,
            escalation_min_count: 2,
            include_warning_only: true,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct RiskAlertOutput {
    pub stock_id: String,
    pub timeframe: Timeframe,
    pub events: Vec<RiskAlertEvent>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RiskAlertEvent {
    pub date: NaiveDate,
    pub kind: RiskAlertEventKind,
    pub value: f64,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum RiskAlertEventKind {
    DispositionAnnounced,
    DispositionEntered,
    DispositionExited,
    DispositionEscalation,
}

pub struct RiskAlertCore;
impl RiskAlertCore { pub fn new() -> Self { RiskAlertCore } }
impl Default for RiskAlertCore { fn default() -> Self { RiskAlertCore::new() } }

/// 解析 Bronze `measure` 中文字串為三級嚴重度。
/// 對齊 chip_cores.md §12.6 拍版三級體系。
fn parse_severity(measure: &str) -> &'static str {
    if measure.contains("全額交割") { return "cash_only"; }
    if measure.contains("人工管制") { return "disposition"; }
    if measure.contains("注意交易資訊") { return "warning"; }
    "unknown"
}

impl IndicatorCore for RiskAlertCore {
    type Input = RiskAlertSeries;
    type Params = RiskAlertParams;
    type Output = RiskAlertOutput;

    fn name(&self) -> &'static str { "risk_alert_core" }
    fn version(&self) -> &'static str { "0.1.0" }
    fn warmup_periods(&self, _params: &Self::Params) -> usize { 0 }

    fn compute(&self, input: &Self::Input, params: Self::Params) -> Result<Self::Output> {
        let mut events: Vec<RiskAlertEvent> = Vec::new();

        // 為 escalation 查找用,記每筆 (date, severity, disposition_cnt)
        type Record = (NaiveDate, &'static str, i32);
        let history: Vec<Record> = input.points.iter()
            .filter_map(|p: &RiskAlertRaw| {
                let measure = p.measure.as_deref().unwrap_or("");
                let sev = parse_severity(measure);
                if !params.include_warning_only && sev == "warning" { return None; }
                let cnt = p.disposition_cnt.unwrap_or(1);
                Some((p.date, sev, cnt))
            })
            .collect();

        for (idx, p) in input.points.iter().enumerate() {
            let measure = p.measure.as_deref().unwrap_or("");
            let severity = parse_severity(measure);
            if !params.include_warning_only && severity == "warning" { continue; }

            let cnt = p.disposition_cnt.unwrap_or(1);
            let condition = p.condition.clone().unwrap_or_default();
            let period_start = p.period_start;
            let period_end = p.period_end;

            // 1. DispositionAnnounced(公告日當日)
            events.push(RiskAlertEvent {
                date: p.date,
                kind: RiskAlertEventKind::DispositionAnnounced,
                value: cnt as f64,
                metadata: json!({
                    "severity": severity,
                    "disposition_cnt": cnt,
                    "period_start": period_start.map(|d| d.format("%Y-%m-%d").to_string()),
                    "period_end": period_end.map(|d| d.format("%Y-%m-%d").to_string()),
                    "condition": condition,
                    "raw_measure": measure,
                }),
            });

            // 2. DispositionEntered(period_start 當日)— 若 period_start 在序列內可單獨 fire,
            // 但 risk_alert 事件序列只有公告 row,Entered/Exited 用 metadata 攜帶日期供下游識讀;
            // Aggregation Layer 可用 period_start/end 與 query date 比對判定「目前在處置期間」
            if let Some(ps) = period_start {
                if ps != p.date {
                    events.push(RiskAlertEvent {
                        date: ps,
                        kind: RiskAlertEventKind::DispositionEntered,
                        value: cnt as f64,
                        metadata: json!({
                            "severity": severity,
                            "disposition_cnt": cnt,
                            "announced_date": p.date.format("%Y-%m-%d").to_string(),
                            "period_end": period_end.map(|d| d.format("%Y-%m-%d").to_string()),
                        }),
                    });
                }
            }

            // 3. DispositionExited(period_end + 1 個曆日 — 近似 + 1 個交易日;
            // 精確下次 trading_calendar 介入留 V3)
            if let Some(pe) = period_end {
                events.push(RiskAlertEvent {
                    date: pe.succ_opt().unwrap_or(pe),
                    kind: RiskAlertEventKind::DispositionExited,
                    value: cnt as f64,
                    metadata: json!({
                        "severity": severity,
                        "disposition_cnt": cnt,
                        "announced_date": p.date.format("%Y-%m-%d").to_string(),
                        "period_end": pe.format("%Y-%m-%d").to_string(),
                    }),
                });
            }

            // 4. DispositionEscalation(escalation_window_days 內 ≥ escalation_min_count 次)
            let window_start = p.date - chrono::Duration::days(params.escalation_window_days);
            let mut chain: Vec<&Record> = history.iter()
                .filter(|(d, _, _)| *d >= window_start && *d <= p.date)
                .collect();
            chain.sort_by_key(|r| r.0);
            if chain.len() >= params.escalation_min_count
                && chain.iter().rev().take(1).any(|r| r.0 == p.date)
            {
                // 只在「當日成為第 N 次」時 fire(edge trigger)
                let prior_count = idx; // history 同序;簡化用 idx
                if (prior_count + 1) >= params.escalation_min_count {
                    let chain_json: Vec<_> = chain.iter().map(|(d, s, c)| json!({
                        "date": d.format("%Y-%m-%d").to_string(),
                        "disposition_cnt": c,
                        "severity": s,
                    })).collect();
                    events.push(RiskAlertEvent {
                        date: p.date,
                        kind: RiskAlertEventKind::DispositionEscalation,
                        value: chain.len() as f64,
                        metadata: json!({
                            "severity": severity,
                            "disposition_cnt": cnt,
                            "escalation_chain": chain_json,
                            "window_days": params.escalation_window_days,
                        }),
                    });
                }
            }
        }

        Ok(RiskAlertOutput {
            stock_id: input.stock_id.clone(),
            timeframe: params.timeframe,
            events,
        })
    }

    fn produce_facts(&self, output: &Self::Output) -> Vec<Fact> {
        output.events.iter().map(|e| Fact {
            stock_id: output.stock_id.clone(),
            fact_date: e.date,
            timeframe: output.timeframe,
            source_core: "risk_alert_core".to_string(),
            source_version: "0.1.0".to_string(),
            params_hash: None,
            statement: format!("Risk alert {:?} on {}: cnt={:.0}", e.kind, e.date, e.value),
            metadata: fact_schema::with_event_kind(e.metadata.clone(), &e.kind),
        }).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(date: &str, cnt: i32, ps: Option<&str>, pe: Option<&str>, measure: &str) -> RiskAlertRaw {
        RiskAlertRaw {
            date: NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap(),
            disposition_cnt: Some(cnt),
            period_start: ps.map(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()),
            period_end: pe.map(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()),
            condition: Some("test".to_string()),
            measure: Some(measure.to_string()),
        }
    }

    #[test]
    fn severity_parser() {
        assert_eq!(parse_severity("注意交易資訊"), "warning");
        assert_eq!(parse_severity("人工管制之撮合終端機"), "disposition");
        assert_eq!(parse_severity("改以全額交割"), "cash_only");
        assert_eq!(parse_severity("無關文字"), "unknown");
    }

    #[test]
    fn announced_entered_exited_basic() {
        let core = RiskAlertCore::new();
        let input = RiskAlertSeries {
            stock_id: "3363".to_string(),
            points: vec![raw("2025-01-13", 2, Some("2025-01-14"), Some("2025-02-07"),
                "人工管制之撮合終端機")],
        };
        let out = core.compute(&input, RiskAlertParams::default()).unwrap();
        // Announced + Entered + Exited(+ possibly Escalation if cnt >=2 → 因為 cnt=2 第一筆 idx=0
        // history.len=1,prior_count+1=1 < min_count=2 → 不 fire)
        // 預期 3 個 event
        assert!(out.events.len() >= 3);
        let kinds: Vec<_> = out.events.iter().map(|e| e.kind).collect();
        assert!(kinds.contains(&RiskAlertEventKind::DispositionAnnounced));
        assert!(kinds.contains(&RiskAlertEventKind::DispositionEntered));
        assert!(kinds.contains(&RiskAlertEventKind::DispositionExited));
    }

    #[test]
    fn escalation_within_60_days() {
        let core = RiskAlertCore::new();
        let input = RiskAlertSeries {
            stock_id: "3363".to_string(),
            points: vec![
                raw("2025-01-13", 1, Some("2025-01-14"), Some("2025-02-07"), "注意交易資訊"),
                raw("2025-02-20", 2, Some("2025-02-21"), Some("2025-03-15"), "人工管制"),
            ],
        };
        let out = core.compute(&input, RiskAlertParams::default()).unwrap();
        let escalations: Vec<_> = out.events.iter()
            .filter(|e| e.kind == RiskAlertEventKind::DispositionEscalation).collect();
        assert_eq!(escalations.len(), 1, "second event within 60d should fire escalation");
    }
}
