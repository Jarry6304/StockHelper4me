//! tw_stock_compute — Phase 4 後復權計算與 K 線聚合
//!
//! CLI 使用方式：
//!   tw_stock_compute --db <path> --mode <backfill|incremental> [--stocks <id1,id2,...>]
//!
//! 執行步驟：
//!   1.   讀取 price_daily + price_adjustment_events
//!   1.5. 補算 capital_increase 事件的 adjustment_factor
//!   2.   計算 price_daily_fwd（後復權 OHLCV）
//!   3.   聚合 price_weekly_fwd（依 trading_calendar 的 ISO week）
//!   4.   聚合 price_monthly_fwd（依 year-month）
//!   5.   更新 stock_sync_status.fwd_adj_valid = 1
//!   6.   stdout 輸出 JSON 摘要

use std::collections::HashMap;
use std::time::Instant;

use anyhow::{Context, Result};
use chrono::{Datelike, IsoWeek, NaiveDate};
use clap::Parser;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::Value;

// ─────────────────────────────────────────────
// CLI 參數定義
// ─────────────────────────────────────────────

#[derive(Parser, Debug)]
#[command(
    name = "tw_stock_compute",
    about = "台股後復權計算與 K 線聚合（Phase 4）"
)]
struct Args {
    /// SQLite 資料庫路徑
    #[arg(long)]
    db: String,

    /// 執行模式：backfill（全量）或 incremental（增量）
    #[arg(long, default_value = "backfill")]
    mode: String,

    /// 指定股票代碼（逗號分隔）；省略則處理所有待計算股票
    #[arg(long)]
    stocks: Option<String>,
}

// ─────────────────────────────────────────────
// 資料結構
// ─────────────────────────────────────────────

/// 日K 原始價格
#[derive(Debug, Clone)]
struct DailyPrice {
    date: NaiveDate,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    volume: i64,
}

/// 價格調整事件
#[derive(Debug, Clone)]
struct AdjEvent {
    date: NaiveDate,
    event_type: String,
    adjustment_factor: f64,
    /// detail JSON（用於補算 capital_increase AF）
    detail: Option<String>,
}

/// 後復權日K
#[derive(Debug, Clone)]
struct FwdDailyPrice {
    stock_id: String,
    date: NaiveDate,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    volume: i64,
}

/// stdout 輸出摘要
#[derive(Serialize)]
struct Summary {
    schema_version: String,
    processed: usize,
    skipped: usize,
    errors: Vec<ErrorEntry>,
    af_patched: usize,
    elapsed_ms: u128,
}

/// 錯誤記錄
#[derive(Serialize)]
struct ErrorEntry {
    stock_id: String,
    reason: String,
}

// ─────────────────────────────────────────────
// 主程式
// ─────────────────────────────────────────────

fn main() -> Result<()> {
    let args = Args::parse();
    let timer = Instant::now();

    // 開啟 SQLite 連線，啟用 WAL 模式
    let conn = Connection::open(&args.db)
        .with_context(|| format!("無法開啟資料庫：{}", args.db))?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;")?;

    // 取得要處理的股票清單
    let stock_ids = resolve_stock_ids(&conn, &args)?;

    let mut processed  = 0usize;
    let mut skipped    = 0usize;
    let mut af_patched = 0usize;
    let mut errors: Vec<ErrorEntry> = Vec::new();

    // 取得交易日曆（用於週K/月K聚合）
    let trading_dates = load_trading_calendar(&conn)?;

    for stock_id in &stock_ids {
        match process_stock(&conn, stock_id, &trading_dates, &args.mode) {
            Ok(patched) => {
                processed += 1;
                af_patched += patched;
            }
            Err(e) => {
                // 單股失敗不中斷整體流程
                errors.push(ErrorEntry {
                    stock_id: stock_id.clone(),
                    reason: e.to_string(),
                });
                skipped += 1;
            }
        }
    }

    let elapsed_ms = timer.elapsed().as_millis();

    // 輸出 JSON 摘要至 stdout（Python 端解析）
    let summary = Summary {
        schema_version: "1.1".to_string(),
        processed,
        skipped,
        errors,
        af_patched,
        elapsed_ms,
    };
    println!("{}", serde_json::to_string(&summary)?);

    Ok(())
}

// ─────────────────────────────────────────────
// 股票清單解析
// ─────────────────────────────────────────────

/// 解析要處理的股票清單。
/// - `--stocks` 有指定 → 使用指定清單
/// - 未指定 → 從 stock_sync_status 取 fwd_adj_valid = 0 的股票
fn resolve_stock_ids(conn: &Connection, args: &Args) -> Result<Vec<String>> {
    if let Some(stocks_str) = &args.stocks {
        return Ok(stocks_str.split(',').map(|s| s.trim().to_string()).collect());
    }

    // 查詢待計算的股票
    let mut stmt = conn.prepare(
        "SELECT stock_id FROM stock_sync_status WHERE fwd_adj_valid = 0 ORDER BY stock_id",
    )?;
    let ids: Vec<String> = stmt
        .query_map([], |row| row.get(0))?
        .filter_map(|r| r.ok())
        .collect();

    Ok(ids)
}

// ─────────────────────────────────────────────
// 交易日曆
// ─────────────────────────────────────────────

/// 載入交易日曆，回傳所有交易日的 NaiveDate 集合
fn load_trading_calendar(conn: &Connection) -> Result<Vec<NaiveDate>> {
    let mut stmt = conn.prepare(
        "SELECT date FROM trading_calendar WHERE market = 'TW' ORDER BY date",
    )?;
    let dates: Vec<NaiveDate> = stmt
        .query_map([], |row| row.get::<_, String>(0))?
        .filter_map(|r| r.ok())
        .filter_map(|s| NaiveDate::parse_from_str(&s, "%Y-%m-%d").ok())
        .collect();
    Ok(dates)
}

// ─────────────────────────────────────────────
// 單股處理
// ─────────────────────────────────────────────

/// 處理單一股票的完整 Phase 4 流程。
/// 回傳補算的 AF 筆數。
fn process_stock(
    conn: &Connection,
    stock_id: &str,
    trading_dates: &[NaiveDate],
    _mode: &str,
) -> Result<usize> {
    // 讀取原始日K
    let raw_prices = load_raw_prices(conn, stock_id)?;
    if raw_prices.is_empty() {
        return Ok(0);
    }

    // 讀取調整事件
    let mut events = load_adj_events(conn, stock_id)?;

    // Step 1.5：補算 capital_increase AF
    let patched = patch_capital_increase_af(conn, stock_id, &mut events, &raw_prices)?;

    // Step 2：計算後復權日K
    let fwd_prices = compute_forward_adjusted(stock_id, &raw_prices, &events);

    // Step 3 & 4：聚合週K / 月K
    let weekly  = aggregate_weekly(&fwd_prices, trading_dates);
    let monthly = aggregate_monthly(&fwd_prices);

    // 寫入 DB（使用 transaction 確保原子性）
    conn.execute_batch("BEGIN;")?;

    // 清除舊的後復權資料
    conn.execute(
        "DELETE FROM price_daily_fwd WHERE market = 'TW' AND stock_id = ?",
        params![stock_id],
    )?;
    conn.execute(
        "DELETE FROM price_weekly_fwd WHERE market = 'TW' AND stock_id = ?",
        params![stock_id],
    )?;
    conn.execute(
        "DELETE FROM price_monthly_fwd WHERE market = 'TW' AND stock_id = ?",
        params![stock_id],
    )?;

    // 寫入後復權日K
    {
        let mut stmt = conn.prepare(
            "INSERT OR REPLACE INTO price_daily_fwd
             (market, stock_id, date, open, high, low, close, volume)
             VALUES ('TW', ?, ?, ?, ?, ?, ?, ?)",
        )?;
        for p in &fwd_prices {
            stmt.execute(params![
                p.stock_id,
                p.date.to_string(),
                p.open, p.high, p.low, p.close, p.volume
            ])?;
        }
    }

    // 寫入週K
    {
        let mut stmt = conn.prepare(
            "INSERT OR REPLACE INTO price_weekly_fwd
             (market, stock_id, year, week, open, high, low, close, volume)
             VALUES ('TW', ?, ?, ?, ?, ?, ?, ?, ?)",
        )?;
        for (year, week, o, h, l, c, v) in &weekly {
            stmt.execute(params![stock_id, year, week, o, h, l, c, v])?;
        }
    }

    // 寫入月K
    {
        let mut stmt = conn.prepare(
            "INSERT OR REPLACE INTO price_monthly_fwd
             (market, stock_id, year, month, open, high, low, close, volume)
             VALUES ('TW', ?, ?, ?, ?, ?, ?, ?, ?)",
        )?;
        for (year, month, o, h, l, c, v) in &monthly {
            stmt.execute(params![stock_id, year, month, o, h, l, c, v])?;
        }
    }

    // Step 5：標記後復權已完成
    conn.execute(
        "INSERT INTO stock_sync_status (market, stock_id, fwd_adj_valid)
         VALUES ('TW', ?, 1)
         ON CONFLICT(market, stock_id) DO UPDATE SET fwd_adj_valid = 1",
        params![stock_id],
    )?;

    conn.execute_batch("COMMIT;")?;

    Ok(patched)
}

// ─────────────────────────────────────────────
// 資料載入
// ─────────────────────────────────────────────

/// 從 price_daily 載入原始日K資料（按日期升序）
fn load_raw_prices(conn: &Connection, stock_id: &str) -> Result<Vec<DailyPrice>> {
    let mut stmt = conn.prepare(
        "SELECT date, open, high, low, close, volume
         FROM price_daily
         WHERE market = 'TW' AND stock_id = ?
         ORDER BY date ASC",
    )?;

    let prices: Vec<DailyPrice> = stmt
        .query_map(params![stock_id], |row| {
            let date_str: String = row.get(0)?;
            Ok((date_str, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?))
        })?
        .filter_map(|r| r.ok())
        .filter_map(|(d, o, h, l, c, v)| {
            NaiveDate::parse_from_str(&d, "%Y-%m-%d")
                .ok()
                .map(|date| DailyPrice { date, open: o, high: h, low: l, close: c, volume: v })
        })
        .collect();

    Ok(prices)
}

/// 從 price_adjustment_events 載入調整事件（按日期升序）
fn load_adj_events(conn: &Connection, stock_id: &str) -> Result<Vec<AdjEvent>> {
    let mut stmt = conn.prepare(
        "SELECT date, event_type, adjustment_factor, detail
         FROM price_adjustment_events
         WHERE market = 'TW' AND stock_id = ?
         ORDER BY date ASC",
    )?;

    let events: Vec<AdjEvent> = stmt
        .query_map(params![stock_id], |row| {
            let date_str: String = row.get(0)?;
            Ok((
                date_str,
                row.get::<_, String>(1)?,
                row.get::<_, f64>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        })?
        .filter_map(|r| r.ok())
        .filter_map(|(d, et, af, detail)| {
            NaiveDate::parse_from_str(&d, "%Y-%m-%d")
                .ok()
                .map(|date| AdjEvent { date, event_type: et, adjustment_factor: af, detail })
        })
        .collect();

    Ok(events)
}

// ─────────────────────────────────────────────
// Step 1.5：補算 capital_increase AF
// ─────────────────────────────────────────────

/// 補算 capital_increase 事件的 adjustment_factor。
///
/// 公式（Subscription Price Method）：
///   after_price  = (P_pre * S_old + sub_price * S_new) / (S_old + S_new)
///   AF           = P_pre / after_price
///
/// 其中：
///   P_pre      = 除權前一交易日收盤價
///   sub_price  = 現增認購價
///   sub_rate   = 認購比率（每 1 股有權認購 sub_rate 股）
///   S_old = 1, S_new = sub_rate（以舊股比率計算）
fn patch_capital_increase_af(
    conn: &Connection,
    stock_id: &str,
    events: &mut Vec<AdjEvent>,
    raw_prices: &[DailyPrice],
) -> Result<usize> {
    // 建立日期 → 收盤價的快速查詢表
    let price_map: HashMap<NaiveDate, f64> = raw_prices
        .iter()
        .map(|p| (p.date, p.close))
        .collect();

    let mut patched = 0usize;

    for event in events.iter_mut() {
        // 只處理 capital_increase 且 AF = 1.0（暫用佔位符）的事件
        if event.event_type != "capital_increase" || (event.adjustment_factor - 1.0).abs() > 1e-9 {
            continue;
        }

        let detail_str = match &event.detail {
            Some(d) => d.clone(),
            None => continue,
        };

        // 解析 detail JSON
        let detail: Value = match serde_json::from_str(&detail_str) {
            Ok(v) => v,
            Err(_) => continue,
        };

        // 確認是 pending_rust_phase4 狀態
        if detail.get("status").and_then(|v| v.as_str()) != Some("pending_rust_phase4") {
            continue;
        }

        let sub_price = match detail.get("subscription_price").and_then(|v| v.as_f64()) {
            Some(p) if p > 0.0 => p,
            _ => continue,
        };

        let sub_rate = match detail.get("subscription_rate_raw").and_then(|v| v.as_f64()) {
            Some(r) if r > 0.0 => r,
            _ => continue,
        };

        // 找除權日前一交易日的收盤價
        let ex_date = event.date;
        let p_pre = raw_prices
            .iter()
            .filter(|p| p.date < ex_date)
            .last()
            .map(|p| p.close);

        let p_pre = match p_pre {
            Some(p) if p > 0.0 => p,
            _ => {
                // price_daily 中無前一日資料，跳過
                continue;
            }
        };

        // 計算 after_price 與 AF
        // sub_rate 單位：每 1 股舊股可認購 sub_rate 股新股
        // 將 sub_rate 從 % 轉為比率（FinMind 的 CashIncreaseSubscriptionRate 單位為張/千股，需確認）
        let normalized_rate = sub_rate / 1000.0;   // 暫以千股為單位換算
        let after_price = (p_pre * 1.0 + sub_price * normalized_rate) / (1.0 + normalized_rate);
        let af = if after_price > 0.0 { p_pre / after_price } else { 1.0 };

        // 更新 price_adjustment_events
        conn.execute(
            "UPDATE price_adjustment_events
             SET adjustment_factor = ?, before_price = ?, after_price = ?
             WHERE market = 'TW' AND stock_id = ? AND date = ? AND event_type = 'capital_increase'",
            params![af, p_pre, after_price, stock_id, ex_date.to_string()],
        )?;

        // 同步更新記憶體中的 event
        event.adjustment_factor = af;
        patched += 1;

        let _ = price_map; // 避免 dead_code 警告
    }

    Ok(patched)
}

// ─────────────────────────────────────────────
// Step 2：後復權計算
// ─────────────────────────────────────────────

/// 計算後復權（Forward Adjusted）日K。
///
/// 後復權算法：
///   從最早日期往最新日期計算累積 AF。
///   遇到調整事件時，該日之前所有價格乘以 AF。
///
/// 實作採「全段重算」策略：
///   從最新到最舊反向遍歷，維護一個累積 multiplier。
///   每遇到一個調整事件，multiplier *= AF。
fn compute_forward_adjusted(
    stock_id: &str,
    raw_prices: &[DailyPrice],
    events: &[AdjEvent],
) -> Vec<FwdDailyPrice> {
    if raw_prices.is_empty() {
        return Vec::new();
    }

    // 建立日期 → 事件 AF 的快速查詢（一天可能有多個事件，相乘）
    let mut event_af: HashMap<NaiveDate, f64> = HashMap::new();
    for event in events {
        let af = event.adjustment_factor;
        if (af - 1.0).abs() > 1e-12 {
            *event_af.entry(event.date).or_insert(1.0) *= af;
        }
    }

    // 從最新到最舊反向計算，維護累積 multiplier
    let mut multiplier = 1.0_f64;
    let mut result: Vec<FwdDailyPrice> = Vec::with_capacity(raw_prices.len());

    for price in raw_prices.iter().rev() {
        // 先用「當前」multiplier 計算當日 fwd。
        // 除息日當日 raw 已是除息後價，不需再乘該日 AF（會重複計算）。
        result.push(FwdDailyPrice {
            stock_id: stock_id.to_string(),
            date:     price.date,
            open:     (price.open  * multiplier * 100.0).round() / 100.0,
            high:     (price.high  * multiplier * 100.0).round() / 100.0,
            low:      (price.low   * multiplier * 100.0).round() / 100.0,
            close:    (price.close * multiplier * 100.0).round() / 100.0,
            volume:   (price.volume as f64 / multiplier).round() as i64,
        });

        // 套用完當日後才更新 multiplier，影響「此日之前」的資料
        if let Some(&af) = event_af.get(&price.date) {
            multiplier *= af;
        }
    }

    // 反轉回升序
    result.reverse();
    result
}

// ─────────────────────────────────────────────
// Step 3：週K 聚合
// ─────────────────────────────────────────────

/// 聚合後復權週K（以 ISO week 分組）。
/// 回傳 (year, week, open, high, low, close, volume) 元組列表。
fn aggregate_weekly(
    fwd_prices: &[FwdDailyPrice],
    _trading_dates: &[NaiveDate],
) -> Vec<(i32, u32, f64, f64, f64, f64, i64)> {
    if fwd_prices.is_empty() {
        return Vec::new();
    }

    // 以 (year, week) 分組
    let mut groups: HashMap<(i32, u32), Vec<&FwdDailyPrice>> = HashMap::new();
    for price in fwd_prices {
        let iso = price.date.iso_week();
        let key = (iso.year(), iso.week());
        groups.entry(key).or_default().push(price);
    }

    // 計算每週的 OHLCV
    let mut weeks: Vec<(i32, u32, f64, f64, f64, f64, i64)> = groups
        .iter()
        .map(|(&(year, week), prices)| {
            // 確保按日期排序
            let mut sorted = prices.clone();
            sorted.sort_by_key(|p| p.date);

            let open   = sorted.first().map(|p| p.open).unwrap_or(0.0);
            let close  = sorted.last().map(|p| p.close).unwrap_or(0.0);
            let high   = sorted.iter().map(|p| p.high).fold(f64::NEG_INFINITY, f64::max);
            let low    = sorted.iter().map(|p| p.low).fold(f64::INFINITY, f64::min);
            let volume = sorted.iter().map(|p| p.volume).sum();

            (year, week, open, high, low, close, volume)
        })
        .collect();

    // 按年份 + 週次排序
    weeks.sort_by(|a, b| (a.0, a.1).cmp(&(b.0, b.1)));
    weeks
}

// ─────────────────────────────────────────────
// Step 4：月K 聚合
// ─────────────────────────────────────────────

/// 聚合後復權月K（以 year-month 分組）。
/// 回傳 (year, month, open, high, low, close, volume) 元組列表。
fn aggregate_monthly(
    fwd_prices: &[FwdDailyPrice],
) -> Vec<(i32, u32, f64, f64, f64, f64, i64)> {
    if fwd_prices.is_empty() {
        return Vec::new();
    }

    // 以 (year, month) 分組
    let mut groups: HashMap<(i32, u32), Vec<&FwdDailyPrice>> = HashMap::new();
    for price in fwd_prices {
        let key = (price.date.year(), price.date.month());
        groups.entry(key).or_default().push(price);
    }

    // 計算每月的 OHLCV
    let mut months: Vec<(i32, u32, f64, f64, f64, f64, i64)> = groups
        .iter()
        .map(|(&(year, month), prices)| {
            let mut sorted = prices.clone();
            sorted.sort_by_key(|p| p.date);

            let open   = sorted.first().map(|p| p.open).unwrap_or(0.0);
            let close  = sorted.last().map(|p| p.close).unwrap_or(0.0);
            let high   = sorted.iter().map(|p| p.high).fold(f64::NEG_INFINITY, f64::max);
            let low    = sorted.iter().map(|p| p.low).fold(f64::INFINITY, f64::min);
            let volume = sorted.iter().map(|p| p.volume).sum();

            (year, month, open, high, low, close, volume)
        })
        .collect();

    months.sort_by(|a, b| (a.0, a.1).cmp(&(b.0, b.1)));
    months
}
