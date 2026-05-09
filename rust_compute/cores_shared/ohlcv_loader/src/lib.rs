// ohlcv_loader — 從 Silver 層 price_*_fwd 表讀取 OHLCV 序列
//
// 對齊 m2Spec/oldm2Spec/cores_overview.md §3.4(各 Core Input 由各 loader 提供)
// + §4.4(Cores 一律從 Silver 層讀取,不直接讀 Bronze)。
//
// 提供:
//   - load_daily(pool, stock_id, lookback_days):讀 price_daily_fwd 最近 N 天
//   - load_weekly(pool, stock_id, lookback_weeks):讀 price_weekly_fwd
//   - load_monthly(pool, stock_id, lookback_months):讀 price_monthly_fwd
//   - load_for_neely(pool, stock_id, timeframe, params):輔助函式 — 套 NeelyCore.warmup_periods
//
// **M3 PR-7 階段**(沙箱無 PG 不能 end-to-end test):
//   - 完整 sqlx 實作落地
//   - cargo build 通過
//   - 真實連 PG 驗證留 user 本機(本機 Postgres 17 + alembic upgrade head 後)

use anyhow::{Context, Result};
use chrono::NaiveDate;
use fact_schema::Timeframe;
use neely_core::NeelyCore;
use neely_core::NeelyCoreParams;
use sqlx::postgres::PgPool;

// Re-export 給 indicator cores 共用(避免它們再 dep neely_core)
pub use neely_core::output::{OhlcvBar, OhlcvSeries};

/// price_daily_fwd / price_weekly_fwd / price_monthly_fwd 三表 row 共用結構
#[derive(Debug, Clone, sqlx::FromRow)]
struct FwdBarRow {
    /// 日線:date 直接拿;週/月線:用 (year, week / month) 計算
    date: NaiveDate,
    open: Option<f64>,
    high: Option<f64>,
    low: Option<f64>,
    close: Option<f64>,
    volume: Option<i64>,
}

/// 從 price_daily_fwd 讀最近 N 天,回傳 OhlcvSeries(對齊 NeelyCore.Input)。
///
/// 篩選:`stock_id = $1` AND `date BETWEEN (today - lookback_days) AND today`
/// 排序:date ASC(NeelyCore Stage 1 monowave detector 期待時序遞增)
/// NULL 篩除:open/high/low/close 任一 NULL 的 row 跳過(Silver 層應已濾,
/// 這裡是 defense-in-depth)
pub async fn load_daily(
    pool: &PgPool,
    stock_id: &str,
    lookback_days: i32,
) -> Result<OhlcvSeries> {
    let rows: Vec<FwdBarRow> = sqlx::query_as(
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
    .context("load_daily: query price_daily_fwd failed")?;

    Ok(rows_to_series(stock_id, Timeframe::Daily, rows))
}

/// 從 price_weekly_fwd 讀最近 N 週。
///
/// price_weekly_fwd PK = (market, stock_id, year, week);本 loader 只篩 stock_id,
/// 用 (year, week) 排序倒推最近 N 週。week 對應週末日期由 Silver builder 已寫入 date 欄位。
pub async fn load_weekly(
    pool: &PgPool,
    stock_id: &str,
    lookback_weeks: i32,
) -> Result<OhlcvSeries> {
    // price_weekly_fwd 不一定有 date 欄,部分 schema 用 (year, week);
    // 假設 date 欄存在(對齊 Silver builder),否則本 query 需改 (year, week) 推算
    let rows: Vec<FwdBarRow> = sqlx::query_as(
        r#"
        SELECT date,
               open::float8  AS open,
               high::float8  AS high,
               low::float8   AS low,
               close::float8 AS close,
               volume
        FROM price_weekly_fwd
        WHERE stock_id = $1
          AND is_dirty = FALSE
          AND date >= (CURRENT_DATE - ($2::int * 7))
          AND open IS NOT NULL AND high IS NOT NULL
          AND low IS NOT NULL AND close IS NOT NULL
        ORDER BY date ASC
        "#,
    )
    .bind(stock_id)
    .bind(lookback_weeks)
    .fetch_all(pool)
    .await
    .context("load_weekly: query price_weekly_fwd failed")?;

    Ok(rows_to_series(stock_id, Timeframe::Weekly, rows))
}

/// 從 price_monthly_fwd 讀最近 N 月。
pub async fn load_monthly(
    pool: &PgPool,
    stock_id: &str,
    lookback_months: i32,
) -> Result<OhlcvSeries> {
    let rows: Vec<FwdBarRow> = sqlx::query_as(
        r#"
        SELECT date,
               open::float8  AS open,
               high::float8  AS high,
               low::float8   AS low,
               close::float8 AS close,
               volume
        FROM price_monthly_fwd
        WHERE stock_id = $1
          AND is_dirty = FALSE
          AND date >= (CURRENT_DATE - ($2::int * 31))
          AND open IS NOT NULL AND high IS NOT NULL
          AND low IS NOT NULL AND close IS NOT NULL
        ORDER BY date ASC
        "#,
    )
    .bind(stock_id)
    .bind(lookback_months)
    .fetch_all(pool)
    .await
    .context("load_monthly: query price_monthly_fwd failed")?;

    Ok(rows_to_series(stock_id, Timeframe::Monthly, rows))
}

/// 輔助函式:依 NeelyCore.warmup_periods(params) 自動載入足量 OHLCV。
///
/// Daily / Weekly / Monthly 對應 warmup * 1.2 (緩衝 20%) 量的歷史。
/// 對齊 cores_overview §3.4 / §7.3。
pub async fn load_for_neely(
    pool: &PgPool,
    stock_id: &str,
    params: &NeelyCoreParams,
) -> Result<OhlcvSeries> {
    use fact_schema::WaveCore;
    let core = NeelyCore::new();
    let warmup = core.warmup_periods(params);
    // 1.2x 緩衝(對齊 §7.3)
    let lookback = (warmup as f64 * 1.2).ceil() as i32;

    match params.timeframe {
        Timeframe::Daily => load_daily(pool, stock_id, lookback).await,
        Timeframe::Weekly => load_weekly(pool, stock_id, lookback).await,
        Timeframe::Monthly => load_monthly(pool, stock_id, lookback).await,
    }
}

fn rows_to_series(stock_id: &str, timeframe: Timeframe, rows: Vec<FwdBarRow>) -> OhlcvSeries {
    let bars: Vec<OhlcvBar> = rows
        .into_iter()
        .filter_map(|r| {
            // NULL 篩除已在 SQL,這裡再 defense-in-depth(unwrap 以已 filtered 假設)
            Some(OhlcvBar {
                date: r.date,
                open: r.open?,
                high: r.high?,
                low: r.low?,
                close: r.close?,
                volume: r.volume,
            })
        })
        .collect();

    OhlcvSeries {
        stock_id: stock_id.to_string(),
        timeframe,
        bars,
    }
}

// 沒 unit test:loader 直接接 PG,沙箱無 PG 不能 mock;留 user 本機 integration test
// (PR-7 後續 + P0 Gate 校準時做)
