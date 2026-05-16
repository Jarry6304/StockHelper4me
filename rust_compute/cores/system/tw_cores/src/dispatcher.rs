// dispatcher.rs — dispatch_indicator / dispatch_structural / dispatch_neely
// (從 main.rs v3.5 R4 C8 抽出)
//
// **不抽 generic dispatcher<C, W>** 對齊 cores_overview.md §十四「禁止抽象」+
// PR-9b 留言「ErasedCore trait wrapper(V3 才考慮)」。三個 dispatch 保留各自
// 實作,讀者可一眼追到 compute → produce_facts → write 三步走的流程。

use fact_schema::{params_hash, IndicatorCore, Timeframe};
use sqlx::postgres::PgPool;
use std::time::Instant;

use crate::helpers::extract_indicator_meta;
use crate::summary::CoreRunSummary;
use crate::writers::{write_facts, write_indicator_value, write_structural_snapshot};

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

            let mut iv_written = 0u64;
            let mut fact_written = 0u64;
            if write {
                if !stock_id.is_empty() {
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
