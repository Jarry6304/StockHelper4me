// traditional_core — 傳統派(Frost & Prechter EWP)波浪引擎,與 Neely **完全解耦的獨立 vertical**。
//
// 對齊 Traditional Core v2 SPEC + m3Spec/traditional_rules.md(規則層)。
//
// 解耦不變式:
//   - entry = **純函式** `run(series, config)`,**不 impl** `fact_schema::WaveCore`
//   - **不 dep** `neely_core` / `ohlcv_loader`(自帶 loader 直讀 Silver)
//   - 自有型別(output.rs)、自有 9 級 Degree、forest **不選 primary**、無 confidence/composite_score
//
// 8-stage pipeline(references/engine.md):
//   1 Pivot(ATR×k)→ 2 候選+形態假設 → 3 硬 Validator → 4 形態確認 →
//   5 指引評估 → 6 Fib 投影 → 7 失效條件 → 8 Degree + Forest 組裝

use anyhow::{anyhow, Result};
use std::time::Instant;

pub mod candidates;
pub mod classifier;
pub mod compaction;
pub mod config;
pub mod degree;
pub mod fibonacci;
pub mod guidelines;
pub mod loader;
pub mod mode;
pub mod monowave;
pub mod node;
pub mod output;
pub mod patterns;
pub mod pivot;
pub mod rules;
pub mod scenario;
pub mod triggers;
pub mod validator;

pub use config::TraditionalEngineConfig;
pub use output::{
    TimeRange, TradBar, TradOhlcvSeries, TraditionalCoreOutput, TraditionalDiagnostics,
    TraditionalScenario,
};

/// 純函式入口(不 impl WaveCore)。**v3 由下而上多度數 fractal 引擎**:
/// monowave(degree-0)→ compaction(逐度數,子浪細分 = 建構約束)→ scenario 組裝 → forest cap。
pub fn run(series: &TradOhlcvSeries, config: &TraditionalEngineConfig) -> Result<TraditionalCoreOutput> {
    use crate::output::{Direction, Pivot, PivotKind};

    let started = Instant::now();
    let bars = &series.bars;
    if bars.is_empty() {
        return Err(anyhow!("traditional_core::run: empty OHLCV series for {}", series.stock_id));
    }
    let data_range = TimeRange {
        start: bars[0].date,
        end: bars[bars.len() - 1].date,
    };

    // Stage 1 — degree-0 monowaves(close-based,不 ATR 過濾)
    let monowaves = monowave::detect_monowaves(bars, config.monowave_epsilon);

    // pivot_series(dashboard skeleton):monowave 端點
    let mut pivot_series: Vec<Pivot> = Vec::new();
    for (i, mw) in monowaves.iter().enumerate() {
        let up = matches!(mw.direction, Direction::Up);
        if i == 0 {
            pivot_series.push(Pivot {
                bar_index: mw.start_bar,
                date: mw.start_date,
                price: mw.start_price,
                kind: if up { PivotKind::Low } else { PivotKind::High },
            });
        }
        pivot_series.push(Pivot {
            bar_index: mw.end_bar,
            date: mw.end_date,
            price: mw.end_price,
            kind: if up { PivotKind::High } else { PivotKind::Low },
        });
    }

    // 資料不足(少於一個 3-leg 修正窗所需 monowave)
    if monowaves.len() < 3 {
        return Ok(TraditionalCoreOutput {
            stock_id: series.stock_id.clone(),
            timeframe: series.timeframe,
            data_range,
            pivot_series,
            scenario_forest: Vec::new(),
            diagnostics: TraditionalDiagnostics {
                pivot_count: monowaves.len(),
                candidate_count: 0,
                validator_pass_count: 0,
                validator_reject_count: 0,
                rejections: Vec::new(),
                forest_overflow_triggered: false,
                insufficient_data: true,
                elapsed_ms: started.elapsed().as_millis() as u64,
            },
        });
    }

    // Stage 2-4 — 由下而上 compaction(子浪細分 = 建構約束)→ top-level pattern 節點
    let top_nodes = compaction::compact(monowaves.clone(), config);
    let candidate_count = top_nodes.len();

    // Stage 5-8 — 各 top 節點組裝 TraditionalScenario(guidelines/fib/triggers/degree)
    let scenarios = scenario::assemble(&top_nodes, config);

    // Forest 組裝(不選 primary;超 max 走 beam fallback)
    let (forest, overflow) = degree::finalize_forest(scenarios, config.forest_max_size);

    Ok(TraditionalCoreOutput {
        stock_id: series.stock_id.clone(),
        timeframe: series.timeframe,
        data_range,
        pivot_series,
        scenario_forest: forest,
        diagnostics: TraditionalDiagnostics {
            pivot_count: monowaves.len(),
            candidate_count,
            validator_pass_count: candidate_count,
            validator_reject_count: 0,
            rejections: Vec::new(),
            forest_overflow_triggered: overflow,
            insufficient_data: false,
            elapsed_ms: started.elapsed().as_millis() as u64,
        },
    })
}

// ---------------------------------------------------------------------------
// Core 註冊(metadata only — inventory,非 trait dispatch)
// ---------------------------------------------------------------------------

/// 單元結構,供 tw_cores `list-cores` 的 dead-code-prevention `let _ = TraditionalCore::new()` 用。
/// **不 impl WaveCore**(對齊 SPEC「不走 trait dispatch」)。
pub struct TraditionalCore;

impl TraditionalCore {
    pub fn new() -> Self {
        TraditionalCore
    }
}

impl Default for TraditionalCore {
    fn default() -> Self {
        Self::new()
    }
}

inventory::submit! {
    core_registry::CoreRegistration::new(
        "traditional_core",
        "0.1.0",
        core_registry::CoreKind::Wave,
        "P3",
        "Traditional Core(Frost & Prechter EWP)獨立 vertical — 純函式 run(),不 impl WaveCore",
    )
}

// ---------------------------------------------------------------------------
// Integration tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::{TradBar, TraditionalPatternType};
    use chrono::{Duration, NaiveDate};
    use fact_schema::Timeframe;

    /// 從 start→end 線性鋪 n 根 bar(monotone,bar range 0.2)。date 連續遞增。
    fn ramp(start: f64, end: f64, n: usize, day: &mut i64, out: &mut Vec<TradBar>) {
        for i in 0..n {
            let t = (i + 1) as f64 / n as f64;
            let v = start + (end - start) * t;
            *day += 1;
            out.push(TradBar {
                date: NaiveDate::from_ymd_opt(2020, 1, 1).unwrap() + Duration::days(*day),
                open: v,
                high: v + 0.1,
                low: v - 0.1,
                close: v,
                volume: Some(1000),
            });
        }
    }

    fn clean_impulse_series() -> TradOhlcvSeries {
        let mut bars = Vec::new();
        let mut day = 0i64;
        ramp(10.0, 15.0, 6, &mut day, &mut bars); // W1 +5
        ramp(15.0, 12.0, 5, &mut day, &mut bars); // W2 -3
        ramp(12.0, 22.0, 8, &mut day, &mut bars); // W3 +10
        ramp(22.0, 18.0, 5, &mut day, &mut bars); // W4 -4(不重疊 W1 頂 15)
        ramp(18.0, 28.0, 8, &mut day, &mut bars); // W5 +10
        ramp(28.0, 24.0, 5, &mut day, &mut bars); // tail(確認 28 為 High pivot)
        TradOhlcvSeries {
            stock_id: "TEST".into(),
            timeframe: Timeframe::Daily,
            bars,
        }
    }

    #[test]
    fn run_produces_forest_with_impulse_and_no_primary_concept() {
        let cfg = TraditionalEngineConfig {
            atr_period: 4,
            swing_atr_multiplier: 1.0,
            ..Default::default()
        };
        let out = run(&clean_impulse_series(), &cfg).unwrap();
        assert!(!out.diagnostics.insufficient_data);
        assert!(out.diagnostics.pivot_count >= 6, "pivots={}", out.diagnostics.pivot_count);
        assert!(!out.scenario_forest.is_empty(), "forest should be non-empty");
        assert!(
            out.scenario_forest
                .iter()
                .any(|s| matches!(s.pattern_type, TraditionalPatternType::Impulse)),
            "should classify at least one Impulse"
        );
        // preference_score == guidelines + qualifiers(無主觀汙染)
        for s in &out.scenario_forest {
            assert_eq!(s.preference_score, s.guidelines_satisfied.len() + s.qualifiers_met.len());
        }
        // forest 依 preference_score 降序(UI 偏好;非 primary 標記)
        for w in out.scenario_forest.windows(2) {
            assert!(w[0].preference_score >= w[1].preference_score);
        }
    }

    #[test]
    fn insufficient_data_on_flat_series() {
        let mut bars = Vec::new();
        let mut day = 0i64;
        ramp(10.0, 10.0, 3, &mut day, &mut bars); // flat → 無 pivot
        let series = TradOhlcvSeries {
            stock_id: "FLAT".into(),
            timeframe: Timeframe::Daily,
            bars,
        };
        let out = run(&series, &TraditionalEngineConfig::default()).unwrap();
        assert!(out.diagnostics.insufficient_data);
        assert!(out.scenario_forest.is_empty());
    }

    #[test]
    fn forest_overflow_caps_at_max() {
        // 用極小 forest_max_size 強制 overflow(只要候選 > max)
        let cfg = TraditionalEngineConfig {
            atr_period: 4,
            swing_atr_multiplier: 1.0,
            forest_max_size: 1,
            ..Default::default()
        };
        let out = run(&clean_impulse_series(), &cfg).unwrap();
        if out.scenario_forest.len() == 1 && out.diagnostics.validator_pass_count > 1 {
            assert!(out.diagnostics.forest_overflow_triggered);
        }
        assert!(out.scenario_forest.len() <= 1);
    }
}
