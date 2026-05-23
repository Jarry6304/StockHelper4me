// ohlcv_loader — 從 Silver 層 price_*_fwd 表讀取 OHLCV 序列
//
// 對齊 m3Spec/cores_overview.md §3.4(各 Core Input 由各 loader 提供)
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
/// **v3.37 fix(2026-05-18)**:原本 SQL 用 `date` column 但 price_weekly_fwd 沒有
/// (PK = (market, stock_id, year, week))。改用 (year, week) ORDER BY DESC LIMIT N
/// 取最近 N 週,然後 outer query reverse → ASC(NeelyCore Stage 1 monowave
/// detector 期待時序遞增)。date 從 `make_date(year, 1, 1) + (week-1)*7` 合成。
pub async fn load_weekly(
    pool: &PgPool,
    stock_id: &str,
    lookback_weeks: i32,
) -> Result<OhlcvSeries> {
    let rows: Vec<FwdBarRow> = sqlx::query_as(
        r#"
        WITH ordered AS (
            SELECT
                make_date(year, 1, 1) + INTERVAL '1 day' * ((week - 1) * 7) AS date,
                open::float8  AS open,
                high::float8  AS high,
                low::float8   AS low,
                close::float8 AS close,
                volume
            FROM price_weekly_fwd
            WHERE stock_id = $1
              AND is_dirty = FALSE
              AND open IS NOT NULL AND high IS NOT NULL
              AND low IS NOT NULL AND close IS NOT NULL
            ORDER BY year DESC, week DESC
            LIMIT $2::int
        )
        SELECT date::date AS date, open, high, low, close, volume
        FROM ordered
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
///
/// **v3.37 fix(2026-05-18)**:同 load_weekly,price_monthly_fwd PK=(market,stock_id,
/// year, month) 沒 date column。改用 (year, month) ORDER BY DESC LIMIT N 取最近 N 月,
/// outer reverse → ASC。date 用 `make_date(year, month, 1)` 合成(月初代表性日期)。
pub async fn load_monthly(
    pool: &PgPool,
    stock_id: &str,
    lookback_months: i32,
) -> Result<OhlcvSeries> {
    let rows: Vec<FwdBarRow> = sqlx::query_as(
        r#"
        WITH ordered AS (
            SELECT
                make_date(year, month, 1) AS date,
                open::float8  AS open,
                high::float8  AS high,
                low::float8   AS low,
                close::float8 AS close,
                volume
            FROM price_monthly_fwd
            WHERE stock_id = $1
              AND is_dirty = FALSE
              AND open IS NOT NULL AND high IS NOT NULL
              AND low IS NOT NULL AND close IS NOT NULL
            ORDER BY year DESC, month DESC
            LIMIT $2::int
        )
        SELECT date::date AS date, open, high, low, close, volume
        FROM ordered
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

/// 輔助函式:依 timeframe 自動載入足量 OHLCV(NeelyCore 用)。
///
/// **v3.38(2026-05-18)user 拍版 per-forecast-horizon spec**:
///   支援 1m / 3m / 6m forecast 三 horizon(drop 1y),統一資料窗口拉取:
///   - Daily   = 1,500 bars(~6 yr,覆蓋 6m forecast `daily_bars_required=1500`)
///   - Weekly  = 300 bars(~6 yr,覆蓋 6m forecast `weekly_bars_required=300`)
///   - Monthly = 60 bars(~5 yr,對齊 user 拍版「年級評估不期待精準,monthly 只給
///     long-anchor reference」+ 6m forecast `monthly_bars_required=60`)
///   - Quarterly = warmup_buffered.max(72)(spec 外保留 floor)
///
/// **背景**:v3.36 hotfix 把 daily 推到 6 yr 是為了長 history 股(3030)長 degree
/// scenarios,但 user audit + spec(neely_core_architecture §13.3)揭露 Daily 1-3 yr
/// 對應 Minute degree(剛好是 1m-6m horizon);**長 degree anchor 走 weekly/monthly Neely
/// 而非過度延伸 daily**(對齊 v3.37 multi-timeframe 設計 + NEoWave 原書「各 timeframe
/// 負責自己 degree」哲學)。
///
/// v3.36 daily 6 yr 統計上 ok(對齊 user spec daily_bars_required=1500),保留;
/// v3.36 monthly 144 bars 縮減為 60(對齊 user spec)。
///
/// MCP layer 用 `daily_bars` / `weekly_bars` / `monthly_bars` actual count 走 degradation
/// logic(per-forecast-horizon `degree_uncertain` / `no_6m` / `insufficient_history`)。
///
/// 對齊 cores_overview §3.4 / §7.3 + m3Spec/neely_core_architecture.md §5.4 §8.6 §13.3。
pub async fn load_for_neely(
    pool: &PgPool,
    stock_id: &str,
    params: &NeelyCoreParams,
) -> Result<OhlcvSeries> {
    use fact_schema::WaveCore;
    let core = NeelyCore::new();
    let warmup = core.warmup_periods(params);
    // 1.2x 緩衝(對齊 §7.3 原規格,Quarterly fallback 用)
    let warmup_buffered = (warmup as f64 * 1.2).ceil() as i32;

    // v3.38 user 拍版 fixed table(對齊 per-forecast-horizon spec 完整 6m 需求)
    let lookback = match params.timeframe {
        Timeframe::Daily     => 1500,                       // ~6 yr,6m forecast daily_bars_required
        Timeframe::Weekly    => 300,                        // ~6 yr,6m forecast weekly_bars_required
        Timeframe::Monthly   => 60,                         // ~5 yr,6m forecast monthly_bars_required
        Timeframe::Quarterly => warmup_buffered.max(72),    // Quarterly 不在 user spec,保留 6 yr floor
    };

    match params.timeframe {
        Timeframe::Daily => load_daily(pool, stock_id, lookback).await,
        Timeframe::Weekly => load_weekly(pool, stock_id, lookback).await,
        Timeframe::Monthly => load_monthly(pool, stock_id, lookback).await,
        Timeframe::Quarterly => Err(anyhow::anyhow!(
            "ohlcv_loader: Timeframe::Quarterly 不適用 OHLCV(Quarterly 為 financial_statement \
             季頻財報專用,沒對應 price_*_fwd 表)"
        )),
    }
}

// ─── PIT-aware loader(v0.3 spec phase 3,2026-05-23)─────────────────────
//
// Reconstructs as-of-T OHLCV view from raw Bronze tables (price_daily +
// price_adjustment_events).  Critical for backtest paths: `price_daily_fwd`
// bakes in future events, but `load_asof_daily(stock, asof_t, lookback)` only
// applies AF for events with date ≤ asof_t.
//
// Python mirror: src/pit/ohlcv.py::asof_close_series.  AF formula priorities
// mirror silver_s1_adjustment::derive_simple_event_af (Priority 1 API exact /
// Priority 2 dividend fallback / Priority 3 capital_increase from detail).
//
// Forward loop rule: "先 push 再更新 multiplier" — multiplier for row T₀ =
// product of AFs for events with date strictly > T₀.

#[derive(Debug, Clone, sqlx::FromRow)]
struct RawPriceRow {
    date: NaiveDate,
    open: Option<f64>,
    high: Option<f64>,
    low: Option<f64>,
    close: Option<f64>,
    volume: Option<i64>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct AdjEventRow {
    date: NaiveDate,
    event_type: String,
    before_price: Option<f64>,
    reference_price: Option<f64>,
    cash_dividend: Option<f64>,
    stock_dividend: Option<f64>,
    volume_factor: f64,
    detail: Option<serde_json::Value>,
}

/// Compute (af, vf) for a single event.  Mirror of Python
/// `src.pit.ohlcv._compute_event_af`.
fn compute_event_af(ev: &AdjEventRow, raw_prev_close: Option<f64>) -> (f64, f64) {
    let cash = ev.cash_dividend.unwrap_or(0.0);
    let stock = ev.stock_dividend.unwrap_or(0.0);
    let vf = ev.volume_factor;

    // Priority 1: API exact values with reliability check
    if let (Some(bp), Some(rp)) = (ev.before_price, ev.reference_price) {
        if rp > 0.0 && bp > 0.0 {
            let bp_eq_rp = (bp - rp).abs() < 1e-4;
            let unreliable = ev.event_type == "dividend"
                && stock > 0.0
                && cash == 0.0
                && bp_eq_rp;
            if !unreliable {
                return (bp / rp, vf);
            }
        }
    }

    // Priority 2: dividend fallback formula
    if ev.event_type == "dividend" {
        let bp_use = ev.before_price.or(raw_prev_close);
        if let Some(bp) = bp_use {
            if bp > 0.0 && (cash > 0.0 || stock > 0.0) {
                let p_after = (bp - cash) / (1.0 + stock / 10.0);
                if p_after > 0.0 {
                    return (bp / p_after, vf);
                }
            }
        }
    }

    // Priority 3: capital_increase from detail JSONB
    if ev.event_type == "capital_increase" {
        if let Some(detail) = &ev.detail {
            let sub_price = detail.get("subscription_price").and_then(|v| v.as_f64());
            let sub_rate = detail.get("subscription_rate_raw").and_then(|v| v.as_f64());
            if let (Some(sub_price), Some(sub_rate), Some(prev)) = (sub_price, sub_rate, raw_prev_close) {
                if sub_price > 0.0 && sub_rate > 0.0 && prev > 0.0 {
                    let r = sub_rate / 1000.0;
                    let after_price = (prev + sub_price * r) / (1.0 + r);
                    if after_price > 0.0 {
                        return (prev / after_price, vf);
                    }
                }
            }
        }
    }

    // Default: no adjustment
    (1.0, vf)
}

/// As-of-T view of daily OHLC from raw Bronze.
///
/// Returns OhlcvSeries with bars adjusted using only events with date ≤ asof.
/// For asof = today, this gives the same as price_daily_fwd; for any earlier
/// asof, no future events leak into past bars.
///
/// `lookback_days`: calendar days back from asof to fetch.  Caller is
/// responsible for ensuring enough history for downstream warmup needs.
pub async fn load_asof_daily(
    pool: &PgPool,
    stock_id: &str,
    asof: NaiveDate,
    lookback_days: i32,
) -> Result<OhlcvSeries> {
    let earliest = asof - chrono::Duration::days(lookback_days as i64);

    let raw_rows: Vec<RawPriceRow> = sqlx::query_as(
        r#"
        SELECT date,
               open::float8  AS open,
               high::float8  AS high,
               low::float8   AS low,
               close::float8 AS close,
               volume
        FROM price_daily
        WHERE market = 'TW'
          AND stock_id = $1
          AND date >= $2 AND date <= $3
          AND open IS NOT NULL AND high IS NOT NULL
          AND low IS NOT NULL AND close IS NOT NULL
        ORDER BY date ASC
        "#,
    )
    .bind(stock_id)
    .bind(earliest)
    .bind(asof)
    .fetch_all(pool)
    .await
    .context("load_asof_daily: query price_daily failed")?;

    if raw_rows.is_empty() {
        return Ok(OhlcvSeries {
            stock_id: stock_id.to_string(),
            timeframe: Timeframe::Daily,
            bars: vec![],
        });
    }

    let earliest_raw = raw_rows[0].date;
    let event_rows: Vec<AdjEventRow> = sqlx::query_as(
        r#"
        SELECT date,
               event_type,
               before_price::float8     AS before_price,
               reference_price::float8  AS reference_price,
               cash_dividend::float8    AS cash_dividend,
               stock_dividend::float8   AS stock_dividend,
               volume_factor::float8    AS volume_factor,
               detail
        FROM price_adjustment_events
        WHERE market = 'TW'
          AND stock_id = $1
          AND date > $2 AND date <= $3
        ORDER BY date ASC
        "#,
    )
    .bind(stock_id)
    .bind(earliest_raw)
    .bind(asof)
    .fetch_all(pool)
    .await
    .context("load_asof_daily: query price_adjustment_events failed")?;

    // Build per-date event multipliers (same-day events combine multiplicatively).
    use std::collections::HashMap;
    let close_by_date: HashMap<NaiveDate, f64> = raw_rows
        .iter()
        .filter_map(|r| r.close.map(|c| (r.date, c)))
        .collect();
    let raw_dates: Vec<NaiveDate> = raw_rows.iter().map(|r| r.date).collect();

    let mut event_af: HashMap<NaiveDate, f64> = HashMap::new();
    let mut event_vf: HashMap<NaiveDate, f64> = HashMap::new();
    for ev in &event_rows {
        let prev_close = raw_dates
            .iter()
            .rev()
            .find(|d| **d < ev.date)
            .and_then(|d| close_by_date.get(d).copied());
        let (af, vf) = compute_event_af(ev, prev_close);
        if (af - 1.0).abs() > 1e-12 {
            *event_af.entry(ev.date).or_insert(1.0) *= af;
        }
        if (vf - 1.0).abs() > 1e-12 {
            *event_vf.entry(ev.date).or_insert(1.0) *= vf;
        }
    }

    // Forward loop in reverse: push current row first, then update multiplier
    // for earlier dates.
    let mut result_reversed: Vec<OhlcvBar> = Vec::with_capacity(raw_rows.len());
    let mut price_mult = 1.0_f64;
    let mut volume_mult = 1.0_f64;
    for r in raw_rows.iter().rev() {
        // SQL filters NULLs but defense-in-depth via Option chain
        let (Some(open), Some(high), Some(low), Some(close)) =
            (r.open, r.high, r.low, r.close)
        else {
            continue;
        };
        let adj_volume = r.volume.map(|v| {
            ((v as f64) / volume_mult).round() as i64
        });
        result_reversed.push(OhlcvBar {
            date: r.date,
            open: (open * price_mult * 10000.0).round() / 10000.0,
            high: (high * price_mult * 10000.0).round() / 10000.0,
            low: (low * price_mult * 10000.0).round() / 10000.0,
            close: (close * price_mult * 10000.0).round() / 10000.0,
            volume: adj_volume,
        });
        if let Some(&af) = event_af.get(&r.date) {
            price_mult *= af;
        }
        if let Some(&vf) = event_vf.get(&r.date) {
            volume_mult *= vf;
        }
    }
    result_reversed.reverse();

    Ok(OhlcvSeries {
        stock_id: stock_id.to_string(),
        timeframe: Timeframe::Daily,
        bars: result_reversed,
    })
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
