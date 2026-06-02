// loader.rs — Traditional Core 自有 Silver 讀取(直讀 price_*_fwd → TradOhlcvSeries)
//
// **解耦理由**:共用 `ohlcv_loader` 會 dep `neely_core` 並回傳 neely `OhlcvSeries`(re-export),
// 故 Traditional Core 自帶 loader。SQL 形狀對齊 `ohlcv_loader`(後復權 + 漲跌停已於 Silver 處理),
// 但回傳自有 `TradOhlcvSeries`、不 import 任何 neely 型別。
//
// 對齊 m3Spec/cores_overview.md §4.4(Cores 一律讀 Silver、不讀 Bronze)。

use crate::config::TraditionalEngineConfig;
use crate::output::{TradBar, TradOhlcvSeries};
use anyhow::{Context, Result};
use chrono::NaiveDate;
use fact_schema::Timeframe;
use sqlx::postgres::PgPool;

#[derive(Debug, Clone, sqlx::FromRow)]
struct FwdRow {
    date: NaiveDate,
    open: Option<f64>,
    high: Option<f64>,
    low: Option<f64>,
    close: Option<f64>,
    volume: Option<i64>,
}

fn rows_to_series(stock_id: &str, timeframe: Timeframe, rows: Vec<FwdRow>) -> TradOhlcvSeries {
    let bars = rows
        .into_iter()
        .filter_map(|r| match (r.open, r.high, r.low, r.close) {
            (Some(open), Some(high), Some(low), Some(close)) => Some(TradBar {
                date: r.date,
                open,
                high,
                low,
                close,
                volume: r.volume,
            }),
            _ => None,
        })
        .collect();
    TradOhlcvSeries {
        stock_id: stock_id.to_string(),
        timeframe,
        bars,
    }
}

/// 讀 price_daily_fwd 最近 N 天(date ASC)。
pub async fn load_daily(pool: &PgPool, stock_id: &str, lookback_days: i32) -> Result<TradOhlcvSeries> {
    let rows: Vec<FwdRow> = sqlx::query_as(
        r#"
        SELECT date,
               open::float8  AS open,
               high::float8  AS high,
               low::float8   AS low,
               close::float8 AS close,
               volume
        FROM price_daily_fwd
        WHERE stock_id = $1
          AND is_dirty = FALSE
          AND date >= (CURRENT_DATE - $2::int)
          AND open IS NOT NULL AND high IS NOT NULL
          AND low IS NOT NULL AND close IS NOT NULL
        ORDER BY date ASC
        "#,
    )
    .bind(stock_id)
    .bind(lookback_days)
    .fetch_all(pool)
    .await
    .context("traditional_core::loader::load_daily: query price_daily_fwd failed")?;
    Ok(rows_to_series(stock_id, Timeframe::Daily, rows))
}

/// 讀 price_weekly_fwd 最近 N 週(PK=(market,stock_id,year,week),date 由 year+week 合成)。
pub async fn load_weekly(pool: &PgPool, stock_id: &str, lookback_weeks: i32) -> Result<TradOhlcvSeries> {
    let rows: Vec<FwdRow> = sqlx::query_as(
        r#"
        WITH ordered AS (
            SELECT make_date(year, 1, 1) + INTERVAL '1 day' * ((week - 1) * 7) AS date,
                   open::float8 AS open, high::float8 AS high,
                   low::float8 AS low, close::float8 AS close, volume
            FROM price_weekly_fwd
            WHERE stock_id = $1 AND is_dirty = FALSE
              AND open IS NOT NULL AND high IS NOT NULL
              AND low IS NOT NULL AND close IS NOT NULL
            ORDER BY year DESC, week DESC
            LIMIT $2::int
        )
        SELECT date::date AS date, open, high, low, close, volume
        FROM ordered ORDER BY date ASC
        "#,
    )
    .bind(stock_id)
    .bind(lookback_weeks)
    .fetch_all(pool)
    .await
    .context("traditional_core::loader::load_weekly: query price_weekly_fwd failed")?;
    Ok(rows_to_series(stock_id, Timeframe::Weekly, rows))
}

/// 讀 price_monthly_fwd 最近 N 月(date 由 year+month 合成,月初)。
pub async fn load_monthly(pool: &PgPool, stock_id: &str, lookback_months: i32) -> Result<TradOhlcvSeries> {
    let rows: Vec<FwdRow> = sqlx::query_as(
        r#"
        WITH ordered AS (
            SELECT make_date(year, month, 1) AS date,
                   open::float8 AS open, high::float8 AS high,
                   low::float8 AS low, close::float8 AS close, volume
            FROM price_monthly_fwd
            WHERE stock_id = $1 AND is_dirty = FALSE
              AND open IS NOT NULL AND high IS NOT NULL
              AND low IS NOT NULL AND close IS NOT NULL
            ORDER BY year DESC, month DESC
            LIMIT $2::int
        )
        SELECT date::date AS date, open, high, low, close, volume
        FROM ordered ORDER BY date ASC
        "#,
    )
    .bind(stock_id)
    .bind(lookback_months)
    .fetch_all(pool)
    .await
    .context("traditional_core::loader::load_monthly: query price_monthly_fwd failed")?;
    Ok(rows_to_series(stock_id, Timeframe::Monthly, rows))
}

/// 依 config.timeframe 自動載入足量 OHLCV(對齊 ohlcv_loader fixed table:daily 1500 / weekly 300 / monthly 60)。
pub async fn load_for_timeframe(
    pool: &PgPool,
    stock_id: &str,
    config: &TraditionalEngineConfig,
) -> Result<TradOhlcvSeries> {
    match config.timeframe {
        Timeframe::Daily => load_daily(pool, stock_id, 1500).await,
        Timeframe::Weekly => load_weekly(pool, stock_id, 300).await,
        Timeframe::Monthly => load_monthly(pool, stock_id, 60).await,
        Timeframe::Quarterly => Err(anyhow::anyhow!(
            "traditional_core: Timeframe::Quarterly 無對應 price_*_fwd 表(季頻僅 financial_statement 用)"
        )),
    }
}
