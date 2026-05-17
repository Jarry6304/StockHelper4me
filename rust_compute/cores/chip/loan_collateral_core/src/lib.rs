// loan_collateral_core(P2)— 對齊 m3Spec/chip_cores.md §十(v3.21 拍版)
//
// 11 EventKind:5 大類(Margin / FirmLoan / UnrestrictedLoan / FinanceLoan /
// SettlementMargin)× Surge/Crash = 10 + LoanCategoryConcentration = 11
//
// Reference:
// - category_concentration_threshold = 70%:Basel Committee on Banking
//   Supervision (2006), "Studies on Credit Risk Concentration" Working Paper 15
//   — CR1 > 0.7 視為 high concentration risk

use anyhow::Result;
use chip_loader::LoanCollateralSeries;
use chrono::NaiveDate;
use fact_schema::{Fact, IndicatorCore, Timeframe};
use serde::Serialize;
use serde_json::json;

inventory::submit! {
    core_registry::CoreRegistration::new(
        "loan_collateral_core", "0.1.0", core_registry::CoreKind::Chip, "P2",
        "Loan Collateral Core(5 大類借券 × Surge/Crash + Concentration)",
    )
}

#[derive(Debug, Clone, Serialize)]
pub struct LoanCollateralParams {
    pub timeframe: Timeframe,
    pub balance_change_pct_threshold: f64,     // 預設 10.0 (%)
    pub category_concentration_threshold: f64, // 預設 0.7 (Basel CR1)
}

impl Default for LoanCollateralParams {
    fn default() -> Self {
        Self {
            timeframe: Timeframe::Daily,
            balance_change_pct_threshold: 10.0,
            category_concentration_threshold: 0.7,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct LoanCollateralOutput {
    pub stock_id: String,
    pub timeframe: Timeframe,
    pub events: Vec<LoanCollateralEvent>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LoanCollateralEvent {
    pub date: NaiveDate,
    pub kind: LoanCollateralEventKind,
    pub value: f64,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum LoanCollateralEventKind {
    MarginBalanceSurge, MarginBalanceCrash,
    FirmLoanSurge, FirmLoanCrash,
    UnrestrictedLoanSurge, UnrestrictedLoanCrash,
    FinanceLoanSurge, FinanceLoanCrash,
    SettlementMarginSurge, SettlementMarginCrash,
    LoanCategoryConcentration,
}

pub struct LoanCollateralCore;
impl LoanCollateralCore { pub fn new() -> Self { LoanCollateralCore } }
impl Default for LoanCollateralCore { fn default() -> Self { LoanCollateralCore::new() } }

fn check_category(
    events: &mut Vec<LoanCollateralEvent>,
    date: NaiveDate,
    category: &str,
    change_pct: Option<f64>,
    current_balance: Option<i64>,
    threshold: f64,
    surge_kind: LoanCollateralEventKind,
    crash_kind: LoanCollateralEventKind,
) {
    let pct = match change_pct { Some(v) => v, None => return };
    let bal = current_balance.unwrap_or(0);
    if pct >= threshold {
        events.push(LoanCollateralEvent {
            date, kind: surge_kind, value: pct,
            metadata: json!({"category": category, "current_balance": bal, "change_pct": pct}),
        });
    } else if pct <= -threshold {
        events.push(LoanCollateralEvent {
            date, kind: crash_kind, value: pct,
            metadata: json!({"category": category, "current_balance": bal, "change_pct": pct}),
        });
    }
}

impl IndicatorCore for LoanCollateralCore {
    type Input = LoanCollateralSeries;
    type Params = LoanCollateralParams;
    type Output = LoanCollateralOutput;

    fn name(&self) -> &'static str { "loan_collateral_core" }
    fn version(&self) -> &'static str { "0.1.0" }
    fn warmup_periods(&self, _params: &Self::Params) -> usize { 2 }

    fn compute(&self, input: &Self::Input, params: Self::Params) -> Result<Self::Output> {
        let mut events = Vec::new();
        let thr = params.balance_change_pct_threshold;

        for p in &input.points {
            // 5 大類 × Surge/Crash
            check_category(&mut events, p.date, "margin",
                p.margin_change_pct, p.margin_current_balance, thr,
                LoanCollateralEventKind::MarginBalanceSurge,
                LoanCollateralEventKind::MarginBalanceCrash);
            check_category(&mut events, p.date, "firm_loan",
                p.firm_loan_change_pct, p.firm_loan_current_balance, thr,
                LoanCollateralEventKind::FirmLoanSurge,
                LoanCollateralEventKind::FirmLoanCrash);
            check_category(&mut events, p.date, "unrestricted_loan",
                p.unrestricted_loan_change_pct, p.unrestricted_loan_current_balance, thr,
                LoanCollateralEventKind::UnrestrictedLoanSurge,
                LoanCollateralEventKind::UnrestrictedLoanCrash);
            check_category(&mut events, p.date, "finance_loan",
                p.finance_loan_change_pct, p.finance_loan_current_balance, thr,
                LoanCollateralEventKind::FinanceLoanSurge,
                LoanCollateralEventKind::FinanceLoanCrash);
            check_category(&mut events, p.date, "settlement_margin",
                p.settlement_margin_change_pct, p.settlement_margin_current_balance, thr,
                LoanCollateralEventKind::SettlementMarginSurge,
                LoanCollateralEventKind::SettlementMarginCrash);

            // LoanCategoryConcentration(dominant_category_ratio > threshold)
            if let Some(ratio) = p.dominant_category_ratio {
                if ratio >= params.category_concentration_threshold {
                    events.push(LoanCollateralEvent {
                        date: p.date,
                        kind: LoanCollateralEventKind::LoanCategoryConcentration,
                        value: ratio,
                        metadata: json!({
                            "dominant_category": p.dominant_category.clone(),
                            "category_ratio": ratio,
                            "total_balance": p.total_balance.unwrap_or(0),
                        }),
                    });
                }
            }
        }

        Ok(LoanCollateralOutput {
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
            source_core: "loan_collateral_core".to_string(),
            source_version: "0.1.0".to_string(),
            params_hash: None,
            statement: format!("LoanCollateral {:?} on {}: value={:.4}", e.kind, e.date, e.value),
            metadata: fact_schema::with_event_kind(e.metadata.clone(), &e.kind),
        }).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chip_loader::LoanCollateralRaw;

    fn pt(date: &str, margin_pct: Option<f64>, ratio: Option<f64>) -> LoanCollateralRaw {
        LoanCollateralRaw {
            date: NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap(),
            margin_current_balance: Some(26483),
            firm_loan_current_balance: Some(22),
            unrestricted_loan_current_balance: Some(77125),
            finance_loan_current_balance: Some(8691),
            settlement_margin_current_balance: Some(0),
            margin_change_pct: margin_pct,
            firm_loan_change_pct: Some(0.0),
            unrestricted_loan_change_pct: Some(0.0),
            finance_loan_change_pct: Some(0.0),
            settlement_margin_change_pct: Some(0.0),
            total_balance: Some(112321),
            dominant_category: Some("unrestricted_loan".to_string()),
            dominant_category_ratio: ratio,
        }
    }

    #[test]
    fn margin_surge_above_threshold() {
        let core = LoanCollateralCore::new();
        let input = LoanCollateralSeries {
            stock_id: "2330".to_string(),
            points: vec![pt("2025-01-02", Some(15.0), None)],
        };
        let out = core.compute(&input, LoanCollateralParams::default()).unwrap();
        let surges: Vec<_> = out.events.iter()
            .filter(|e| e.kind == LoanCollateralEventKind::MarginBalanceSurge).collect();
        assert_eq!(surges.len(), 1);
    }

    #[test]
    fn margin_crash_below_neg_threshold() {
        let core = LoanCollateralCore::new();
        let input = LoanCollateralSeries {
            stock_id: "2330".to_string(),
            points: vec![pt("2025-01-02", Some(-12.0), None)],
        };
        let out = core.compute(&input, LoanCollateralParams::default()).unwrap();
        let crashes: Vec<_> = out.events.iter()
            .filter(|e| e.kind == LoanCollateralEventKind::MarginBalanceCrash).collect();
        assert_eq!(crashes.len(), 1);
    }

    #[test]
    fn concentration_above_70_pct() {
        let core = LoanCollateralCore::new();
        let input = LoanCollateralSeries {
            stock_id: "2330".to_string(),
            points: vec![pt("2025-01-02", Some(0.0), Some(0.75))],
        };
        let out = core.compute(&input, LoanCollateralParams::default()).unwrap();
        let concentrations: Vec<_> = out.events.iter()
            .filter(|e| e.kind == LoanCollateralEventKind::LoanCategoryConcentration).collect();
        assert_eq!(concentrations.len(), 1);
    }

    #[test]
    fn no_fire_when_below_threshold() {
        let core = LoanCollateralCore::new();
        let input = LoanCollateralSeries {
            stock_id: "2330".to_string(),
            points: vec![pt("2025-01-02", Some(5.0), Some(0.55))],
        };
        let out = core.compute(&input, LoanCollateralParams::default()).unwrap();
        assert!(out.events.is_empty());
    }
}
