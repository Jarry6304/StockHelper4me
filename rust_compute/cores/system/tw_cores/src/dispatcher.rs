// dispatcher.rs — dispatch_indicator / dispatch_structural / dispatch_neely
// (從 main.rs v3.5 R4 C8 抽出)
//
// **不抽 generic dispatcher<C, W>** 對齊 cores_overview.md §十四「禁止抽象」+
// PR-9b 留言「ErasedCore trait wrapper(V3 才考慮)」。三個 dispatch 保留各自
// 實作,讀者可一眼追到 compute → produce_facts → write 三步走的流程。

use fact_schema::{params_hash, IndicatorCore, Timeframe};
use sqlx::postgres::PgPool;
use std::time::Instant;

use crate::helpers::{extract_indicator_meta, indicator_output_is_empty};
use crate::summary::CoreRunSummary;
use crate::writers::{
    write_facts, write_forecast_log, write_indicator_value, write_structural_snapshot,
    ForecastLogRow,
};

/// dispatch_indicator — 標準走 indicator_values 表的 IndicatorCore dispatch。
pub async fn dispatch_indicator<C>(
    pool: &PgPool,
    core: &C,
    input: &C::Input,
    params: C::Params,
    write: bool,
) -> CoreRunSummary
where
    C: IndicatorCore,
{
    let start = Instant::now();
    let core_name = core.name().to_string();
    let core_version = core.version().to_string();
    let hash = params_hash(&params).unwrap_or_default();

    match core.compute(input, params) {
        Ok(output) => {
            let facts = core.produce_facts(&output);
            let value_json = match serde_json::to_value(&output) {
                Ok(v) => v,
                Err(e) => {
                    return CoreRunSummary::err(
                        &core_name,
                        "",
                        format!("serialize output failed: {}", e),
                        start,
                    );
                }
            };
            let (stock_id, value_date, timeframe_str) = extract_indicator_meta(&value_json);
            // 空序列 output 不寫 indicator_values:value_date 會 fallback 今天,
            // 空 row 反而 shadow 掉真實資料 row(見 helpers::indicator_output_is_empty)。
            let output_empty = indicator_output_is_empty(&value_json);

            let mut iv_written = 0u64;
            let mut fact_written = 0u64;
            if write {
                if !stock_id.is_empty() && !output_empty {
                    match write_indicator_value(
                        pool,
                        &stock_id,
                        value_date,
                        &timeframe_str,
                        &core_name,
                        &core_version,
                        &hash,
                        &value_json,
                    )
                    .await
                    {
                        Ok(()) => iv_written = 1,
                        Err(e) => tracing::warn!(
                            core = %core_name,
                            stock_id,
                            "write_indicator_value failed: {:#}",
                            e
                        ),
                    }
                } else if output_empty {
                    tracing::debug!(
                        core = %core_name,
                        stock_id,
                        "skip empty-series indicator_values write"
                    );
                }
                match write_facts(pool, &facts).await {
                    Ok(n) => fact_written = n,
                    Err(e) => tracing::warn!(core = %core_name, "write_facts failed: {:#}", e),
                }
            }

            CoreRunSummary {
                core: core_name,
                stock_id,
                status: "ok".to_string(),
                events: facts.len() as u64,
                iv_written,
                fact_written,
                elapsed_ms: start.elapsed().as_millis() as u64,
                error: None,
            }
        }
        Err(e) => CoreRunSummary::err(&core_name, "", format!("compute failed: {:#}", e), start),
    }
}

/// dispatch_structural — 走 IndicatorCore trait,但 Output 寫進 structural_snapshots
/// 而非 indicator_values(對齊 m3Spec/indicator_cores_pattern.md §2.4)。
/// 用於 P2 pattern cores(support_resistance / candlestick_pattern / trendline)。
pub async fn dispatch_structural<C>(
    pool: &PgPool,
    core: &C,
    input: &C::Input,
    params: C::Params,
    write: bool,
) -> CoreRunSummary
where
    C: IndicatorCore,
{
    let start = Instant::now();
    let core_name = core.name().to_string();
    let core_version = core.version().to_string();
    let hash = params_hash(&params).unwrap_or_default();

    match core.compute(input, params) {
        Ok(output) => {
            let facts = core.produce_facts(&output);
            let snapshot_json = match serde_json::to_value(&output) {
                Ok(v) => v,
                Err(e) => {
                    return CoreRunSummary::err(
                        &core_name,
                        "",
                        format!("serialize output failed: {}", e),
                        start,
                    );
                }
            };
            let (stock_id, snapshot_date, timeframe_str) = extract_indicator_meta(&snapshot_json);
            let tf = crate::helpers::parse_timeframe(&timeframe_str).unwrap_or(Timeframe::Daily);

            let mut iv_written = 0u64; // 借用欄位記 snapshot 寫 1 row(對齊 dispatch_neely 慣例)
            let mut fact_written = 0u64;
            if write {
                if !stock_id.is_empty() {
                    match write_structural_snapshot(
                        pool,
                        &stock_id,
                        snapshot_date,
                        tf,
                        &core_name,
                        &core_version,
                        &hash,
                        &snapshot_json,
                    )
                    .await
                    {
                        Ok(()) => iv_written = 1,
                        Err(e) => tracing::warn!(
                            core = %core_name,
                            stock_id,
                            "write_structural_snapshot failed: {:#}",
                            e
                        ),
                    }
                }
                match write_facts(pool, &facts).await {
                    Ok(n) => fact_written = n,
                    Err(e) => tracing::warn!(core = %core_name, "write_facts failed: {:#}", e),
                }
            }

            CoreRunSummary {
                core: core_name,
                stock_id,
                status: "ok".to_string(),
                events: facts.len() as u64,
                iv_written,
                fact_written,
                elapsed_ms: start.elapsed().as_millis() as u64,
                error: None,
            }
        }
        Err(e) => CoreRunSummary::err(&core_name, "", format!("compute failed: {:#}", e), start),
    }
}

/// dispatch_forecast — v0.3 interval-forecast spine 第 4 個 dispatch。
///
/// Calls IndicatorCore::compute, serializes the resulting forecasts into
/// forecast_log rows, and batch-writes them.  Does NOT write facts or
/// indicator_values(forecast cores 不寫 facts;forecast_log 是專屬 sink)。
///
/// Used by kalman_forecast_core(initial)+ future forecast cores
/// (log_channel_core, neely_fib emitter, fusion)。
pub async fn dispatch_forecast<C>(
    pool: &PgPool,
    core: &C,
    input: &C::Input,
    params: C::Params,
    write: bool,
    source_core_tag: &str,
    calibrated: bool,
) -> CoreRunSummary
where
    C: IndicatorCore,
{
    let start = Instant::now();
    let core_name = core.name().to_string();
    let hash = params_hash(&params).unwrap_or_default();

    match core.compute(input, params) {
        Ok(output) => {
            let value_json = match serde_json::to_value(&output) {
                Ok(v) => v,
                Err(e) => {
                    return CoreRunSummary::err(
                        &core_name,
                        "",
                        format!("serialize output failed: {}", e),
                        start,
                    );
                }
            };

            // Extract forecast rows from the output's `forecasts` array.
            // Convention: all forecast cores output {stock_id, forecast_date,
            // forecasts: [{horizon_days, confidence, lower, upper, point}]}.
            let stock_id = value_json
                .get("stock_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let forecast_date_str = value_json
                .get("forecast_date")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let forecast_date = match chrono::NaiveDate::parse_from_str(forecast_date_str, "%Y-%m-%d") {
                Ok(d) => d,
                Err(e) => {
                    return CoreRunSummary::err(
                        &core_name,
                        &stock_id,
                        format!("missing/invalid forecast_date in output: {}", e),
                        start,
                    );
                }
            };

            let mut rows: Vec<ForecastLogRow> = Vec::new();
            if let Some(forecasts) = value_json.get("forecasts").and_then(|v| v.as_array()) {
                for f in forecasts {
                    let horizon_days = match f.get("horizon_days").and_then(|v| v.as_i64()) {
                        Some(h) => h as i16,
                        None => continue,
                    };
                    let confidence = match f.get("confidence").and_then(|v| v.as_f64()) {
                        Some(c) => c,
                        None => continue,
                    };
                    let lower = f.get("lower").and_then(|v| v.as_f64());
                    let upper = f.get("upper").and_then(|v| v.as_f64());
                    let point = f.get("point").and_then(|v| v.as_f64());

                    rows.push(ForecastLogRow {
                        stock_id: stock_id.clone(),
                        forecast_date,
                        horizon_days,
                        lower,
                        upper,
                        point,
                        confidence,
                        calibrated,
                        source_core: source_core_tag.to_string(),
                        regime_tag: None,
                        params_hash: Some(hash.clone()),
                    });
                }
            }

            let events = rows.len() as u64;
            let mut fact_written = 0u64;
            if write && !rows.is_empty() {
                match write_forecast_log(pool, &rows).await {
                    Ok(n) => fact_written = n,
                    Err(e) => tracing::warn!(
                        core = %core_name,
                        stock_id,
                        "write_forecast_log failed: {:#}",
                        e
                    ),
                }
            }

            CoreRunSummary {
                core: core_name,
                stock_id,
                status: "ok".to_string(),
                events,
                iv_written: 0,
                fact_written,
                elapsed_ms: start.elapsed().as_millis() as u64,
                error: None,
            }
        }
        Err(e) => CoreRunSummary::err(&core_name, "", format!("compute failed: {:#}", e), start),
    }
}

/// dispatch_neely — Wave Core 特化(Output 是 Scenario Forest 而非 IndicatorOutput)。
pub async fn dispatch_neely(
    pool: &PgPool,
    stock_id: &str,
    series: &neely_core::output::OhlcvSeries,
    params: neely_core::NeelyCoreParams,
    write: bool,
) -> CoreRunSummary {
    use fact_schema::WaveCore;

    let start = Instant::now();
    let core = neely_core::NeelyCore::new();
    let hash = params_hash(&params).unwrap_or_default();
    let tf = params.timeframe;
    let core_version = core.version().to_string();

    match core.compute(series, params) {
        Ok(output) => {
            let facts = core.produce_facts(&output);
            let mut iv_written = 0u64; // neely 不寫 indicator_values
            let mut fact_written = 0u64;
            if write {
                let snapshot_json = match serde_json::to_value(&output) {
                    Ok(v) => v,
                    Err(e) => {
                        return CoreRunSummary::err(
                            "neely_core",
                            stock_id,
                            format!("serialize output failed: {}", e),
                            start,
                        );
                    }
                };
                match write_structural_snapshot(
                    pool,
                    &output.stock_id,
                    output.data_range.end,
                    tf,
                    "neely_core",
                    &core_version,
                    &hash,
                    &snapshot_json,
                )
                .await
                {
                    Ok(()) => iv_written = 1, // 借用欄位記 snapshot 寫 1 row
                    Err(e) => tracing::warn!(
                        core = "neely_core",
                        stock_id,
                        "write_structural_snapshot failed: {:#}",
                        e
                    ),
                }
                match write_facts(pool, &facts).await {
                    Ok(n) => fact_written = n,
                    Err(e) => {
                        tracing::warn!(core = "neely_core", "write_facts failed: {:#}", e)
                    }
                }
            }

            CoreRunSummary {
                core: "neely_core".to_string(),
                stock_id: stock_id.to_string(),
                status: "ok".to_string(),
                events: facts.len() as u64,
                iv_written,
                fact_written,
                elapsed_ms: start.elapsed().as_millis() as u64,
                error: None,
            }
        }
        Err(e) => CoreRunSummary::err(
            "neely_core",
            stock_id,
            format!("compute failed: {:#}", e),
            start,
        ),
    }
}
