// financial_statement_core(P2)— Fundamental Core(季頻)
//
// 對齊 m2Spec/oldm2Spec/fundamental_cores.md §五 financial_statement_core(spec r2)。
// Params §5.3 / Output §5.5(18 欄)/ EventKind 8 個 / warmup §5.4。
//
// 4 維 PK(market, stock_id, date, type),type ∈ {income, balance, cashflow}。
// 載入器負責拉 raw row,本 core 把同 date 的三類 row 組裝成 FinancialPoint。
//
// JSONB key 對齊 best-guess(對齊 PR #18.5 dual-write entries),P0 後對 Silver
// `financial_statement_derived.detail` 校準。

use anyhow::Result;
use chrono::NaiveDate;
use fact_schema::{Fact, IndicatorCore, Timeframe};
use fundamental_loader::FinancialStatementSeries;
use serde::Serialize;
use serde_json::json;
use std::collections::BTreeMap;

inventory::submit! {
    core_registry::CoreRegistration::new(
        "financial_statement_core", "0.1.0", core_registry::CoreKind::Fundamental, "P2",
        "Financial Statement Core(財報三表 — income/balance/cashflow)",
    )
}

// ---------------------------------------------------------------------------
// Params(§5.3)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct FinancialStatementParams {
    pub gross_margin_change_threshold: f64, // 預設 2.0(%)
    pub roe_high_threshold: f64,            // 預設 15.0(%)
    pub debt_ratio_high_threshold: f64,     // 預設 60.0(%)
    pub fcf_negative_streak_quarters: usize, // 預設 4
}

impl Default for FinancialStatementParams {
    fn default() -> Self {
        Self {
            gross_margin_change_threshold: 2.0,
            roe_high_threshold: 15.0,
            debt_ratio_high_threshold: 60.0,
            fcf_negative_streak_quarters: 4,
        }
    }
}

// ---------------------------------------------------------------------------
// Output(§5.5,18 欄)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct FinancialStatementOutput {
    pub stock_id: String,
    pub timeframe: Timeframe,
    pub series: Vec<FinancialPoint>,
    pub events: Vec<FinancialEvent>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FinancialPoint {
    pub period: String,         // "2026Q1"
    pub fact_date: NaiveDate,   // 季末日
    pub report_date: NaiveDate, // 實際發布日(detail.report_date 或 fallback fact_date)
    // 損益表
    pub revenue: i64,
    pub gross_profit: i64,
    pub gross_margin_pct: f64,
    pub operating_profit: i64,
    pub operating_margin_pct: f64,
    pub net_income: i64,
    pub net_margin_pct: f64,
    pub eps: f64,
    // 資產負債表
    pub total_assets: i64,
    pub total_liabilities: i64,
    pub total_equity: i64,
    pub debt_ratio_pct: f64,
    // 現金流量表
    pub operating_cash_flow: i64,
    pub investing_cash_flow: i64,
    pub financing_cash_flow: i64,
    pub free_cash_flow: i64,
    // 比率指標
    pub roe_pct: f64,
    pub roa_pct: f64,
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
    GrossMarginRising,
    GrossMarginFalling,
    RoeHigh,
    DebtRatioRising,
    OperatingCashFlowNegative,
    FreeCashFlowNegativeStreak,
    EpsTurnNegative,
    EpsTurnPositive,
}

// ---------------------------------------------------------------------------
// Core
// ---------------------------------------------------------------------------

pub struct FinancialStatementCore;
impl FinancialStatementCore { pub fn new() -> Self { FinancialStatementCore } }
impl Default for FinancialStatementCore { fn default() -> Self { FinancialStatementCore::new() } }

impl IndicatorCore for FinancialStatementCore {
    type Input = FinancialStatementSeries;
    type Params = FinancialStatementParams;
    type Output = FinancialStatementOutput;

    fn name(&self) -> &'static str { "financial_statement_core" }
    fn version(&self) -> &'static str { "0.1.0" }

    /// §5.4:`fcf_negative_streak_quarters * 90 + 60`(daily batch 解讀單位)
    fn warmup_periods(&self, params: &Self::Params) -> usize {
        params.fcf_negative_streak_quarters * 90 + 60
    }

    fn compute(&self, input: &Self::Input, params: Self::Params) -> Result<Self::Output> {
        // Pass 1:把同 date 的 income / balance / cashflow 3 row 組裝成 1 個 FinancialPoint
        let mut by_date: BTreeMap<NaiveDate, [Option<&serde_json::Value>; 3]> = BTreeMap::new();
        for raw in &input.points {
            let idx = match raw.r#type.as_str() {
                "income" => 0, "balance" => 1, "cashflow" => 2,
                _ => continue,
            };
            by_date.entry(raw.date).or_insert([None, None, None])[idx] = Some(&raw.detail);
        }

        let series: Vec<FinancialPoint> = by_date.into_iter().map(|(date, slots)| {
            let inc = slots[0]; let bal = slots[1]; let cf = slots[2];
            let revenue = jget_i64(inc, "Revenue");
            let gross_profit = jget_i64(inc, "GrossProfit");
            let gross_margin_pct = if revenue > 0 { gross_profit as f64 / revenue as f64 * 100.0 } else { 0.0 };
            let operating_profit = jget_i64(inc, "OperatingProfit");
            let operating_margin_pct = if revenue > 0 { operating_profit as f64 / revenue as f64 * 100.0 } else { 0.0 };
            let net_income = jget_i64(inc, "NetIncome");
            let net_margin_pct = if revenue > 0 { net_income as f64 / revenue as f64 * 100.0 } else { 0.0 };
            let eps = jget_f64(inc, "EPS");
            let total_assets = jget_i64(bal, "TotalAssets");
            let total_liabilities = jget_i64(bal, "TotalLiabilities");
            let total_equity = jget_i64(bal, "TotalEquity");
            let debt_ratio_pct = if total_assets > 0 { total_liabilities as f64 / total_assets as f64 * 100.0 } else { 0.0 };
            let operating_cash_flow = jget_i64(cf, "OperatingCashFlow");
            let investing_cash_flow = jget_i64(cf, "InvestingCashFlow");
            let financing_cash_flow = jget_i64(cf, "FinancingCashFlow");
            let free_cash_flow = operating_cash_flow + investing_cash_flow; // FCF = OCF + ICF(經典定義)
            let roe_pct = if total_equity > 0 { net_income as f64 / total_equity as f64 * 100.0 } else { 0.0 };
            let roa_pct = if total_assets > 0 { net_income as f64 / total_assets as f64 * 100.0 } else { 0.0 };
            let report_date = inc.and_then(|v| v.get("report_date"))
                .and_then(|v| v.as_str())
                .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
                .unwrap_or(date);
            FinancialPoint {
                period: format_period(date),
                fact_date: date,
                report_date,
                revenue, gross_profit, gross_margin_pct, operating_profit, operating_margin_pct,
                net_income, net_margin_pct, eps,
                total_assets, total_liabilities, total_equity, debt_ratio_pct,
                operating_cash_flow, investing_cash_flow, financing_cash_flow, free_cash_flow,
                roe_pct, roa_pct,
            }
        }).collect();

        // events
        let mut events = Vec::new();
        let mut prev_eps_sign: Option<i32> = None;
        let mut prev_gross: Option<f64> = None;
        let mut prev_debt: Option<f64> = None;
        let mut fcf_neg_count: usize = 0;
        for p in &series {
            // GrossMarginRising / Falling
            if let Some(prev) = prev_gross {
                let diff = p.gross_margin_pct - prev;
                if diff >= params.gross_margin_change_threshold {
                    events.push(FinancialEvent { date: p.fact_date, kind: FinancialEventKind::GrossMarginRising, value: diff,
                        metadata: json!({"period": p.period, "current": p.gross_margin_pct, "previous": prev, "change": diff}) });
                } else if diff <= -params.gross_margin_change_threshold {
                    events.push(FinancialEvent { date: p.fact_date, kind: FinancialEventKind::GrossMarginFalling, value: diff,
                        metadata: json!({"period": p.period, "current": p.gross_margin_pct, "previous": prev, "change": diff}) });
                }
            }
            prev_gross = Some(p.gross_margin_pct);
            // RoeHigh
            if p.roe_pct >= params.roe_high_threshold {
                events.push(FinancialEvent { date: p.fact_date, kind: FinancialEventKind::RoeHigh, value: p.roe_pct,
                    metadata: json!({"period": p.period, "roe": p.roe_pct, "threshold": params.roe_high_threshold}) });
            }
            // DebtRatioRising(超 threshold + 上升)
            if p.debt_ratio_pct >= params.debt_ratio_high_threshold {
                if let Some(prev) = prev_debt {
                    if p.debt_ratio_pct > prev {
                        events.push(FinancialEvent { date: p.fact_date, kind: FinancialEventKind::DebtRatioRising, value: p.debt_ratio_pct,
                            metadata: json!({"period": p.period, "current": p.debt_ratio_pct, "previous": prev}) });
                    }
                }
            }
            prev_debt = Some(p.debt_ratio_pct);
            // OperatingCashFlowNegative
            if p.operating_cash_flow < 0 {
                events.push(FinancialEvent { date: p.fact_date, kind: FinancialEventKind::OperatingCashFlowNegative, value: p.operating_cash_flow as f64,
                    metadata: json!({"period": p.period, "ocf": p.operating_cash_flow}) });
            }
            // FCF negative streak
            if p.free_cash_flow < 0 { fcf_neg_count += 1; } else { fcf_neg_count = 0; }
            if fcf_neg_count == params.fcf_negative_streak_quarters {
                events.push(FinancialEvent { date: p.fact_date, kind: FinancialEventKind::FreeCashFlowNegativeStreak, value: fcf_neg_count as f64,
                    metadata: json!({"period": p.period, "quarters": fcf_neg_count}) });
            }
            // EPS turn pos/neg
            let cur_sign = if p.eps > 0.0 { 1 } else if p.eps < 0.0 { -1 } else { 0 };
            if let Some(prev) = prev_eps_sign {
                if prev > 0 && cur_sign < 0 {
                    events.push(FinancialEvent { date: p.fact_date, kind: FinancialEventKind::EpsTurnNegative, value: p.eps,
                        metadata: json!({"period": p.period, "eps": p.eps}) });
                } else if prev < 0 && cur_sign > 0 {
                    events.push(FinancialEvent { date: p.fact_date, kind: FinancialEventKind::EpsTurnPositive, value: p.eps,
                        metadata: json!({"period": p.period, "eps": p.eps}) });
                }
            }
            prev_eps_sign = Some(cur_sign);
        }

        Ok(FinancialStatementOutput {
            stock_id: input.stock_id.clone(),
            timeframe: Timeframe::Monthly, // 季頻沒對應 Timeframe enum,取 Monthly approximation
            series,
            events,
        })
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
}

fn jget_i64(detail: Option<&serde_json::Value>, key: &str) -> i64 {
    detail.and_then(|d| d.get(key)).and_then(|v| v.as_i64().or_else(|| v.as_f64().map(|f| f as i64))).unwrap_or(0)
}
fn jget_f64(detail: Option<&serde_json::Value>, key: &str) -> f64 {
    detail.and_then(|d| d.get(key)).and_then(|v| v.as_f64()).unwrap_or(0.0)
}

fn format_period(date: NaiveDate) -> String {
    use chrono::Datelike;
    let q = match date.month() { 1..=3 => 1, 4..=6 => 2, 7..=9 => 3, _ => 4 };
    format!("{}Q{}", date.year(), q)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fundamental_loader::FinancialStatementRaw;

    #[test]
    fn name_warmup() {
        let core = FinancialStatementCore::new();
        assert_eq!(core.name(), "financial_statement_core");
        assert_eq!(core.warmup_periods(&FinancialStatementParams::default()), 4 * 90 + 60);
    }

    #[test]
    fn period_format() {
        assert_eq!(format_period(NaiveDate::parse_from_str("2026-03-31", "%Y-%m-%d").unwrap()), "2026Q1");
        assert_eq!(format_period(NaiveDate::parse_from_str("2026-09-30", "%Y-%m-%d").unwrap()), "2026Q3");
    }

    #[test]
    fn assembles_three_types_into_one_point() {
        let series = FinancialStatementSeries {
            stock_id: "2330".to_string(),
            points: vec![
                FinancialStatementRaw {
                    date: NaiveDate::parse_from_str("2026-03-31", "%Y-%m-%d").unwrap(),
                    r#type: "income".to_string(),
                    detail: json!({"Revenue": 100_000_000, "GrossProfit": 50_000_000, "EPS": 5.5, "NetIncome": 20_000_000}),
                },
                FinancialStatementRaw {
                    date: NaiveDate::parse_from_str("2026-03-31", "%Y-%m-%d").unwrap(),
                    r#type: "balance".to_string(),
                    detail: json!({"TotalAssets": 500_000_000, "TotalLiabilities": 200_000_000, "TotalEquity": 300_000_000}),
                },
                FinancialStatementRaw {
                    date: NaiveDate::parse_from_str("2026-03-31", "%Y-%m-%d").unwrap(),
                    r#type: "cashflow".to_string(),
                    detail: json!({"OperatingCashFlow": 30_000_000, "InvestingCashFlow": -10_000_000}),
                },
            ],
        };
        let out = FinancialStatementCore::new().compute(&series, FinancialStatementParams::default()).unwrap();
        assert_eq!(out.series.len(), 1);
        let p = &out.series[0];
        assert_eq!(p.period, "2026Q1");
        assert!((p.gross_margin_pct - 50.0).abs() < 1e-6);
        assert!((p.debt_ratio_pct - 40.0).abs() < 1e-6);
        assert_eq!(p.free_cash_flow, 30_000_000 - 10_000_000); // 經典 FCF
        // ROE = 20M / 300M = 6.67%
        assert!((p.roe_pct - 6.6667).abs() < 0.01);
    }

    #[test]
    fn empty_input_no_panic() {
        let series = FinancialStatementSeries { stock_id: "2330".to_string(), points: vec![] };
        let out = FinancialStatementCore::new().compute(&series, FinancialStatementParams::default()).unwrap();
        assert!(out.events.is_empty());
        assert!(out.series.is_empty());
    }
}
