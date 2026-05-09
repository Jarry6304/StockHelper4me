// chip_loader — 從 Silver 層 *_derived 表讀取 Chip 資料序列
//
// 對齊 m3Spec/chip_cores.md §2.1(各 chip core 對應的 *Series 與 loader)
// + cores_overview.md §3.4 / §4.4(Cores 一律從 Silver 層讀取)。
//
// 提供:
//   - DayTradingSeries / load_day_trading()
//   - InstitutionalDailySeries / load_institutional_daily()
//   - MarginDailySeries / load_margin_daily()
//   - ForeignHoldingSeries / load_foreign_holding()
//   - HoldingSharesPerSeries / load_holding_shares_per()
//
// **M3 PR-CC1 階段**(day_trading 第 1 個):
//   - 落地 DayTradingSeries + load_day_trading()
//   - 其他 4 種 Series + loader 留下 PR(對齊各 chip core 落地時)

use anyhow::{Context, Result};
use chrono::NaiveDate;
use serde::Serialize;
use sqlx::postgres::PgPool;

// ---------------------------------------------------------------------------
// DayTrading
// ---------------------------------------------------------------------------

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

/// 從 Silver `day_trading_derived` 讀最近 N 天。
///
/// 對齊 m3Spec/chip_cores.md §7.2(day_trading_derived 三欄)。
pub async fn load_day_trading(
    pool: &PgPool,
    stock_id: &str,
    lookback_days: i32,
) -> Result<DayTradingSeries> {
    let points: Vec<DayTradingRaw> = sqlx::query_as(
        r#"
        SELECT date, day_trading_buy, day_trading_sell, day_trading_ratio
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

// 沒 unit test:loader 直接接 PG,沙箱無 PG 不能 mock;留 user 本機 integration test
