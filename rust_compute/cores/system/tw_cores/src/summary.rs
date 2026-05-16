// summary.rs — CoreRunSummary struct + print_summary(從 main.rs v3.5 R4 C8 抽出)

use serde::Serialize;
use std::time::Instant;

#[derive(Debug, Clone, Serialize)]
pub struct CoreRunSummary {
    pub core: String,
    pub stock_id: String,
    pub status: String,
    pub events: u64,
    pub iv_written: u64,
    pub fact_written: u64,
    pub elapsed_ms: u64,
    pub error: Option<String>,
}

impl CoreRunSummary {
    pub fn err(core: &str, stock_id: &str, msg: String, start: Instant) -> Self {
        Self {
            core: core.to_string(),
            stock_id: stock_id.to_string(),
            status: "err".to_string(),
            events: 0,
            iv_written: 0,
            fact_written: 0,
            elapsed_ms: start.elapsed().as_millis() as u64,
            error: Some(msg),
        }
    }
}

pub fn loader_err_summary(
    core: &str,
    stock_id: &str,
    op: &str,
    e: &anyhow::Error,
) -> CoreRunSummary {
    CoreRunSummary {
        core: core.to_string(),
        stock_id: stock_id.to_string(),
        status: "loader_err".to_string(),
        events: 0,
        iv_written: 0,
        fact_written: 0,
        elapsed_ms: 0,
        error: Some(format!("{}: {:#}", op, e)),
    }
}

pub fn print_summary(
    summary: &[CoreRunSummary],
    total_elapsed: std::time::Duration,
    write: bool,
) {
    use std::collections::BTreeMap;
    println!();
    println!("== run-all summary ==");
    println!(
        "total elapsed: {:.1}s    write={}    rows={}",
        total_elapsed.as_secs_f64(),
        write,
        summary.len()
    );

    let mut by_core: BTreeMap<&str, (u64, u64, u64, u64, u64, u64)> = BTreeMap::new();
    // (ok_count, err_count, total_events, total_iv_written, total_fact_written, total_elapsed_ms)
    for r in summary {
        let entry = by_core.entry(&r.core).or_insert((0, 0, 0, 0, 0, 0));
        if r.status == "ok" {
            entry.0 += 1;
        } else {
            entry.1 += 1;
        }
        entry.2 += r.events;
        entry.3 += r.iv_written;
        entry.4 += r.fact_written;
        entry.5 += r.elapsed_ms;
    }

    println!();
    println!("{:<28} {:>6} {:>6} {:>9} {:>10} {:>10} {:>10}",
        "core", "ok", "err", "events", "iv_rows", "facts_new", "elapsed_s");
    println!("{}", "-".repeat(86));
    for (core, (ok, err, events, iv, facts, ms)) in &by_core {
        println!(
            "{:<28} {:>6} {:>6} {:>9} {:>10} {:>10} {:>10.1}",
            core, ok, err, events, iv, facts, *ms as f64 / 1000.0
        );
    }
    // v3.4 r2:`facts_new` 為本輪新增(rows_affected from INSERT ... ON CONFLICT
    // DO NOTHING);第二次 run 同 facts → facts_new=0 但 facts 表仍有 row。
    // 查 core 累計 facts → SELECT COUNT(*) FROM facts WHERE source_core=...

    let errs: Vec<&CoreRunSummary> = summary.iter().filter(|r| r.status != "ok").collect();
    if !errs.is_empty() {
        println!();
        println!("== errors(前 20)==");
        for r in errs.iter().take(20) {
            println!(
                "  [{}] {} stock={} — {}",
                r.status,
                r.core,
                if r.stock_id.is_empty() { "-" } else { &r.stock_id },
                r.error.as_deref().unwrap_or("(no message)")
            );
        }
        if errs.len() > 20 {
            println!("  ... 其他 {} 條 error 省略", errs.len() - 20);
        }
    }
}
