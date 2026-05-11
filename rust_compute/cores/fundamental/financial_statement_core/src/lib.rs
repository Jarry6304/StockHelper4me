// financial_statement_core(P2)— Fundamental Core(季頻)
//
// 對齊 m2Spec/oldm2Spec/fundamental_cores.md §五 financial_statement_core(spec r2)。
// Params §5.3 / Output §5.5(18 欄)/ EventKind 8 個 / warmup §5.4。
//
// **Reference(2026-05-10 加)**:
//   roe_high_threshold=15.0:**Buffett, Warren E. (1987) Berkshire shareholder letter
//                              + Cunningham L. (1997) "The Essays of Warren Buffett"**
//                              — 巴菲特標準「ROE 持續高於 15% = 高品質公司」
//                              (Fortune 1987 研究:25/1000 公司 avg ROE > 20% +
//                              最低不曾 < 15%)
//   ROE/ROA 算法(2026-05-11 改 TTM):net_income 用過去 4 季加總(TTM),除以期末
//                              equity / assets;對齊 Buffett「annual ROE」原 intent。
//                              FinMind income 給的是純季度元值,直接除永遠不會觸發
//                              15% 閾值(TSMC 2020-Q3 季 ROE=7.7%,年化 ~30%)。
//                              不足 4 季時用 quarterly × 4 作 annualized fallback。
//   debt_ratio_high_threshold=60.0:**業界 IFRS 公司分析共識**(Damodaran A. NYU Stern
//                                    "Investment Valuation" 提 70%+ 為「mathematical
//                                    problem」,60% 保守警示;非 explicit cite)
//   gross_margin_change_threshold=2.0:**IFRS 趨勢分析慣例**(相對變化 ~10% 視為顯著,
//                                       2pp / 20pp typical gross margin = 10%)
//   fcf_negative_streak_quarters=4:無學術 cite,業界「1 年連續為負 = distressed」共識
//
// 4 維 PK(market, stock_id, date, type),type ∈ {income, balance, cashflow}。
// 載入器負責拉 raw row,本 core 把同 date 的三類 row 組裝成 FinancialPoint。
//
// **detail JSONB key**:對齊 Silver `financial_statement_derived.detail`(2026-05-11
// Silver builder _per suffix fix):
//   Bronze `financial_statement.origin_name` 是中文(IFRS 會計科目)。
//   Silver builder 修法後:type=balance → detail["資產總額"]=元值,
//   type=balance_per → detail["資產總額_per"]=%(balance_per 在 by_date 走 `_ => continue`)。
//   故 balance slot 讀到元值;ROE/ROA 可正常計算。
//   FinMind 不同年代用不同括號(半形 vs 全形)+ 同概念多命名變體,fallback chain 各列兩者。
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

        let mut series: Vec<FinancialPoint> = by_date.into_iter().map(|(date, slots)| {
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
            // balance 元值(2026-05-11 Silver builder _per suffix fix 後):
            // Silver builder pack:type="balance" → detail["資產總額"]=元值,
            //                     type="balance_per" → detail["資產總額_per"]=%。
            // by_date 只映射 type="balance"(index 1),balance_per 走 `_ => continue`。
            // 故 jget_first_i64(bal, TOTAL_ASSETS_KEYS) 讀到的是元值(NTD)。
            let total_assets = jget_first_i64(bal, TOTAL_ASSETS_KEYS);
            let total_liabilities = jget_first_i64(bal, TOTAL_LIABILITIES_KEYS);
            let total_equity = jget_first_i64(bal, TOTAL_EQUITY_KEYS);
            let debt_ratio_pct = if total_assets > 0 {
                total_liabilities as f64 / total_assets as f64 * 100.0
            } else {
                0.0
            };
            let operating_cash_flow = jget_first_i64(cf, OPERATING_CASH_FLOW_KEYS);
            let investing_cash_flow = jget_first_i64(cf, INVESTING_CASH_FLOW_KEYS);
            let financing_cash_flow = jget_first_i64(cf, FINANCING_CASH_FLOW_KEYS);
            let free_cash_flow = operating_cash_flow + investing_cash_flow; // FCF = OCF + ICF(經典定義)
            let roe_pct = if total_equity > 0 {
                net_income as f64 / total_equity as f64 * 100.0
            } else {
                0.0
            };
            let roa_pct = if total_assets > 0 {
                net_income as f64 / total_assets as f64 * 100.0
            } else {
                0.0
            };
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

        // TTM (Trailing 12 Months) ROE/ROA — 2026-05-11 修法
        // 對齊 Buffett (1987) annual ROE 15% 標準。FinMind income 給的是季元值,
        // 直接除過 15% 閾值;改用過去 4 季 net_income 加總後再除 equity / assets。
        // 不足 4 季(序列前 3 個點)用 quarterly × 4 年化作 fallback,避免早期資料永遠無 RoeHigh。
        for i in 0..series.len() {
            let ttm_net_income: i64 = if i >= 3 {
                series[i-3..=i].iter().map(|p| p.net_income).sum()
            } else {
                series[i].net_income.saturating_mul(4)
            };
            let p = &mut series[i];
            p.roe_pct = if p.total_equity > 0 {
                ttm_net_income as f64 / p.total_equity as f64 * 100.0
            } else { 0.0 };
            p.roa_pct = if p.total_assets > 0 {
                ttm_net_income as f64 / p.total_assets as f64 * 100.0
            } else { 0.0 };
        }

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
            timeframe: Timeframe::Quarterly, // 2026-05-10 加 Timeframe::Quarterly variant 對齊 spec §5.5 季頻
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
// FinMind 元值 row 與 _per(%) row 都用同 origin_name;Silver builder _per suffix
// fix(2026-05-11)後:detail["資產總額"] = 元值(type=balance),
// detail["資產總額_per"] = %(type=balance_per)。
// by_date 只映射 type="balance"(index 1),balance_per 走 `_ => continue` → 不進入。
// 故 balance slot 讀到的是元值;ROE/ROA 可從 income元 / balance元 計算。
//
// 中文括號 user query 揭露用「（）」全形(Unicode U+FF08 / U+FF09),不是半形「()」
// (U+0028 / U+0029)。FinMind 不同年代可能混用,fallback chain 兩個都列防衛。
//
// **balance 元值**(2026-05-11 Silver builder fix 後)。詳見 docs/m3_cores_spec_pending.md §4.3。
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
// Balance keys(元值;Silver builder _per suffix fix 後 2026-05-11)
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
                    // balance 元值(Silver builder _per suffix fix 後 2026-05-11)
                    detail: json!({
                        "資產總額":   5_000_000_000_i64,
                        "負債總額":   1_558_000_000_i64,
                        "權益總額":   3_442_000_000_i64,
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
        // balance 元值(Silver builder _per suffix fix 後 2026-05-11)
        assert_eq!(p.total_assets, 5_000_000_000_i64);
        assert_eq!(p.total_liabilities, 1_558_000_000_i64);
        assert_eq!(p.total_equity, 3_442_000_000_i64);
        // debt_ratio = 1558M / 5000M × 100 = 31.16%
        assert!((p.debt_ratio_pct - 31.16).abs() < 0.01);
        // TTM ROE = (20M × 4) / 3442M × 100 ≈ 2.32%(< 15% threshold,不觸發 RoeHigh)
        // 單季資料走 quarterly × 4 年化 fallback(2026-05-11 TTM 修法)
        let expected_roe = 20_000_000_f64 * 4.0 / 3_442_000_000_f64 * 100.0;
        assert!((p.roe_pct - expected_roe).abs() < 0.001);
        assert!(out.events.iter().all(|e| e.kind != FinancialEventKind::RoeHigh));
    }

    /// 確認英文 key fallback chain work + ROE/ROA 從元值正確計算(net_income=20M,equity=60M → ROE=33%)
    #[test]
    fn assembles_three_types_into_one_point_roe_computed_from_elements() {
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
        assert_eq!(p.total_assets, 100_000_000);
        assert_eq!(p.total_equity, 60_000_000);
        // TTM ROE = (20M × 4) / 60M × 100 = 133.33% → > 15% threshold → RoeHigh fires
        // 單季資料走 quarterly × 4 年化 fallback(2026-05-11 TTM 修法)
        let expected_roe = 20_000_000_f64 * 4.0 / 60_000_000_f64 * 100.0;
        let expected_roa = 20_000_000_f64 * 4.0 / 100_000_000_f64 * 100.0;
        assert!((p.roe_pct - expected_roe).abs() < 0.01);
        assert!((p.roa_pct - expected_roa).abs() < 0.01);
        assert!(out.events.iter().any(|e| e.kind == FinancialEventKind::RoeHigh));
    }

    /// TTM ROE 4 季加總路徑(序列第 4 點起 i >= 3)— 對齊 Buffett annual ROE intent
    #[test]
    fn ttm_roe_uses_four_quarter_sum() {
        // 4 季 net_income: 10M + 15M + 20M + 25M = 70M TTM
        // equity 期末 = 500M → TTM ROE = 70/500 × 100 = 14%(< 15% 不觸發)
        let mk = |d: &str, ni: i64, eq: i64, assets: i64| -> Vec<FinancialStatementRaw> {
            vec![
                FinancialStatementRaw { date: NaiveDate::parse_from_str(d, "%Y-%m-%d").unwrap(),
                    r#type: "income".to_string(),
                    detail: json!({"Revenue": 100_000_000, "GrossProfit": 50_000_000, "EPS": 1.0, "NetIncome": ni}) },
                FinancialStatementRaw { date: NaiveDate::parse_from_str(d, "%Y-%m-%d").unwrap(),
                    r#type: "balance".to_string(),
                    detail: json!({"TotalAssets": assets, "TotalLiabilities": 100_000_000, "TotalEquity": eq}) },
                FinancialStatementRaw { date: NaiveDate::parse_from_str(d, "%Y-%m-%d").unwrap(),
                    r#type: "cashflow".to_string(),
                    detail: json!({"OperatingCashFlow": 30_000_000, "InvestingCashFlow": -10_000_000}) },
            ]
        };
        let mut points = vec![];
        points.extend(mk("2025-03-31", 10_000_000, 500_000_000, 1_000_000_000));
        points.extend(mk("2025-06-30", 15_000_000, 500_000_000, 1_000_000_000));
        points.extend(mk("2025-09-30", 20_000_000, 500_000_000, 1_000_000_000));
        points.extend(mk("2025-12-31", 25_000_000, 500_000_000, 1_000_000_000));
        let series = FinancialStatementSeries { stock_id: "TEST".to_string(), points };
        let out = FinancialStatementCore::new().compute(&series, FinancialStatementParams::default()).unwrap();

        // 第 4 個 quarter(2025-12-31, i=3)用 TTM 4 季加總
        let p4 = &out.series[3];
        let expected_ttm_roe = 70_000_000_f64 / 500_000_000_f64 * 100.0; // 14%
        assert!((p4.roe_pct - expected_ttm_roe).abs() < 0.01,
                "expected TTM ROE=14%, got {}", p4.roe_pct);
        assert!(out.series.iter().all(|p| !p.roe_pct.is_nan()));
        // 第 1 個 quarter(i=0)走 × 4 fallback:10M × 4 / 500M = 8%
        let p1 = &out.series[0];
        assert!((p1.roe_pct - 8.0).abs() < 0.01, "expected fallback ROE=8%, got {}", p1.roe_pct);
    }
}
