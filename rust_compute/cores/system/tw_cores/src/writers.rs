// writers.rs — PG IO helpers(connect_pg / write_indicator_value / write_structural_snapshot
// / write_facts / resolve_stock_list,從 main.rs v3.5 R4 C8 抽出)

use anyhow::{Context, Result};
use chrono::NaiveDate;
use fact_schema::{Fact, Timeframe};
use sqlx::postgres::{PgPool, PgPoolOptions};

pub async fn connect_pg(max_connections: u32) -> Result<PgPool> {
    let database_url = std::env::var("DATABASE_URL")
        .context("DATABASE_URL not set — set environment or .env before running")?;
    let pool = PgPoolOptions::new()
        .max_connections(max_connections)
        .connect(&database_url)
        .await
        .context("failed to connect to PG")?;
    Ok(pool)
}

#[allow(clippy::too_many_arguments)]
pub async fn write_indicator_value(
    pool: &PgPool,
    stock_id: &str,
    value_date: NaiveDate,
    timeframe: &str,
    source_core: &str,
    source_version: &str,
    params_hash_hex: &str,
    value_json: &serde_json::Value,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO indicator_values
            (stock_id, value_date, timeframe, source_core, source_version, params_hash, value)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        ON CONFLICT (stock_id, value_date, timeframe, source_core, params_hash)
        DO UPDATE SET
            source_version = EXCLUDED.source_version,
            value          = EXCLUDED.value,
            created_at     = NOW()
        "#,
    )
    .bind(stock_id)
    .bind(value_date)
    .bind(timeframe)
    .bind(source_core)
    .bind(source_version)
    .bind(params_hash_hex)
    .bind(value_json)
    .execute(pool)
    .await
    .context("insert indicator_values failed")?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn write_structural_snapshot(
    pool: &PgPool,
    stock_id: &str,
    snapshot_date: NaiveDate,
    timeframe: Timeframe,
    core_name: &str,
    source_version: &str,
    params_hash_hex: &str,
    snapshot_json: &serde_json::Value,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO structural_snapshots
            (stock_id, snapshot_date, timeframe, core_name,
             source_version, params_hash, snapshot)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        ON CONFLICT (stock_id, snapshot_date, timeframe, core_name, params_hash)
        DO UPDATE SET
            source_version = EXCLUDED.source_version,
            snapshot       = EXCLUDED.snapshot,
            created_at     = NOW()
        "#,
    )
    .bind(stock_id)
    .bind(snapshot_date)
    .bind(timeframe.as_str())
    .bind(core_name)
    .bind(source_version)
    .bind(params_hash_hex)
    .bind(snapshot_json)
    .execute(pool)
    .await
    .context("insert structural_snapshots failed")?;
    Ok(())
}

/// PR-9c batch INSERT 取代 per-event loop:用 UNNEST array bind 一次插入 N row。
/// per-event INSERT 是 stage 5 主要 bottleneck;batch INSERT 降至 N batches round-trip。
/// BATCH_SIZE 4000 conservative,避免單筆 query 過大。
///
/// **回傳語意**:`Ok(n)` 為**新插入** row 數(`ON CONFLICT DO NOTHING` 跳過 dedup row,
/// 不計入 `rows_affected`)。第二次 run 同 facts → `n=0` 不代表「沒產出 facts」,
/// 而是「facts 已存在 facts 表內」(uq_facts_dedup unique index 保證 idempotent)。
pub async fn write_facts(pool: &PgPool, facts: &[Fact]) -> Result<u64> {
    if facts.is_empty() {
        return Ok(0);
    }
    const BATCH_SIZE: usize = 4000;
    let mut total_inserted = 0u64;
    for chunk in facts.chunks(BATCH_SIZE) {
        let stock_ids:      Vec<&str>             = chunk.iter().map(|f| f.stock_id.as_str()).collect();
        let fact_dates:     Vec<chrono::NaiveDate> = chunk.iter().map(|f| f.fact_date).collect();
        let timeframes:     Vec<&str>             = chunk.iter().map(|f| f.timeframe.as_str()).collect();
        let source_cores:   Vec<&str>             = chunk.iter().map(|f| f.source_core.as_str()).collect();
        let source_versions:Vec<&str>             = chunk.iter().map(|f| f.source_version.as_str()).collect();
        let params_hashes:  Vec<&str>             = chunk.iter().map(|f| f.params_hash.as_deref().unwrap_or("")).collect();
        let statements:     Vec<&str>             = chunk.iter().map(|f| f.statement.as_str()).collect();
        let metadatas:      Vec<&serde_json::Value> = chunk.iter().map(|f| &f.metadata).collect();

        let res = sqlx::query(
            r#"
            INSERT INTO facts
                (stock_id, fact_date, timeframe, source_core,
                 source_version, params_hash, statement, metadata)
            SELECT * FROM UNNEST(
                $1::text[], $2::date[], $3::text[], $4::text[],
                $5::text[], $6::text[], $7::text[], $8::jsonb[]
            )
            ON CONFLICT DO NOTHING
            "#,
        )
        .bind(&stock_ids)
        .bind(&fact_dates)
        .bind(&timeframes)
        .bind(&source_cores)
        .bind(&source_versions)
        .bind(&params_hashes)
        .bind(&statements)
        .bind(&metadatas)
        .execute(pool)
        .await
        .context("batch insert facts failed")?;
        total_inserted += res.rows_affected();
    }
    Ok(total_inserted)
}

pub async fn resolve_stock_list(
    pool: &PgPool,
    stocks: Option<&str>,
    limit: Option<usize>,
    dirty: bool,
) -> Result<Vec<String>> {
    if let Some(s) = stocks {
        let list: Vec<String> = s
            .split(',')
            .map(|x| x.trim().to_string())
            .filter(|x| !x.is_empty())
            .collect();
        if let Some(n) = limit {
            return Ok(list.into_iter().take(n).collect());
        }
        return Ok(list);
    }

    // dirty=true:對齊 silver/orchestrator.py:_fetch_dirty_fwd_stocks
    // 只拉 is_dirty=TRUE 的 stocks(走 PR #20 trigger 維護的 dirty queue)
    let sql = if dirty {
        r#"
        SELECT DISTINCT stock_id
        FROM price_daily_fwd
        WHERE market = 'TW' AND is_dirty = TRUE
        ORDER BY stock_id ASC
        "#
    } else {
        r#"
        SELECT DISTINCT stock_id
        FROM price_daily_fwd
        WHERE market = 'TW'
        ORDER BY stock_id ASC
        "#
    };
    let rows: Vec<(String,)> = sqlx::query_as(sql)
        .fetch_all(pool)
        .await
        .context("query price_daily_fwd distinct stock_id failed")?;
    let mut list: Vec<String> = rows.into_iter().map(|(s,)| s).collect();
    if let Some(n) = limit {
        list.truncate(n);
    }
    Ok(list)
}
