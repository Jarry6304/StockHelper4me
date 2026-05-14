// environment_loader — Silver *_derived 讀取 environment 資料序列
//
// 對齊 oldm2Spec/environment_cores.md §2(spec user m3Spec 待寫)+ cores_overview §3.4 / §4.4。
//
// 提供 5 種 Series:
//   - MarketIndexTwSeries(taiex_index_derived)
//   - MarketIndexUsSeries(us_market_index_derived)
//   - ExchangeRateSeries(exchange_rate_derived;PK 含 currency)
//   - FearGreedIndexSeries(暫讀 Bronze fear_greed_index — Silver 沒 derived,§6.2 已知例外)
//   - MarketMarginMaintenanceSeries(market_margin_maintenance_derived)

use anyhow::{Context, Result};
use chrono::NaiveDate;
use serde::Serialize;
use sqlx::postgres::PgPool;

// ===========================================================================
// MarketIndexTw(加權指數)— TAIEX + TPEx 並列大盤,各自獨立保留字
// 對齊 m3Spec/environment_cores.md §3.2 / §3.6:
//   - TAIEX → _index_taiex_
//   - TPEx  → _index_tpex_
// Loader 內部依 Silver 端 stock_id 拆兩條獨立序列
// ===========================================================================

#[derive(Debug, Clone, Serialize)]
pub struct MarketIndexTwSeries {
    pub taiex: Vec<MarketIndexTwRaw>,
    pub tpex: Vec<MarketIndexTwRaw>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct MarketIndexTwRaw {
    pub date: NaiveDate,
    pub open: Option<f64>,
    pub high: Option<f64>,
    pub low: Option<f64>,
    pub close: Option<f64>,
    pub volume: Option<i64>,
}

async fn load_taiex_index(pool: &PgPool, silver_id: &str, lookback_days: i32) -> Result<Vec<MarketIndexTwRaw>> {
    sqlx::query_as(
        r#"
        SELECT date,
               open::float8  AS open,
               high::float8  AS high,
               low::float8   AS low,
               close::float8 AS close,
               volume
        FROM taiex_index_derived
        WHERE stock_id = $1
          AND date >= (CURRENT_DATE - $2::int)
        ORDER BY date ASC
        "#,
    )
    .bind(silver_id).bind(lookback_days).fetch_all(pool).await.with_context(|| format!("load_taiex_index({}) failed", silver_id))
}

pub async fn load_taiex(pool: &PgPool, lookback_days: i32) -> Result<MarketIndexTwSeries> {
    let taiex = load_taiex_index(pool, "TAIEX", lookback_days).await?;
    let tpex = load_taiex_index(pool, "TPEx", lookback_days).await?;
    Ok(MarketIndexTwSeries { taiex, tpex })
}

// ===========================================================================
// MarketIndexUs(SPY / VIX)
// ===========================================================================

#[derive(Debug, Clone, Serialize)]
pub struct MarketIndexUsSeries {
    pub stock_id: String,
    pub points: Vec<MarketIndexUsRaw>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct MarketIndexUsRaw {
    pub date: NaiveDate,
    pub close: Option<f64>,
    pub volume: Option<i64>,
}

pub async fn load_us_market(pool: &PgPool, stock_id: &str, lookback_days: i32) -> Result<MarketIndexUsSeries> {
    let points: Vec<MarketIndexUsRaw> = sqlx::query_as(
        r#"
        SELECT date,
               close::float8 AS close,
               volume
        FROM us_market_index_derived
        WHERE stock_id = $1
          AND date >= (CURRENT_DATE - $2::int)
        ORDER BY date ASC
        "#,
    )
    .bind(stock_id).bind(lookback_days).fetch_all(pool).await.context("load_us_market failed")?;
    Ok(MarketIndexUsSeries { stock_id: stock_id.to_string(), points })
}

/// SPY + VIX 兩 series 的組合(對齊 us_market_core spec §4.6 同 Point 含兩者欄位)
#[derive(Debug, Clone, Serialize)]
pub struct UsMarketCombinedSeries {
    pub spy: MarketIndexUsSeries,
    pub vix: MarketIndexUsSeries,
}

pub async fn load_us_market_combined(pool: &PgPool, lookback_days: i32) -> Result<UsMarketCombinedSeries> {
    let spy = load_us_market(pool, "SPY", lookback_days).await?;
    let vix = load_us_market(pool, "^VIX", lookback_days).await?;
    Ok(UsMarketCombinedSeries { spy, vix })
}

// ===========================================================================
// ExchangeRate(PK 含 currency,不含 stock_id)
// ===========================================================================

#[derive(Debug, Clone, Serialize)]
pub struct ExchangeRateSeries {
    pub currency: String,
    pub points: Vec<ExchangeRateRaw>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ExchangeRateRaw {
    pub date: NaiveDate,
    pub rate: Option<f64>,
}

pub async fn load_exchange_rate(pool: &PgPool, currency: &str, lookback_days: i32) -> Result<ExchangeRateSeries> {
    let points: Vec<ExchangeRateRaw> = sqlx::query_as(
        r#"
        SELECT date,
               rate::float8 AS rate
        FROM exchange_rate_derived
        WHERE currency = $1
          AND date >= (CURRENT_DATE - $2::int)
        ORDER BY date ASC
        "#,
    )
    .bind(currency).bind(lookback_days).fetch_all(pool).await.context("load_exchange_rate failed")?;
    Ok(ExchangeRateSeries { currency: currency.to_string(), points })
}

// ===========================================================================
// FearGreedIndex(架構例外:暫直讀 Bronze)
// ===========================================================================

#[derive(Debug, Clone, Serialize)]
pub struct FearGreedIndexSeries {
    pub points: Vec<FearGreedRaw>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct FearGreedRaw {
    pub date: NaiveDate,
    pub value: Option<f64>,
}

pub async fn load_fear_greed(pool: &PgPool, lookback_days: i32) -> Result<FearGreedIndexSeries> {
    // §6.2 spec 已記:暫直讀 Bronze(無 *_derived);P0 後補 Silver derived 切換
    // PG 表欄名是 `score`(NUMERIC),Rust struct field 是 `value`(對齊 spec §6.5)— alias rename
    let points: Vec<FearGreedRaw> = sqlx::query_as(
        r#"
        SELECT date,
               score::float8 AS value
        FROM fear_greed_index
        WHERE date >= (CURRENT_DATE - $1::int)
        ORDER BY date ASC
        "#,
    )
    .bind(lookback_days).fetch_all(pool).await.context("load_fear_greed failed")?;
    Ok(FearGreedIndexSeries { points })
}

// ===========================================================================
// MarketMarginMaintenance(market_margin_maintenance_derived,PK 不含 stock_id)
// ===========================================================================

#[derive(Debug, Clone, Serialize)]
pub struct MarketMarginMaintenanceSeries {
    pub points: Vec<MarketMarginRaw>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct MarketMarginRaw {
    pub date: NaiveDate,
    pub ratio: Option<f64>,
    pub total_margin_purchase_balance: Option<i64>,
    pub total_short_sale_balance: Option<i64>,
}

pub async fn load_market_margin(pool: &PgPool, lookback_days: i32) -> Result<MarketMarginMaintenanceSeries> {
    let points: Vec<MarketMarginRaw> = sqlx::query_as(
        r#"
        SELECT date,
               ratio::float8 AS ratio,
               total_margin_purchase_balance,
               total_short_sale_balance
        FROM market_margin_maintenance_derived
        WHERE date >= (CURRENT_DATE - $1::int)
        ORDER BY date ASC
        "#,
    )
    .bind(lookback_days).fetch_all(pool).await.context("load_market_margin failed")?;
    Ok(MarketMarginMaintenanceSeries { points })
}

// ===========================================================================
// BusinessIndicator(business_indicator_derived,月頻;PK 含 stock_id='_market_' sentinel)
// 對齊 m3Spec/environment_cores.md §八 r3 — Cores 端用保留字 _index_business_(loader 轉)
// ===========================================================================

#[derive(Debug, Clone, Serialize)]
pub struct BusinessIndicatorSeries {
    pub points: Vec<BusinessIndicatorRaw>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct BusinessIndicatorRaw {
    pub date: NaiveDate,
    pub leading_indicator: Option<f64>,
    pub coincident_indicator: Option<f64>,
    pub lagging_indicator: Option<f64>,
    pub monitoring: Option<i32>,
    pub monitoring_color: Option<String>,  // 'blue'/'yellow_blue'/'green'/'yellow_red'/'red'
}

pub async fn load_business_indicator(pool: &PgPool, lookback_days: i32) -> Result<BusinessIndicatorSeries> {
    let points: Vec<BusinessIndicatorRaw> = sqlx::query_as(
        r#"
        SELECT date,
               leading_indicator::float8    AS leading_indicator,
               coincident_indicator::float8 AS coincident_indicator,
               lagging_indicator::float8    AS lagging_indicator,
               monitoring,
               monitoring_color
        FROM business_indicator_derived
        WHERE stock_id = '_market_'
          AND date >= (CURRENT_DATE - $1::int)
        ORDER BY date ASC
        "#,
    )
    .bind(lookback_days).fetch_all(pool).await.context("load_business_indicator failed")?;
    Ok(BusinessIndicatorSeries { points })
}
