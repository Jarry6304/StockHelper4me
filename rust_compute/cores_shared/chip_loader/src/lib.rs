// chip_loader — 從 Silver 層 *_derived 表讀取 Chip 資料序列
//
// 對齊 m3Spec/chip_cores.md §2.1 + cores_overview.md §3.4 / §4.4。
//
// 提供 5 種 Series + 對應 load_*():
//   - DayTradingSeries          → load_day_trading()       (PR-CC1)
//   - InstitutionalDailySeries  → load_institutional_daily() (PR-CC2)
//   - MarginDailySeries         → load_margin_daily()        (PR-CC3)
//   - ForeignHoldingSeries      → load_foreign_holding()     (PR-CC4)
//   - HoldingSharesPerSeries    → load_holding_shares_per()  (PR-CC5)

use anyhow::{Context, Result};
use chrono::NaiveDate;
use serde::Serialize;
use sqlx::postgres::PgPool;

// ===========================================================================
// DayTrading(PR-CC1)
// ===========================================================================

#[derive(Debug, Clone, Serialize)]
pub struct DayTradingSeries {
    pub stock_id: String,
    pub points: Vec<DayTradingRaw>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct DayTradingRaw {
    pub date: NaiveDate,
    pub day_trading_buy: Option<i64>,
    pub day_trading_sell: Option<i64>,
    pub day_trading_ratio: Option<f64>,
}

pub async fn load_day_trading(
    pool: &PgPool,
    stock_id: &str,
    lookback_days: i32,
) -> Result<DayTradingSeries> {
    // day_trading_ratio NUMERIC(10,4):explicit cast → float8 對齊 Rust Option<f64>
    let points: Vec<DayTradingRaw> = sqlx::query_as(
        r#"
        SELECT date,
               day_trading_buy,
               day_trading_sell,
               day_trading_ratio::float8 AS day_trading_ratio
        FROM day_trading_derived
        WHERE stock_id = $1
          AND date >= (CURRENT_DATE - $2::int)
        ORDER BY date ASC
        "#,
    )
    .bind(stock_id)
    .bind(lookback_days)
    .fetch_all(pool)
    .await
    .context("load_day_trading: query day_trading_derived failed")?;

    Ok(DayTradingSeries {
        stock_id: stock_id.to_string(),
        points,
    })
}

// ===========================================================================
// InstitutionalDaily(PR-CC2)
// ===========================================================================

#[derive(Debug, Clone, Serialize)]
pub struct InstitutionalDailySeries {
    pub stock_id: String,
    pub points: Vec<InstitutionalDailyRaw>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct InstitutionalDailyRaw {
    pub date: NaiveDate,
    pub foreign_buy: Option<i64>,
    pub foreign_sell: Option<i64>,
    pub investment_trust_buy: Option<i64>,
    pub investment_trust_sell: Option<i64>,
    pub dealer_buy: Option<i64>,
    pub dealer_sell: Option<i64>,
    pub dealer_hedging_buy: Option<i64>,
    pub dealer_hedging_sell: Option<i64>,
    pub gov_bank_net: Option<i64>,
}

impl InstitutionalDailyRaw {
    pub fn foreign_net(&self) -> i64 {
        self.foreign_buy.unwrap_or(0) - self.foreign_sell.unwrap_or(0)
    }
    pub fn trust_net(&self) -> i64 {
        self.investment_trust_buy.unwrap_or(0) - self.investment_trust_sell.unwrap_or(0)
    }
    pub fn dealer_net(&self) -> i64 {
        // 自營商 = 自行 + 避險(spec §3.5 dealer_net 含 hedging)
        let main = self.dealer_buy.unwrap_or(0) - self.dealer_sell.unwrap_or(0);
        let hedge = self.dealer_hedging_buy.unwrap_or(0) - self.dealer_hedging_sell.unwrap_or(0);
        main + hedge
    }
    pub fn total_net(&self) -> i64 {
        self.foreign_net() + self.trust_net() + self.dealer_net()
    }
}

pub async fn load_institutional_daily(
    pool: &PgPool,
    stock_id: &str,
    lookback_days: i32,
) -> Result<InstitutionalDailySeries> {
    let points: Vec<InstitutionalDailyRaw> = sqlx::query_as(
        r#"
        SELECT date,
               foreign_buy, foreign_sell,
               investment_trust_buy, investment_trust_sell,
               dealer_buy, dealer_sell,
               dealer_hedging_buy, dealer_hedging_sell,
               gov_bank_net
        FROM institutional_daily_derived
        WHERE stock_id = $1
          AND date >= (CURRENT_DATE - $2::int)
        ORDER BY date ASC
        "#,
    )
    .bind(stock_id)
    .bind(lookback_days)
    .fetch_all(pool)
    .await
    .context("load_institutional_daily query failed")?;

    Ok(InstitutionalDailySeries {
        stock_id: stock_id.to_string(),
        points,
    })
}

// ===========================================================================
// MarginDaily(PR-CC3)
// ===========================================================================

#[derive(Debug, Clone, Serialize)]
pub struct MarginDailySeries {
    pub stock_id: String,
    pub points: Vec<MarginDailyRaw>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct MarginDailyRaw {
    pub date: NaiveDate,
    pub margin_purchase: Option<i64>,
    pub margin_sell: Option<i64>,
    pub margin_balance: Option<i64>,
    pub short_sale: Option<i64>,
    pub short_cover: Option<i64>,
    pub short_balance: Option<i64>,
    /// 維持率 %(若 Silver 有提供;對齊 spec §4.5 margin_maintenance,沒則 NULL)
    pub margin_maintenance: Option<f64>,
}

pub async fn load_margin_daily(
    pool: &PgPool,
    stock_id: &str,
    lookback_days: i32,
) -> Result<MarginDailySeries> {
    // 注意:Silver margin_daily_derived 有些欄位可能不存在(如 margin_maintenance);
    // PR-CC3 階段假設 schema 有 — 若無 user 本機跑 cargo run 會報 missing column,
    // 留 follow-up 對齊 layered_schema_post_refactor.md 校準
    let points: Vec<MarginDailyRaw> = sqlx::query_as(
        r#"
        SELECT date,
               margin_purchase, margin_sell, margin_balance,
               short_sale, short_cover, short_balance,
               NULL::float8 AS margin_maintenance
        FROM margin_daily_derived
        WHERE stock_id = $1
          AND date >= (CURRENT_DATE - $2::int)
        ORDER BY date ASC
        "#,
    )
    .bind(stock_id)
    .bind(lookback_days)
    .fetch_all(pool)
    .await
    .context("load_margin_daily query failed")?;

    Ok(MarginDailySeries {
        stock_id: stock_id.to_string(),
        points,
    })
}

// ===========================================================================
// ForeignHolding(PR-CC4)
// ===========================================================================

#[derive(Debug, Clone, Serialize)]
pub struct ForeignHoldingSeries {
    pub stock_id: String,
    pub points: Vec<ForeignHoldingRaw>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ForeignHoldingRaw {
    pub date: NaiveDate,
    pub foreign_holding_shares: Option<i64>,
    pub foreign_holding_ratio: Option<f64>,
    /// 外資投資上限 %(若 Silver detail JSONB 有 expose 為欄位;否則 NULL)
    /// 對齊 m3Spec/chip_cores.md §5.5 foreign_limit_pct
    pub foreign_limit_pct: Option<f64>,
}

pub async fn load_foreign_holding(
    pool: &PgPool,
    stock_id: &str,
    lookback_days: i32,
) -> Result<ForeignHoldingSeries> {
    // foreign_holding_ratio NUMERIC(8,4):explicit cast → float8 對齊 Rust Option<f64>
    // foreign_limit_pct 目前 Silver 沒 expose 為 stored col,先 NULL placeholder
    let points: Vec<ForeignHoldingRaw> = sqlx::query_as(
        r#"
        SELECT date,
               foreign_holding_shares,
               foreign_holding_ratio::float8 AS foreign_holding_ratio,
               NULL::float8                  AS foreign_limit_pct
        FROM foreign_holding_derived
        WHERE stock_id = $1
          AND date >= (CURRENT_DATE - $2::int)
        ORDER BY date ASC
        "#,
    )
    .bind(stock_id)
    .bind(lookback_days)
    .fetch_all(pool)
    .await
    .context("load_foreign_holding query failed")?;

    Ok(ForeignHoldingSeries {
        stock_id: stock_id.to_string(),
        points,
    })
}

// ===========================================================================
// HoldingSharesPer(PR-CC5)
// ===========================================================================

#[derive(Debug, Clone, Serialize)]
pub struct HoldingSharesPerSeries {
    pub stock_id: String,
    pub points: Vec<HoldingSharesPerRaw>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct HoldingSharesPerRaw {
    pub date: NaiveDate,
    /// detail JSONB(含 level taxonomy:小戶 / 中實 / 大戶 buckets)
    pub detail: serde_json::Value,
}

pub async fn load_holding_shares_per(
    pool: &PgPool,
    stock_id: &str,
    lookback_days: i32,
) -> Result<HoldingSharesPerSeries> {
    let points: Vec<HoldingSharesPerRaw> = sqlx::query_as(
        r#"
        SELECT date, detail
        FROM holding_shares_per_derived
        WHERE stock_id = $1
          AND date >= (CURRENT_DATE - $2::int)
        ORDER BY date ASC
        "#,
    )
    .bind(stock_id)
    .bind(lookback_days)
    .fetch_all(pool)
    .await
    .context("load_holding_shares_per query failed")?;

    Ok(HoldingSharesPerSeries {
        stock_id: stock_id.to_string(),
        points,
    })
}

// 沒 unit test:loader 直接接 PG,沙箱無 PG 不能 mock;留 user 本機 integration test
