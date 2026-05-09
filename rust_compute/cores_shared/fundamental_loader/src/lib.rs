// fundamental_loader — Silver *_derived 讀取 fundamental 資料序列
//
// 對齊 oldm2Spec/fundamental_cores.md §2(待 user m3Spec 補)+ cores_overview §3.4 / §4.4。

use anyhow::{Context, Result};
use chrono::NaiveDate;
use serde::Serialize;
use sqlx::postgres::PgPool;

// ===========================================================================
// MonthlyRevenue
// ===========================================================================

#[derive(Debug, Clone, Serialize)]
pub struct MonthlyRevenueSeries {
    pub stock_id: String,
    pub points: Vec<MonthlyRevenueRaw>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct MonthlyRevenueRaw {
    pub date: NaiveDate,
    pub revenue: Option<i64>,
    /// YoY 成長率(%)— Silver 已算
    pub revenue_yoy: Option<f64>,
    /// MoM 成長率(%)
    pub revenue_mom: Option<f64>,
    /// detail JSONB,可能含 report_date / cumulative 等(spec §3.2)
    pub detail: Option<serde_json::Value>,
}

pub async fn load_monthly_revenue(
    pool: &PgPool,
    stock_id: &str,
    lookback_months: i32,
) -> Result<MonthlyRevenueSeries> {
    // schema:revenue NUMERIC(20,2) / revenue_yoy NUMERIC(10,4) / revenue_mom NUMERIC(10,4)
    // Rust struct:revenue Option<i64>(對齊 RevenueCore expects i64 元為單位)
    //              yoy/mom Option<f64>;使用 explicit cast 避免 sqlx NUMERIC vs FLOAT8 mismatch
    let points: Vec<MonthlyRevenueRaw> = sqlx::query_as(
        r#"
        SELECT date,
               revenue::int8        AS revenue,
               revenue_yoy::float8  AS revenue_yoy,
               revenue_mom::float8  AS revenue_mom,
               detail
        FROM monthly_revenue_derived
        WHERE stock_id = $1
          AND date >= (CURRENT_DATE - ($2::int * 31))
        ORDER BY date ASC
        "#,
    )
    .bind(stock_id)
    .bind(lookback_months)
    .fetch_all(pool)
    .await
    .context("load_monthly_revenue query failed")?;

    Ok(MonthlyRevenueSeries { stock_id: stock_id.to_string(), points })
}

// ===========================================================================
// ValuationDaily
// ===========================================================================

#[derive(Debug, Clone, Serialize)]
pub struct ValuationDailySeries {
    pub stock_id: String,
    pub points: Vec<ValuationDailyRaw>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ValuationDailyRaw {
    pub date: NaiveDate,
    pub per: Option<f64>,
    pub pbr: Option<f64>,
    pub dividend_yield: Option<f64>,
    pub market_value_weight: Option<f64>,
}

pub async fn load_valuation_daily(
    pool: &PgPool,
    stock_id: &str,
    lookback_days: i32,
) -> Result<ValuationDailySeries> {
    // schema 4 個欄全 NUMERIC,explicit cast 避免 sqlx NUMERIC vs FLOAT8 mismatch
    let points: Vec<ValuationDailyRaw> = sqlx::query_as(
        r#"
        SELECT date,
               per::float8                 AS per,
               pbr::float8                 AS pbr,
               dividend_yield::float8      AS dividend_yield,
               market_value_weight::float8 AS market_value_weight
        FROM valuation_daily_derived
        WHERE stock_id = $1
          AND date >= (CURRENT_DATE - $2::int)
        ORDER BY date ASC
        "#,
    )
    .bind(stock_id)
    .bind(lookback_days)
    .fetch_all(pool)
    .await
    .context("load_valuation_daily query failed")?;

    Ok(ValuationDailySeries { stock_id: stock_id.to_string(), points })
}

// ===========================================================================
// FinancialStatement(季頻;PK 含 type:income / balance / cashflow)
// ===========================================================================

#[derive(Debug, Clone, Serialize)]
pub struct FinancialStatementSeries {
    pub stock_id: String,
    pub points: Vec<FinancialStatementRaw>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct FinancialStatementRaw {
    pub date: NaiveDate,
    pub r#type: String, // income / balance / cashflow
    pub detail: serde_json::Value,
}

pub async fn load_financial_statement(
    pool: &PgPool,
    stock_id: &str,
    lookback_quarters: i32,
) -> Result<FinancialStatementSeries> {
    // 季頻 ~91 天/季
    let points: Vec<FinancialStatementRaw> = sqlx::query_as(
        r#"
        SELECT date, type, detail
        FROM financial_statement_derived
        WHERE stock_id = $1
          AND date >= (CURRENT_DATE - ($2::int * 91))
        ORDER BY date ASC, type ASC
        "#,
    )
    .bind(stock_id)
    .bind(lookback_quarters)
    .fetch_all(pool)
    .await
    .context("load_financial_statement query failed")?;

    Ok(FinancialStatementSeries { stock_id: stock_id.to_string(), points })
}
