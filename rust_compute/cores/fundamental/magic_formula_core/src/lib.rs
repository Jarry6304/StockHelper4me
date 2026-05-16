// magic_formula_core(P2)— Fundamental Core(對齊 m3Spec/magic_formula_core.md)
//
// 上游 Silver:`magic_formula_ranked_derived`(由 silver builder
// `magic_formula_ranked.py` 跨股 cross-rank 後寫入;對齊 v3.4 plan §Phase A)。
// Rust core 純讀 per-stock 序列,比 (i, i-1) 兩日 `is_top_30` 變化 → 產
// EnteredTop30 / ExitedTop30 transition facts。
//
// EventKind 設計(對齊 user 拍版 2026-05-15「只 Top30 transition」):
//   EnteredTop30 : 前一日 is_top_30=false → 今日 true(平均 ~2-4/yr/stock)
//   ExitedTop30  : 前一日 is_top_30=true  → 今日 false
//   transition pattern(對齊 v1.32 P2 ≤ 12/yr/stock + fear_greed_core 範本)
//
// **Reference**:
//   - Greenblatt, J. (2005). *The Little Book That Beats the Market*.
//     Hoboken, NJ: Wiley. Ch. 5-7(原版 rank 邏輯 + 持有期)
//   - Larkin, K. (2009). "Magic Formula investing — the long-term evidence."
//     SSRN id=1330551(OOS 1988-2007 仍有效)
//   - Persson & Selander (2009). Lund Univ. thesis(歐洲市場 valid)

use anyhow::Result;
use chrono::NaiveDate;
use fact_schema::{Fact, IndicatorCore, Timeframe};
use fundamental_loader::MagicFormulaSeries;
use serde::Serialize;
use serde_json::json;

inventory::submit! {
    core_registry::CoreRegistration::new(
        "magic_formula_core", "0.1.0", core_registry::CoreKind::Fundamental, "P2",
        "Magic Formula Core(Greenblatt 2005 Top30 cross-rank transitions)",
    )
}

// ---------------------------------------------------------------------------
// Params
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct MagicFormulaParams {
    pub timeframe: Timeframe,
    /// Top-N 閾值(Greenblatt 原版 20-30,user 拍版 30)
    pub top_n: i32,
}

impl Default for MagicFormulaParams {
    fn default() -> Self {
        Self { timeframe: Timeframe::Daily, top_n: 30 }
    }
}

// ---------------------------------------------------------------------------
// Output
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct MagicFormulaOutput {
    pub stock_id: String,
    pub timeframe: Timeframe,
    pub series: Vec<MagicFormulaSeriesPoint>,
    pub events: Vec<MagicFormulaEvent>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MagicFormulaSeriesPoint {
    pub date: NaiveDate,
    pub fact_date: NaiveDate,
    pub earnings_yield: Option<f64>,
    pub roic: Option<f64>,
    pub combined_rank: Option<i32>,
    pub universe_size: Option<i32>,
    pub is_top_30: bool,
    pub excluded_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MagicFormulaEvent {
    pub date: NaiveDate,
    pub kind: MagicFormulaEventKind,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum MagicFormulaEventKind {
    EnteredTop30,
    ExitedTop30,
}

// ---------------------------------------------------------------------------
// Core
// ---------------------------------------------------------------------------

pub struct MagicFormulaCore;
impl MagicFormulaCore { pub fn new() -> Self { MagicFormulaCore } }
impl Default for MagicFormulaCore { fn default() -> Self { MagicFormulaCore::new() } }

impl IndicatorCore for MagicFormulaCore {
    type Input = MagicFormulaSeries;
    type Params = MagicFormulaParams;
    type Output = MagicFormulaOutput;

    fn name(&self) -> &'static str { "magic_formula_core" }
    fn version(&self) -> &'static str { "0.1.0" }

    /// 0 warmup:Silver builder 已維護 rank state,Rust 只需比相鄰兩日。
    fn warmup_periods(&self, _params: &Self::Params) -> usize { 0 }

    fn compute(&self, input: &Self::Input, params: Self::Params) -> Result<Self::Output> {
        let stock_id = input.stock_id.clone();

        let mut series: Vec<MagicFormulaSeriesPoint> = Vec::with_capacity(input.points.len());
        for p in &input.points {
            series.push(MagicFormulaSeriesPoint {
                date: p.date,
                fact_date: p.date,
                earnings_yield: p.earnings_yield,
                roic: p.roic,
                combined_rank: p.combined_rank,
                universe_size: p.universe_size,
                is_top_30: p.is_top_30,
                excluded_reason: p.excluded_reason.clone(),
            });
        }

        // Transition detection — 對齊 user 拍版「只 Top30 transition」
        // 第 1 個 row 沒前一日比對,跳過(Silver 第一次寫入時不產 phantom event)
        let mut events: Vec<MagicFormulaEvent> = Vec::new();
        for i in 1..series.len() {
            let prev = &series[i - 1];
            let cur = &series[i];
            if !prev.is_top_30 && cur.is_top_30 {
                events.push(MagicFormulaEvent {
                    date: cur.fact_date,
                    kind: MagicFormulaEventKind::EnteredTop30,
                    metadata: json!({
                        "earnings_yield": cur.earnings_yield,
                        "roic": cur.roic,
                        "combined_rank": cur.combined_rank,
                        "universe_size": cur.universe_size,
                        "top_n": params.top_n,
                    }),
                });
            } else if prev.is_top_30 && !cur.is_top_30 {
                events.push(MagicFormulaEvent {
                    date: cur.fact_date,
                    kind: MagicFormulaEventKind::ExitedTop30,
                    metadata: json!({
                        "earnings_yield": cur.earnings_yield,
                        "roic": cur.roic,
                        "combined_rank": cur.combined_rank,
                        "excluded_reason": cur.excluded_reason.clone(),
                        "top_n": params.top_n,
                    }),
                });
            }
        }

        Ok(MagicFormulaOutput { stock_id, timeframe: params.timeframe, series, events })
    }

    fn produce_facts(&self, output: &Self::Output) -> Vec<Fact> {
        output.events.iter().map(|e| Fact {
            stock_id: output.stock_id.clone(),
            fact_date: e.date,
            timeframe: output.timeframe,
            source_core: "magic_formula_core".to_string(),
            source_version: "0.1.0".to_string(),
            params_hash: None,
            statement: format!("{:?} on {}", e.kind, e.date),
            metadata: fact_schema::with_event_kind(e.metadata.clone(), &e.kind),
        }).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fundamental_loader::MagicFormulaPoint;

    fn nd(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn mk_point(date: &str, is_top_30: bool, rank: Option<i32>) -> MagicFormulaPoint {
        MagicFormulaPoint {
            date: nd(date),
            earnings_yield: Some(0.10),
            roic: Some(0.25),
            ey_rank: rank,
            roic_rank: rank,
            combined_rank: rank.map(|r| r * 2),
            universe_size: Some(1400),
            is_top_30,
            excluded_reason: None,
        }
    }

    #[test]
    fn name_warmup_version() {
        let core = MagicFormulaCore::new();
        assert_eq!(core.name(), "magic_formula_core");
        assert_eq!(core.version(), "0.1.0");
        assert_eq!(core.warmup_periods(&MagicFormulaParams::default()), 0);
    }

    #[test]
    fn entered_top30_transition() {
        let series = MagicFormulaSeries {
            stock_id: "2330".to_string(),
            points: vec![
                mk_point("2026-05-13", false, Some(50)),
                mk_point("2026-05-14", false, Some(40)),
                mk_point("2026-05-15", true,  Some(15)),   // entered
            ],
        };
        let out = MagicFormulaCore::new().compute(&series, MagicFormulaParams::default()).unwrap();
        assert_eq!(out.events.len(), 1);
        assert_eq!(out.events[0].kind, MagicFormulaEventKind::EnteredTop30);
        assert_eq!(out.events[0].date, nd("2026-05-15"));
    }

    #[test]
    fn exited_top30_transition() {
        let series = MagicFormulaSeries {
            stock_id: "2330".to_string(),
            points: vec![
                mk_point("2026-05-14", true,  Some(15)),
                mk_point("2026-05-15", false, Some(35)),    // exited
            ],
        };
        let out = MagicFormulaCore::new().compute(&series, MagicFormulaParams::default()).unwrap();
        assert_eq!(out.events.len(), 1);
        assert_eq!(out.events[0].kind, MagicFormulaEventKind::ExitedTop30);
    }

    #[test]
    fn stay_in_top30_no_event() {
        let series = MagicFormulaSeries {
            stock_id: "2330".to_string(),
            points: vec![
                mk_point("2026-05-13", true, Some(10)),
                mk_point("2026-05-14", true, Some(8)),
                mk_point("2026-05-15", true, Some(12)),
            ],
        };
        let out = MagicFormulaCore::new().compute(&series, MagicFormulaParams::default()).unwrap();
        assert_eq!(out.events.len(), 0);
    }

    #[test]
    fn empty_series_no_panic() {
        let series = MagicFormulaSeries { stock_id: "2330".to_string(), points: vec![] };
        let out = MagicFormulaCore::new().compute(&series, MagicFormulaParams::default()).unwrap();
        assert!(out.series.is_empty());
        assert!(out.events.is_empty());
    }

    #[test]
    fn single_point_no_phantom_event() {
        let series = MagicFormulaSeries {
            stock_id: "2330".to_string(),
            points: vec![mk_point("2026-05-15", true, Some(10))],
        };
        let out = MagicFormulaCore::new().compute(&series, MagicFormulaParams::default()).unwrap();
        assert_eq!(out.events.len(), 0, "第 1 個 point 沒前一日比對,不產 phantom event");
    }

    #[test]
    fn produce_facts_metadata_complete() {
        let series = MagicFormulaSeries {
            stock_id: "2330".to_string(),
            points: vec![
                mk_point("2026-05-14", false, Some(40)),
                mk_point("2026-05-15", true,  Some(10)),
            ],
        };
        let out = MagicFormulaCore::new().compute(&series, MagicFormulaParams::default()).unwrap();
        let facts = MagicFormulaCore::new().produce_facts(&out);
        assert_eq!(facts.len(), 1);
        let fact = &facts[0];
        assert_eq!(fact.source_core, "magic_formula_core");
        assert_eq!(fact.stock_id, "2330");
        // metadata 應有 earnings_yield + combined_rank
        assert!(fact.metadata.get("earnings_yield").is_some());
        assert!(fact.metadata.get("combined_rank").is_some());
    }
}
