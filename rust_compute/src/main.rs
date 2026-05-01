//! tw_stock_compute — Phase 4 後復權計算與 K 線聚合 (C5 完整版)

use std::collections::HashMap;
use std::time::Instant;

use anyhow::{Context, Result};
use chrono::{Datelike, NaiveDate};
use clap::Parser;
use serde::Serialize;
use serde_json::Value;
use sqlx::postgres::{PgPool, PgPoolOptions};

const EXPECTED_SCHEMA_VERSION: &str = "2.0";

#[derive(Parser, Debug)]
#[command(name = "tw_stock_compute", about = "台股後復權計算與 K 線聚合")]
struct Args {
    #[arg(long, env = "DATABASE_URL")]
    database_url: String,
    #[arg(long, default_value = "backfill")]
    mode: String,
    #[arg(long)]
    stocks: Option<String>,
}

#[derive(Debug, Clone)]
struct DailyPrice { date: NaiveDate, open: f64, high: f64, low: f64, close: f64, volume: i64 }

#[derive(Debug, Clone)]
struct AdjEvent { date: NaiveDate, event_type: String, adjustment_factor: f64, volume_factor: f64, detail: Option<String> }

#[derive(Debug, Clone)]
struct FwdDailyPrice { stock_id: String, date: NaiveDate, open: f64, high: f64, low: f64, close: f64, volume: i64 }

#[derive(Serialize)]
struct Summary {
    schema_version: String,
    processed: usize,
    skipped: usize,
    errors: Vec<ErrorEntry>,
    af_patched: usize,
    /// 是否因 Ctrl-C / SIGTERM 中斷（Python 端用來判斷批次是否完整）
    interrupted: bool,
    elapsed_ms: u128,
}

#[derive(Serialize)]
struct ErrorEntry { stock_id: String, reason: String }

// ─────────────────────────────────────────────
// 主程式
// ─────────────────────────────────────────────

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> Result<()> {
    let args = Args::parse();
    let timer = Instant::now();

    let pool = PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(std::time::Duration::from_secs(10))
        .connect(&args.database_url)
        .await
        .with_context(|| format!("無法連線到資料庫：{}", mask_url(&args.database_url)))?;

    // C5: 啟動時確認 DB schema 版本與程式碼一致
    assert_schema_version(&pool).await?;

    let stock_ids = resolve_stock_ids(&pool, &args).await?;
    let trading_dates = load_trading_calendar(&pool).await?;

    let mut processed  = 0usize;
    let mut skipped    = 0usize;
    let mut af_patched = 0usize;
    let mut errors: Vec<ErrorEntry> = Vec::new();
    let mut interrupted = false;

    // C4: signal handler
    // tokio::select! 的 branch 不支援 #[cfg(...)]，用兩個完整的 cfg 版本分開處理。
    // 中斷點在「股票與股票之間」，每個股票都在 transaction 內，不會 half-commit。

    // ── Unix 版（支援 SIGTERM + Ctrl-C）
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut sigterm = signal(SignalKind::terminate()).expect("無法註冊 SIGTERM handler");

        'batch: for stock_id in &stock_ids {
            tokio::select! {
                biased;
                _ = sigterm.recv() => { interrupted = true; break 'batch; }
                _ = tokio::signal::ctrl_c() => { interrupted = true; break 'batch; }
                result = process_stock(&pool, stock_id, &trading_dates, &args.mode) => {
                    match result {
                        Ok(patched) => { processed += 1; af_patched += patched; }
                        Err(e) => { errors.push(ErrorEntry { stock_id: stock_id.clone(), reason: e.to_string() }); skipped += 1; }
                    }
                }
            }
        }
    }

    // ── Windows 版（只有 Ctrl-C）
    #[cfg(not(unix))]
    {
        'batch: for stock_id in &stock_ids {
            tokio::select! {
                biased;
                _ = tokio::signal::ctrl_c() => { interrupted = true; break 'batch; }
                result = process_stock(&pool, stock_id, &trading_dates, &args.mode) => {
                    match result {
                        Ok(patched) => { processed += 1; af_patched += patched; }
                        Err(e) => { errors.push(ErrorEntry { stock_id: stock_id.clone(), reason: e.to_string() }); skipped += 1; }
                    }
                }
            }
        }
    }

    pool.close().await;

    println!("{}", serde_json::to_string(&Summary {
        schema_version: EXPECTED_SCHEMA_VERSION.to_string(),
        processed, skipped, errors, af_patched,
        interrupted,
        elapsed_ms: timer.elapsed().as_millis(),
    })?);

    Ok(())
}

// ─────────────────────────────────────────────
// 輔助函式
// ─────────────────────────────────────────────

fn mask_url(url: &str) -> String {
    if let Some(scheme_end) = url.find("://") {
        let after = &url[scheme_end + 3..];
        if let Some(at) = after.find('@') {
            let creds = &after[..at];
            if let Some(colon) = creds.find(':') {
                return format!("{}://{}:***{}", &url[..scheme_end], &creds[..colon], &after[at..]);
            }
        }
    }
    url.to_string()
}

// ─────────────────────────────────────────────
// C5: schema version assert
// ─────────────────────────────────────────────

/// 確認 schema_metadata.schema_version 與 EXPECTED_SCHEMA_VERSION 一致。
/// 版本不符直接 bail!，防止舊程式碼跑在新 DB schema 上（或反之）。
///
/// schema_metadata 表由 alembic baseline 建立：
///   CREATE TABLE schema_metadata (key TEXT PRIMARY KEY, value TEXT NOT NULL);
///   INSERT INTO schema_metadata VALUES ('schema_version', '2.0');
async fn assert_schema_version(pool: &PgPool) -> Result<()> {
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT value FROM schema_metadata WHERE key = 'schema_version'",
    )
    .fetch_optional(pool)
    .await
    .context("查詢 schema_metadata 失敗（表是否存在？請確認 alembic upgrade head 已執行）")?;

    match row {
        None => anyhow::bail!(
            "schema_metadata 中找不到 schema_version，請確認 alembic upgrade head 已執行"
        ),
        Some((ver,)) if ver != EXPECTED_SCHEMA_VERSION => anyhow::bail!(
            "schema version 不符：DB={ver}, 程式碼期望={EXPECTED_SCHEMA_VERSION}"
        ),
        Some(_) => Ok(()),
    }
}

// ─────────────────────────────────────────────
// C3: DB 查詢實作
// ─────────────────────────────────────────────

/// fwd_adj_valid 是 SMALLINT 0/1（非 BOOLEAN）
async fn resolve_stock_ids(pool: &PgPool, args: &Args) -> Result<Vec<String>> {
    if let Some(s) = &args.stocks {
        return Ok(s.split(',').map(|x| x.trim().to_string()).collect());
    }
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT stock_id FROM stock_sync_status WHERE fwd_adj_valid = 0 ORDER BY stock_id",
    )
    .fetch_all(pool)
    .await
    .context("查詢 stock_sync_status 失敗")?;
    Ok(rows.into_iter().map(|(id,)| id).collect())
}

async fn load_trading_calendar(pool: &PgPool) -> Result<Vec<NaiveDate>> {
    let rows: Vec<(NaiveDate,)> = sqlx::query_as(
        "SELECT date FROM trading_calendar WHERE market = 'TW' ORDER BY date",
    )
    .fetch_all(pool)
    .await
    .context("載入交易日曆失敗")?;
    Ok(rows.into_iter().map(|(d,)| d).collect())
}

/// NUMERIC 欄位用 ::float8 拉出，避免需要 bigdecimal feature
async fn load_raw_prices(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    market: &str,
    stock_id: &str,
) -> Result<Vec<DailyPrice>> {
    let rows: Vec<(NaiveDate, f64, f64, f64, f64, i64)> = sqlx::query_as(
        "SELECT date, open::float8, high::float8, low::float8, close::float8, volume
           FROM price_daily
          WHERE market = $1 AND stock_id = $2
          ORDER BY date",
    )
    .bind(market).bind(stock_id)
    .fetch_all(tx.as_mut())
    .await
    .with_context(|| format!("讀取 price_daily 失敗：{stock_id}"))?;
    Ok(rows.into_iter().map(|(date, open, high, low, close, volume)| DailyPrice { date, open, high, low, close, volume }).collect())
}

/// detail JSONB 讀成 TEXT，Rust 端再 parse
async fn load_adj_events(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    market: &str,
    stock_id: &str,
) -> Result<Vec<AdjEvent>> {
    let rows: Vec<(NaiveDate, String, f64, f64, Option<String>)> = sqlx::query_as(
        "SELECT date, event_type, adjustment_factor::float8, volume_factor::float8, detail::text
           FROM price_adjustment_events
          WHERE market = $1 AND stock_id = $2
          ORDER BY date",
    )
    .bind(market).bind(stock_id)
    .fetch_all(tx.as_mut())
    .await
    .with_context(|| format!("讀取 price_adjustment_events 失敗：{stock_id}"))?;
    Ok(rows.into_iter().map(|(date, event_type, af, vf, detail)| AdjEvent {
        date, event_type, adjustment_factor: af, volume_factor: vf, detail
    }).collect())
}

async fn patch_capital_increase_af(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    market: &str,
    stock_id: &str,
    raw_prices: &[DailyPrice],
    events: &mut Vec<AdjEvent>,
) -> Result<usize> {
    let mut patched = 0usize;
    for event in events.iter_mut() {
        if event.event_type != "capital_increase" { continue; }
        let detail_str = match &event.detail { Some(s) => s.clone(), None => continue };
        let Some((af, p_pre, after_price)) = compute_capital_increase_af(raw_prices, event.date, &detail_str) else { continue };

        let new_detail: Value = {
            let mut d: Value = serde_json::from_str(&detail_str).unwrap_or(Value::Null);
            if let Some(obj) = d.as_object_mut() {
                obj.remove("status");
                obj.insert("p_pre".into(), p_pre.into());
                obj.insert("after_price".into(), after_price.into());
                obj.insert("af_computed_by".into(), "rust_phase4".into());
            }
            d
        };

        sqlx::query(
            "UPDATE price_adjustment_events
                SET adjustment_factor = $1, detail = $2::jsonb
              WHERE market = $3 AND stock_id = $4 AND date = $5 AND event_type = 'capital_increase'",
        )
        .bind(af)
        .bind(new_detail.to_string())
        .bind(market).bind(stock_id).bind(event.date)
        .execute(tx.as_mut())
        .await
        .with_context(|| format!("UPDATE capital_increase AF 失敗：{stock_id} {}", event.date))?;

        event.adjustment_factor = af;
        patched += 1;
    }
    Ok(patched)
}

async fn upsert_daily_fwd(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    market: &str, stock_id: &str,
    fwd_prices: &[FwdDailyPrice],
) -> Result<()> {
    sqlx::query("DELETE FROM price_daily_fwd WHERE market = $1 AND stock_id = $2")
        .bind(market).bind(stock_id).execute(tx.as_mut()).await.context("DELETE price_daily_fwd 失敗")?;
    if fwd_prices.is_empty() { return Ok(()); }

    let dates:   Vec<NaiveDate> = fwd_prices.iter().map(|p| p.date).collect();
    let opens:   Vec<f64>       = fwd_prices.iter().map(|p| p.open).collect();
    let highs:   Vec<f64>       = fwd_prices.iter().map(|p| p.high).collect();
    let lows:    Vec<f64>       = fwd_prices.iter().map(|p| p.low).collect();
    let closes:  Vec<f64>       = fwd_prices.iter().map(|p| p.close).collect();
    let volumes: Vec<i64>       = fwd_prices.iter().map(|p| p.volume).collect();

    sqlx::query(
        "INSERT INTO price_daily_fwd (market, stock_id, date, open, high, low, close, volume)
         SELECT $1, $2, UNNEST($3::date[]), UNNEST($4::float8[]), UNNEST($5::float8[]),
                UNNEST($6::float8[]), UNNEST($7::float8[]), UNNEST($8::bigint[])",
    )
    .bind(market).bind(stock_id)
    .bind(&dates).bind(&opens).bind(&highs).bind(&lows).bind(&closes).bind(&volumes)
    .execute(tx.as_mut()).await.context("INSERT price_daily_fwd 失敗")?;
    Ok(())
}

async fn upsert_weekly_fwd(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    market: &str, stock_id: &str,
    weeks: &[(i32, u32, f64, f64, f64, f64, i64)],
) -> Result<()> {
    sqlx::query("DELETE FROM price_weekly_fwd WHERE market = $1 AND stock_id = $2")
        .bind(market).bind(stock_id).execute(tx.as_mut()).await.context("DELETE price_weekly_fwd 失敗")?;
    if weeks.is_empty() { return Ok(()); }

    let years:   Vec<i32> = weeks.iter().map(|w| w.0).collect();
    let week_ns: Vec<i32> = weeks.iter().map(|w| w.1 as i32).collect();
    let opens:   Vec<f64> = weeks.iter().map(|w| w.2).collect();
    let highs:   Vec<f64> = weeks.iter().map(|w| w.3).collect();
    let lows:    Vec<f64> = weeks.iter().map(|w| w.4).collect();
    let closes:  Vec<f64> = weeks.iter().map(|w| w.5).collect();
    let volumes: Vec<i64> = weeks.iter().map(|w| w.6).collect();

    sqlx::query(
        "INSERT INTO price_weekly_fwd (market, stock_id, year, week, open, high, low, close, volume)
         SELECT $1, $2, UNNEST($3::int[]), UNNEST($4::int[]), UNNEST($5::float8[]),
                UNNEST($6::float8[]), UNNEST($7::float8[]), UNNEST($8::float8[]), UNNEST($9::bigint[])",
    )
    .bind(market).bind(stock_id)
    .bind(&years).bind(&week_ns).bind(&opens).bind(&highs).bind(&lows).bind(&closes).bind(&volumes)
    .execute(tx.as_mut()).await.context("INSERT price_weekly_fwd 失敗")?;
    Ok(())
}

async fn upsert_monthly_fwd(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    market: &str, stock_id: &str,
    months: &[(i32, u32, f64, f64, f64, f64, i64)],
) -> Result<()> {
    sqlx::query("DELETE FROM price_monthly_fwd WHERE market = $1 AND stock_id = $2")
        .bind(market).bind(stock_id).execute(tx.as_mut()).await.context("DELETE price_monthly_fwd 失敗")?;
    if months.is_empty() { return Ok(()); }

    let years:    Vec<i32> = months.iter().map(|m| m.0).collect();
    let month_ns: Vec<i32> = months.iter().map(|m| m.1 as i32).collect();
    let opens:    Vec<f64> = months.iter().map(|m| m.2).collect();
    let highs:    Vec<f64> = months.iter().map(|m| m.3).collect();
    let lows:     Vec<f64> = months.iter().map(|m| m.4).collect();
    let closes:   Vec<f64> = months.iter().map(|m| m.5).collect();
    let volumes:  Vec<i64> = months.iter().map(|m| m.6).collect();

    sqlx::query(
        "INSERT INTO price_monthly_fwd (market, stock_id, year, month, open, high, low, close, volume)
         SELECT $1, $2, UNNEST($3::int[]), UNNEST($4::int[]), UNNEST($5::float8[]),
                UNNEST($6::float8[]), UNNEST($7::float8[]), UNNEST($8::float8[]), UNNEST($9::bigint[])",
    )
    .bind(market).bind(stock_id)
    .bind(&years).bind(&month_ns).bind(&opens).bind(&highs).bind(&lows).bind(&closes).bind(&volumes)
    .execute(tx.as_mut()).await.context("INSERT price_monthly_fwd 失敗")?;
    Ok(())
}

async fn mark_fwd_valid(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    market: &str, stock_id: &str,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO stock_sync_status (market, stock_id, fwd_adj_valid)
              VALUES ($1, $2, 1)
         ON CONFLICT (market, stock_id) DO UPDATE SET fwd_adj_valid = 1",
    )
    .bind(market).bind(stock_id)
    .execute(tx.as_mut()).await.context("UPSERT stock_sync_status 失敗")?;
    Ok(())
}

/// 處理單一股票的後復權 + 週月 K 聚合。
///
/// ⚠️ 設計決策：永遠全量重算，`_mode` 參數刻意忽略（變數名 `_` 前綴）。
///
/// 後復權 multiplier 從序列尾端倒推（見 `compute_forward_adjusted`），
/// 新增除權息事件會回頭修改整個 fwd 序列的歷史值。partial / incremental
/// 重算會產生「舊日期的 fwd 價被新事件改動但沒重寫入庫」的資料矛盾，
/// 因此這層必須三張 fwd 表 DELETE 後整段重建（見 `upsert_*_fwd`）。
///
/// 若未來要做真正的 incremental 優化，必須在 Python 層偵測「該股票自從上次
/// Phase 4 以後有沒有新除權息事件」，沒有的話跳過呼叫；有的話仍須全量。
/// 不能在 Rust 內 partial。
async fn process_stock(
    pool: &PgPool,
    stock_id: &str,
    trading_dates: &[NaiveDate],
    _mode: &str,
) -> Result<usize> {
    let market = "TW";
    let mut tx = pool.begin().await.context("開啟 transaction 失敗")?;

    let raw_prices = load_raw_prices(&mut tx, market, stock_id).await?;
    if raw_prices.is_empty() {
        tx.rollback().await.ok();
        return Ok(0);
    }

    let mut events = load_adj_events(&mut tx, market, stock_id).await?;
    let af_patched = patch_capital_increase_af(&mut tx, market, stock_id, &raw_prices, &mut events).await?;

    let fwd_prices = compute_forward_adjusted(stock_id, &raw_prices, &events);
    let weeks  = aggregate_weekly(&fwd_prices, trading_dates);
    let months = aggregate_monthly(&fwd_prices);

    upsert_daily_fwd  (&mut tx, market, stock_id, &fwd_prices).await?;
    upsert_weekly_fwd (&mut tx, market, stock_id, &weeks).await?;
    upsert_monthly_fwd(&mut tx, market, stock_id, &months).await?;
    mark_fwd_valid    (&mut tx, market, stock_id).await?;

    tx.commit().await.context("commit 失敗")?;
    Ok(af_patched)
}

// ─────────────────────────────────────────────
// 純計算（不動 DB）
// ─────────────────────────────────────────────

/// 後復權主迴圈。
///
/// 對價格用 `adjustment_factor` 累積成 `price_multiplier`,對成交量用
/// `volume_factor` 累積成 `volume_multiplier` — 拆兩個 multiplier。
///
/// r3.1 修正(av3 揭露 P0-11 production bug):原版用單一 multiplier(從 AF)
/// 同時除價乘量,對純現金 dividend 造成 dollar_vol 守恆但 volume 失真,對
/// split / par_value_change 更是反方向錯誤(volume 應 ×N 卻變 /N)。
///
/// 修正後:
///   * 現金 dividend (vf=1.0): volume 不動 ← 反映實際 share 流動性
///   * split (vf=1/N): volume × N ← post-split equivalent shares,物理正確
///   * stock_dividend: 目前 field_mapper 寫 vf=1.0(P1-17 待修),Rust 暫時
///     當現金 dividend 處理(volume 不動);field_mapper 修完後自動正確
fn compute_forward_adjusted(stock_id: &str, raw_prices: &[DailyPrice], events: &[AdjEvent]) -> Vec<FwdDailyPrice> {
    if raw_prices.is_empty() { return Vec::new(); }
    let mut event_af: HashMap<NaiveDate, f64> = HashMap::new();
    let mut event_vf: HashMap<NaiveDate, f64> = HashMap::new();
    for e in events {
        if (e.adjustment_factor - 1.0).abs() > 1e-12 {
            *event_af.entry(e.date).or_insert(1.0) *= e.adjustment_factor;
        }
        if (e.volume_factor - 1.0).abs() > 1e-12 {
            *event_vf.entry(e.date).or_insert(1.0) *= e.volume_factor;
        }
    }
    let mut price_multiplier  = 1.0_f64;
    let mut volume_multiplier = 1.0_f64;
    let mut result: Vec<FwdDailyPrice> = Vec::with_capacity(raw_prices.len());
    for price in raw_prices.iter().rev() {
        result.push(FwdDailyPrice {
            stock_id: stock_id.to_string(), date: price.date,
            open:   (price.open   * price_multiplier * 100.0).round() / 100.0,
            high:   (price.high   * price_multiplier * 100.0).round() / 100.0,
            low:    (price.low    * price_multiplier * 100.0).round() / 100.0,
            close:  (price.close  * price_multiplier * 100.0).round() / 100.0,
            volume: (price.volume as f64 / volume_multiplier).round() as i64,
        });
        // 先 push 再更新 multiplier:除權息日當日 raw 已是除權息後,不該再乘該日 AF/vf
        if let Some(&af) = event_af.get(&price.date) { price_multiplier  *= af; }
        if let Some(&vf) = event_vf.get(&price.date) { volume_multiplier *= vf; }
    }
    result.reverse();
    result
}

fn aggregate_weekly(fwd_prices: &[FwdDailyPrice], _trading_dates: &[NaiveDate]) -> Vec<(i32, u32, f64, f64, f64, f64, i64)> {
    let mut groups: HashMap<(i32, u32), Vec<&FwdDailyPrice>> = HashMap::new();
    for p in fwd_prices {
        let iso = p.date.iso_week();
        groups.entry((iso.year(), iso.week())).or_default().push(p);
    }
    let mut weeks: Vec<_> = groups.iter().map(|(&(y, w), ps)| {
        let mut s = ps.clone(); s.sort_by_key(|p| p.date);
        let open  = s.first().map(|p| p.open).unwrap_or(0.0);
        let close = s.last().map(|p| p.close).unwrap_or(0.0);
        let high  = s.iter().map(|p| p.high).fold(f64::NEG_INFINITY, f64::max);
        let low   = s.iter().map(|p| p.low).fold(f64::INFINITY, f64::min);
        let vol   = s.iter().map(|p| p.volume).sum();
        (y, w, open, high, low, close, vol)
    }).collect();
    weeks.sort_by(|a, b| (a.0, a.1).cmp(&(b.0, b.1)));
    weeks
}

fn aggregate_monthly(fwd_prices: &[FwdDailyPrice]) -> Vec<(i32, u32, f64, f64, f64, f64, i64)> {
    let mut groups: HashMap<(i32, u32), Vec<&FwdDailyPrice>> = HashMap::new();
    for p in fwd_prices {
        groups.entry((p.date.year(), p.date.month())).or_default().push(p);
    }
    let mut months: Vec<_> = groups.iter().map(|(&(y, m), ps)| {
        let mut s = ps.clone(); s.sort_by_key(|p| p.date);
        let open  = s.first().map(|p| p.open).unwrap_or(0.0);
        let close = s.last().map(|p| p.close).unwrap_or(0.0);
        let high  = s.iter().map(|p| p.high).fold(f64::NEG_INFINITY, f64::max);
        let low   = s.iter().map(|p| p.low).fold(f64::INFINITY, f64::min);
        let vol   = s.iter().map(|p| p.volume).sum();
        (y, m, open, high, low, close, vol)
    }).collect();
    months.sort_by(|a, b| (a.0, a.1).cmp(&(b.0, b.1)));
    months
}

fn compute_capital_increase_af(raw_prices: &[DailyPrice], event_date: NaiveDate, detail_str: &str) -> Option<(f64, f64, f64)> {
    let detail: Value = serde_json::from_str(detail_str).ok()?;
    if detail.get("status").and_then(|v| v.as_str()) != Some("pending_rust_phase4") { return None; }
    let sub_price = detail.get("subscription_price").and_then(|v| v.as_f64())?;
    if sub_price <= 0.0 { return None; }
    let sub_rate = detail.get("subscription_rate_raw").and_then(|v| v.as_f64())?;
    if sub_rate <= 0.0 { return None; }
    let p_pre = raw_prices.iter().filter(|p| p.date < event_date).last()?.close;
    if p_pre <= 0.0 { return None; }
    let r = sub_rate / 1000.0;
    let after_price = (p_pre + sub_price * r) / (1.0 + r);
    let af = if after_price > 0.0 { p_pre / after_price } else { 1.0 };
    Some((af, p_pre, after_price))
}
