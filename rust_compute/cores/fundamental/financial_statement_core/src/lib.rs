// financial_statement_core(P2)— Fundamental Core(季頻)
//
// 對齊 m2Spec/oldm2Spec/fundamental_cores.md §五 financial_statement_core(spec r2)。
// Params §5.3 / Output §5.5(18 欄)/ EventKind 8 個 / warmup §5.4。
//
// 4 維 PK(market, stock_id, date, type),type ∈ {income, balance, cashflow}。
// 載入器負責拉 raw row,本 core 把同 date 的三類 row 組裝成 FinancialPoint。
//
// **detail JSONB key**:對齊 Silver `financial_statement_derived.detail`(2026-05-10
// fix):
//   Bronze `financial_statement.origin_name` 是中文(IFRS 會計科目),Silver builder
//   `financial_statement.py:60-62` 直接 pack origin_name → value,故 detail JSONB
//   key 是中文(「營業收入合計」/「本期淨利(淨損)」等)。
//   FinMind 不同年代用不同括號(半形 vs 全形)+ 同概念多命名變體,故 12 個欄位
//   各列 fallback chain via `jget_first_i64/f64`。
// **shareholder/financial_statement spec 校準清單**見 docs/m3_cores_spec_pending.md §3.3 / §4.3

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
            // 12 個直接從 detail JSONB 取的欄位,各用 fallback chain 對齊真實 IFRS 中文
            // origin_name + FinMind 半形/全形括號變體
            let revenue = jget_first_i64(inc, REVENUE_KEYS);
            let gross_profit = jget_first_i64(inc, GROSS_PROFIT_KEYS);
            let gross_margin_pct = if revenue > 0 { gross_profit as f64 / revenue as f64 * 100.0 } else { 0.0 };
            let operating_profit = jget_first_i64(inc, OPERATING_PROFIT_KEYS);
            let operating_margin_pct = if revenue > 0 { operating_profit as f64 / revenue as f64 * 100.0 } else { 0.0 };
            let net_income = jget_first_i64(inc, NET_INCOME_KEYS);
            let net_margin_pct = if revenue > 0 { net_income as f64 / revenue as f64 * 100.0 } else { 0.0 };
            let eps = jget_first_f64(inc, EPS_KEYS);
            // balance 是 common-size %(2026-05-10 user 揭露)。
            // total_* 都是 % 對總資產(total_assets ≡ 100.0)。直接讀 _f64,不轉 i64。
            // i64 路徑保留以防 user m3Spec/ 拍版改 Silver builder 改成元值。
            let total_assets_pct = jget_first_f64(bal, TOTAL_ASSETS_KEYS);
            let total_liabilities_pct = jget_first_f64(bal, TOTAL_LIABILITIES_KEYS);
            let total_equity_pct = jget_first_f64(bal, TOTAL_EQUITY_KEYS);
            // 資產 / 負債 / 權益元值無法從 balance % 推回(只在 user 改 Silver 才有)
            let total_assets: i64 = 0;
            let total_liabilities: i64 = 0;
            let total_equity: i64 = 0;
            // debt_ratio 直接讀 % 自身(「負債總額」% 已是 debt/assets ratio)
            let debt_ratio_pct = total_liabilities_pct;
            let operating_cash_flow = jget_first_i64(cf, OPERATING_CASH_FLOW_KEYS);
            let investing_cash_flow = jget_first_i64(cf, INVESTING_CASH_FLOW_KEYS);
            let financing_cash_flow = jget_first_i64(cf, FINANCING_CASH_FLOW_KEYS);
            let free_cash_flow = operating_cash_flow + investing_cash_flow; // FCF = OCF + ICF(經典定義)
            // ROE / ROA 跨 type 算(income 元 / balance %)會炸成 1e11+ false positive,
            // 設 0 skip RoeHigh / 留 EventKind 等 user m3Spec/ 拍版 balance 元值版本
            let roe_pct = 0.0;
            let roa_pct = 0.0;
            // 防止 unused warning(總資產 / 負債 / 權益元值欄位本 PR 設 0,等 user 拍版)
            let _ = (total_assets_pct, total_equity_pct);
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

/// 找第一個非 None 的 detail key(支援 i64 / f64 任一型別,自動轉 i64)
fn jget_first_i64(detail: Option<&serde_json::Value>, keys: &[&str]) -> i64 {
    let Some(d) = detail else { return 0; };
    for k in keys {
        if let Some(v) = d.get(*k) {
            if let Some(n) = v.as_i64() { return n; }
            if let Some(f) = v.as_f64() { return f as i64; }
        }
    }
    0
}

/// 找第一個非 None 的 detail key(f64;i64 自動轉)
fn jget_first_f64(detail: Option<&serde_json::Value>, keys: &[&str]) -> f64 {
    let Some(d) = detail else { return 0.0; };
    for k in keys {
        if let Some(v) = d.get(*k) {
            if let Some(f) = v.as_f64() { return f; }
            if let Some(n) = v.as_i64() { return n as f64; }
        }
    }
    0.0
}

// ---------------------------------------------------------------------------
// 12 個 detail JSONB key fallback chain(對齊 Silver `financial_statement_derived.detail`
// 真實 origin_name 中文)
//
// 來源:Bronze `financial_statement.origin_name` 直接 pack(原 IFRS 中文會計科目);
// FinMind 元值 row 跟 _per(%) row 都用同 origin_name,Silver builder dict 後寫覆蓋
// 前寫 → balance 實際全變 %(common-size analysis,2026-05-10 user 揭露)。
//
// 中文括號 user query 揭露用「（）」全形(Unicode U+FF08 / U+FF09),不是半形「()」
// (U+0028 / U+0029)。FinMind 不同年代可能混用,fallback chain 兩個都列防衛。
//
// **balance 是 %**(common-size,not 元值)→ ROE / ROA 不能跨 type 算(income 元 /
// balance %),設 0 skip RoeHigh / RoaHigh 觸發。debt_ratio_pct 直接讀「負債總額」
// % value(不再除 total_assets)。等 user m3Spec/ 拍版 balance 元值 vs %。
// 詳見 docs/m3_cores_spec_pending.md §4.3。
//
// **回退測試**:既有 mock 用英文 PascalCase key,fallback chain 末位放英文末位
// 維持 backward compat。新增 mock 全形中文 key 的 unit test 直驗 production data path。
// ---------------------------------------------------------------------------

const REVENUE_KEYS: &[&str] = &[
    "營業收入", "營業收入合計", "銷貨收入", "收入合計",
    "Revenue",
];
const GROSS_PROFIT_KEYS: &[&str] = &[
    "營業毛利\u{FF08}毛損\u{FF09}",   // 全形 U+FF08/FF09(實際 user 真 key)
    "營業毛利(毛損)",                  // 半形(防衛)
    "營業毛利", "銷貨毛利",
    "GrossProfit",
];
const OPERATING_PROFIT_KEYS: &[&str] = &[
    "營業利益\u{FF08}損失\u{FF09}",   // 全形 U+FF08/FF09
    "營業利益(損失)",                  // 半形
    "營業利益",
    "OperatingProfit",
];
const NET_INCOME_KEYS: &[&str] = &[
    "本期淨利\u{FF08}淨損\u{FF09}",   // 全形 U+FF08/FF09
    "本期淨利(淨損)",                  // 半形
    "繼續營業單位本期淨利\u{FF08}淨損\u{FF09}",  // 全形
    "淨利\u{FF08}淨損\u{FF09}歸屬於母公司業主",  // 全形
    "本期淨利", "本期綜合損益總額",
    "NetIncome",
];
const EPS_KEYS: &[&str] = &[
    "基本每股盈餘", "每股盈餘", "基本每股盈餘(元)",
    "EPS",
];
// Balance keys 是 % common-size(不是元值;2026-05-10 user 揭露)
const TOTAL_ASSETS_KEYS: &[&str] = &[
    "資產總額", "資產總計",
    "TotalAssets",
];
const TOTAL_LIABILITIES_KEYS: &[&str] = &[
    "負債總額", "負債總計",
    "TotalLiabilities",
];
const TOTAL_EQUITY_KEYS: &[&str] = &[
    "權益總額", "權益總計", "股東權益總計", "歸屬於母公司業主之權益合計",
    "TotalEquity",
];
const OPERATING_CASH_FLOW_KEYS: &[&str] = &[
    "營業活動之淨現金流入\u{FF08}流出\u{FF09}",   // 全形
    "營業活動之淨現金流入(流出)",                  // 半形
    "營業活動之淨現金流入",                         // 簡稱(無括號)
    "營業活動之現金流量",
    "OperatingCashFlow",
];
const INVESTING_CASH_FLOW_KEYS: &[&str] = &[
    "投資活動之淨現金流入\u{FF08}流出\u{FF09}",   // 全形
    "投資活動之淨現金流入(流出)",                  // 半形
    "投資活動之現金流量",
    "InvestingCashFlow",
];
const FINANCING_CASH_FLOW_KEYS: &[&str] = &[
    "籌資活動之淨現金流入\u{FF08}流出\u{FF09}",   // 全形
    "籌資活動之淨現金流入(流出)",                  // 半形
    "籌資活動之現金流量",
    "FinancingCashFlow",
];

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
    fn empty_input_no_panic() {
        let series = FinancialStatementSeries { stock_id: "2330".to_string(), points: vec![] };
        let out = FinancialStatementCore::new().compute(&series, FinancialStatementParams::default()).unwrap();
        assert!(out.events.is_empty());
        assert!(out.series.is_empty());
    }

    /// Regression(2026-05-10):detail JSONB key 改用真實 IFRS 中文 origin_name +
    /// fallback chain。對齊 Silver `financial_statement_derived.detail` 真結構:
    /// - income / cashflow:**全形括號**(實際 user 揭露的 key)+ 元值
    /// - balance:全部是 common-size %(對總資產比;不是元值)
    #[test]
    fn parses_chinese_origin_name_keys() {
        let series = FinancialStatementSeries {
            stock_id: "2330".to_string(),
            points: vec![
                FinancialStatementRaw {
                    date: NaiveDate::parse_from_str("2026-03-31", "%Y-%m-%d").unwrap(),
                    r#type: "income".to_string(),
                    // 全形括號 \u{FF08}/\u{FF09} 對齊 user 揭露 2330 2025-12-31 真實 detail key
                    detail: json!({
                        "營業收入":                                       100_000_000_i64,
                        "營業毛利\u{FF08}毛損\u{FF09}":                   50_000_000_i64,
                        "營業利益\u{FF08}損失\u{FF09}":                   30_000_000_i64,
                        "本期淨利\u{FF08}淨損\u{FF09}":                   20_000_000_i64,
                        "基本每股盈餘":                                   5.5_f64,
                    }),
                },
                FinancialStatementRaw {
                    date: NaiveDate::parse_from_str("2026-03-31", "%Y-%m-%d").unwrap(),
                    r#type: "balance".to_string(),
                    // balance 全是 % 對總資產比(common-size analysis)
                    detail: json!({
                        "資產總額":   100.0_f64,
                        "負債總額":    31.16_f64,
                        "權益總額":    68.84_f64,
                    }),
                },
                FinancialStatementRaw {
                    date: NaiveDate::parse_from_str("2026-03-31", "%Y-%m-%d").unwrap(),
                    r#type: "cashflow".to_string(),
                    detail: json!({
                        "營業活動之淨現金流入\u{FF08}流出\u{FF09}":  30_000_000_i64,
                        "投資活動之淨現金流入\u{FF08}流出\u{FF09}": -10_000_000_i64,
                        "籌資活動之淨現金流入\u{FF08}流出\u{FF09}":  -5_000_000_i64,
                    }),
                },
            ],
        };
        let out = FinancialStatementCore::new().compute(&series, FinancialStatementParams::default()).unwrap();
        assert_eq!(out.series.len(), 1);
        let p = &out.series[0];
        // income 元值
        assert_eq!(p.revenue, 100_000_000);
        assert_eq!(p.gross_profit, 50_000_000);
        assert_eq!(p.net_income, 20_000_000);
        assert!((p.eps - 5.5).abs() < 1e-9);
        // cashflow 元值 + 全形括號 fallback chain
        assert_eq!(p.operating_cash_flow, 30_000_000);
        assert_eq!(p.investing_cash_flow, -10_000_000);
        assert_eq!(p.financing_cash_flow, -5_000_000);
        assert_eq!(p.free_cash_flow, 20_000_000); // 30M + (-10M)
        // margin pct(income 元值內計算)
        assert!((p.gross_margin_pct - 50.0).abs() < 1e-6);
        // balance 是 %:debt_ratio 直接 = 「負債總額」% value(31.16),不再除 total_assets
        assert!((p.debt_ratio_pct - 31.16).abs() < 1e-6);
        // ROE / ROA 跨 type 不算(balance 是 %),設 0 避免 false positive
        assert_eq!(p.roe_pct, 0.0);
        assert_eq!(p.roa_pct, 0.0);
        // 元值 i64 欄位本 PR 設 0(等 user m3Spec/ 拍版 balance 元值版)
        assert_eq!(p.total_assets, 0);
        assert_eq!(p.total_liabilities, 0);
        assert_eq!(p.total_equity, 0);
    }

    /// Regression:既有 mock 用 balance 元值跑(舊 spec 假設),確認 fallback chain
    /// 末位英文 key 仍 work — 但 ROE / ROA 邏輯改了,無條件 = 0(不依賴 total_equity)
    #[test]
    fn assembles_three_types_into_one_point_no_roe_false_positive() {
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
                    // 英文 key fallback,但 balance 仍視為 %(2026-05-10 fix)
                    detail: json!({"TotalAssets": 100_000_000, "TotalLiabilities": 40_000_000, "TotalEquity": 60_000_000}),
                },
                FinancialStatementRaw {
                    date: NaiveDate::parse_from_str("2026-03-31", "%Y-%m-%d").unwrap(),
                    r#type: "cashflow".to_string(),
                    detail: json!({"OperatingCashFlow": 30_000_000, "InvestingCashFlow": -10_000_000}),
                },
            ],
        };
        let out = FinancialStatementCore::new().compute(&series, FinancialStatementParams::default()).unwrap();
        let p = &out.series[0];
        assert_eq!(p.eps, 5.5);
        assert_eq!(p.free_cash_flow, 20_000_000);
        // ROE / ROA 永遠 0(skip cross-type 計算)
        assert_eq!(p.roe_pct, 0.0);
        assert_eq!(p.roa_pct, 0.0);
        // RoeHigh / RoaHigh 不該觸發
        assert!(out.events.iter().all(|e| e.kind != FinancialEventKind::RoeHigh));
    }
}
