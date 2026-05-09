// financial_statement_core(P2)— Fundamental Core(季頻)
//
// 對齊 oldm2Spec/fundamental_cores.md(spec user m3Spec 待寫)。
// 上游 Silver:financial_statement_derived,PK 含 type ∈ {income, balance, cashflow}。
//
// **本 PR 範圍**(極限推進,基本框架):
//   - parse detail JSONB 取營收 / 毛利率 / EPS 等關鍵欄位
//   - 偵測 EPS 季增 / 季減 / 毛利率異常
//
// TODO(後續討論 — 多項):
//   - detail JSONB 中文欄名規範:目前只認英文 key(EPS / GrossProfitMargin / Revenue)
//   - 季頻時間對齊:財報發布有 lag,fact_date 用財報期末 vs 發布日?
//   - V3 議題:`financial_statement_core` 是否拆 income / balance / cashflow 三 cores
//     (cores_overview §14 列入 V3 議題,V2 不規劃)
//   - EPS YoY / 毛利率改善 streak 等多種事件型態
//   - 暫只實作最簡單 EPS 變化偵測,完整事件 list 留 m3Spec 校準

use anyhow::Result;
use chrono::NaiveDate;
use fact_schema::{Fact, IndicatorCore, Timeframe};
use fundamental_loader::FinancialStatementSeries;
use serde::Serialize;
use serde_json::json;

inventory::submit! {
    core_registry::CoreRegistration::new(
        "financial_statement_core",
        "0.1.0",
        core_registry::CoreKind::Fundamental,
        "P2",
        "Financial Statement Core(財報三表 — income/balance/cashflow)",
    )
}

#[derive(Debug, Clone, Serialize)]
pub struct FinancialStatementParams {
    pub timeframe: Timeframe,
    pub eps_qoq_change_threshold: f64, // EPS QoQ 變化 % 閾值,預設 50%
}

impl Default for FinancialStatementParams {
    fn default() -> Self {
        Self { timeframe: Timeframe::Monthly, eps_qoq_change_threshold: 50.0 } // 季頻沒對應 Timeframe 用 Monthly approximation
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct FinancialStatementOutput {
    pub stock_id: String,
    pub timeframe: Timeframe,
    pub series: Vec<FinancialPoint>,
    pub events: Vec<FinancialEvent>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FinancialPoint {
    pub date: NaiveDate,
    pub r#type: String, // income / balance / cashflow
    pub eps: f64,
    pub revenue: f64,
    pub gross_profit_margin: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct FinancialEvent {
    pub date: NaiveDate,
    pub kind: FinancialEventKind,
    pub value: f64,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum FinancialEventKind {
    EpsExtremeQoqUp,
    EpsExtremeQoqDown,
}

pub struct FinancialStatementCore;
impl FinancialStatementCore { pub fn new() -> Self { FinancialStatementCore } }
impl Default for FinancialStatementCore { fn default() -> Self { FinancialStatementCore::new() } }

impl IndicatorCore for FinancialStatementCore {
    type Input = FinancialStatementSeries;
    type Params = FinancialStatementParams;
    type Output = FinancialStatementOutput;

    fn name(&self) -> &'static str { "financial_statement_core" }
    fn version(&self) -> &'static str { "0.1.0" }

    fn compute(&self, input: &Self::Input, params: Self::Params) -> Result<Self::Output> {
        let series: Vec<FinancialPoint> = input.points.iter().map(|p| {
            let d = &p.detail;
            FinancialPoint {
                date: p.date,
                r#type: p.r#type.clone(),
                // best-guess key names — 留 m3Spec 校準
                eps: d.get("EPS").and_then(|v| v.as_f64()).unwrap_or(0.0),
                revenue: d.get("Revenue").and_then(|v| v.as_f64()).unwrap_or(0.0),
                gross_profit_margin: d.get("GrossProfitMargin").and_then(|v| v.as_f64()).unwrap_or(0.0),
            }
        }).collect();

        // EPS QoQ change(只對 income type)
        let mut events = Vec::new();
        let income_points: Vec<&FinancialPoint> = series.iter().filter(|p| p.r#type == "income").collect();
        for win in income_points.windows(2) {
            let prev = win[0];
            let cur = win[1];
            if prev.eps != 0.0 {
                let change_pct = (cur.eps - prev.eps) / prev.eps.abs() * 100.0;
                if change_pct >= params.eps_qoq_change_threshold {
                    events.push(FinancialEvent {
                        date: cur.date,
                        kind: FinancialEventKind::EpsExtremeQoqUp,
                        value: change_pct,
                        metadata: json!({ "eps_prev": prev.eps, "eps_cur": cur.eps, "change_pct": change_pct }),
                    });
                } else if change_pct <= -params.eps_qoq_change_threshold {
                    events.push(FinancialEvent {
                        date: cur.date,
                        kind: FinancialEventKind::EpsExtremeQoqDown,
                        value: change_pct,
                        metadata: json!({ "eps_prev": prev.eps, "eps_cur": cur.eps, "change_pct": change_pct }),
                    });
                }
            }
        }

        Ok(FinancialStatementOutput { stock_id: input.stock_id.clone(), timeframe: params.timeframe, series, events })
    }

    fn produce_facts(&self, output: &Self::Output) -> Vec<Fact> {
        output.events.iter().map(|e| Fact {
            stock_id: output.stock_id.clone(),
            fact_date: e.date,
            timeframe: output.timeframe,
            source_core: "financial_statement_core".to_string(),
            source_version: "0.1.0".to_string(),
            params_hash: None,
            statement: format!("{:?} on {}: value={:.2}", e.kind, e.date, e.value),
            metadata: e.metadata.clone(),
        }).collect()
    }

    fn warmup_periods(&self, _: &Self::Params) -> usize { 4 } // 4 季 = 1 年
}

#[cfg(test)]
mod tests {
    use super::*;
    use fundamental_loader::FinancialStatementRaw;

    #[test]
    fn name_and_warmup() {
        let core = FinancialStatementCore::new();
        assert_eq!(core.name(), "financial_statement_core");
        assert_eq!(core.warmup_periods(&FinancialStatementParams::default()), 4);
    }

    #[test]
    fn empty_input_no_panic() {
        let series = FinancialStatementSeries {
            stock_id: "2330".to_string(),
            points: vec![],
        };
        let core = FinancialStatementCore::new();
        let out = core.compute(&series, FinancialStatementParams::default()).unwrap();
        assert!(out.events.is_empty());
    }

    #[test]
    fn parses_detail_json() {
        let series = FinancialStatementSeries {
            stock_id: "2330".to_string(),
            points: vec![FinancialStatementRaw {
                date: NaiveDate::parse_from_str("2026-03-31", "%Y-%m-%d").unwrap(),
                r#type: "income".to_string(),
                detail: json!({ "EPS": 5.5, "Revenue": 100_000_000.0, "GrossProfitMargin": 50.0 }),
            }],
        };
        let core = FinancialStatementCore::new();
        let out = core.compute(&series, FinancialStatementParams::default()).unwrap();
        assert_eq!(out.series.len(), 1);
        assert!((out.series[0].eps - 5.5).abs() < 1e-9);
    }
}
