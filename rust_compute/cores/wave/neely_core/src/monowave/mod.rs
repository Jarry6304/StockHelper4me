// monowave — Stage 1+2:Monowave Detection + Classification
//
// 對齊 m2Spec/oldm2Spec/neely_core.md §三 / §七 Stage 1-2。
// Pipeline:
//   bars → [Stage 1] detect_monowaves(Pure Close + ATR-filtered)→ Vec<Monowave>
//        → [Stage 2] classify(Rule of Neutrality + Rule of Proportion)
//                     → Vec<ClassifiedMonowave>
//
// 子模組:
//   - pure_close.rs:Stage 1 detector + Wilder ATR
//   - neutrality.rs:Rule of Neutrality(monowave direction 標註)
//   - proportion.rs:Rule of Proportion(magnitude / 45° metrics)

use crate::config::NeelyEngineConfig;
use crate::output::{Monowave, OhlcvBar};

pub mod neutrality;
pub mod proportion;
pub mod pure_close;

pub use proportion::ProportionMetrics;
pub use pure_close::{compute_atr_series, detect_monowaves};

/// Stage 2 classification 結果。
/// 對外不暴露(`output.rs::Monowave` 仍是 raw + Stage 2 修正後的 direction)。
#[derive(Debug, Clone)]
pub struct ClassifiedMonowave {
    /// 已套用 Rule of Neutrality(direction 可能被改 Neutral)
    pub monowave: Monowave,
    /// monowave 起點 bar 對應的 ATR(供 Validator R3 / Proportion 用)
    pub atr_at_start: f64,
    /// Rule of Proportion 算出的 metrics(magnitude / 45° slope 等)
    pub metrics: ProportionMetrics,
}

/// Stage 1+2 統合 entry。
///
/// `bars` / `monowaves` 由 caller 提供;`classify_monowaves` 不重跑 Stage 1。
/// 這樣 caller(`lib.rs::compute`)可分別計時 Stage 1 / Stage 2。
pub fn classify_monowaves(
    bars: &[OhlcvBar],
    monowaves: Vec<Monowave>,
    stock_id: &str,
    cfg: &NeelyEngineConfig,
) -> Vec<ClassifiedMonowave> {
    if bars.is_empty() || monowaves.is_empty() {
        return Vec::new();
    }
    let atrs = compute_atr_series(bars, cfg.atr_period);

    monowaves
        .into_iter()
        .map(|mw| {
            // 找 monowave 起 / 終點對應的 bar index
            let start_idx = find_bar_index(bars, mw.start_date).unwrap_or(0);
            let end_idx = find_bar_index(bars, mw.end_date).unwrap_or(bars.len() - 1);
            let atr_at_start = atrs.get(start_idx).copied().unwrap_or(0.0);
            let duration = end_idx.saturating_sub(start_idx) + 1;

            // Stage 2-A:Rule of Neutrality — 修正 direction
            let new_direction = neutrality::classify_neutrality(
                &mw,
                atr_at_start,
                stock_id,
                cfg.neutral_threshold_taiex,
            );
            let mut classified_mw = mw;
            classified_mw.direction = new_direction;

            // Stage 2-B:Rule of Proportion — 算 metrics
            let metrics = proportion::compute_proportion_metrics(
                &classified_mw,
                atr_at_start,
                duration,
            );

            ClassifiedMonowave {
                monowave: classified_mw,
                atr_at_start,
                metrics,
            }
        })
        .collect()
}

fn find_bar_index(bars: &[OhlcvBar], target_date: chrono::NaiveDate) -> Option<usize> {
    bars.iter().position(|b| b.date == target_date)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::MonowaveDirection;
    use chrono::NaiveDate;

    fn bar(d: &str, o: f64, h: f64, l: f64, c: f64) -> OhlcvBar {
        OhlcvBar {
            date: NaiveDate::parse_from_str(d, "%Y-%m-%d").unwrap(),
            open: o,
            high: h,
            low: l,
            close: c,
            volume: None,
        }
    }

    #[test]
    fn empty_inputs_yield_empty_classification() {
        let cfg = NeelyEngineConfig::default();
        assert!(classify_monowaves(&[], vec![], "2330", &cfg).is_empty());
        let bars = vec![bar("2026-01-01", 100.0, 101.0, 99.0, 100.0)];
        assert!(classify_monowaves(&bars, vec![], "2330", &cfg).is_empty());
    }

    #[test]
    fn end_to_end_zigzag_stock_classifies_three_directional_monowaves() {
        // 7-bar 清晰 zigzag(對齊 pure_close::detect_clean_zigzag_yields_three_monowaves)
        let bars = vec![
            bar("2026-01-01", 10.0, 10.5, 9.5, 10.0),
            bar("2026-01-02", 10.0, 11.5, 10.0, 11.0),
            bar("2026-01-03", 11.0, 13.0, 11.0, 13.0),
            bar("2026-01-04", 13.0, 13.0, 11.5, 11.5),
            bar("2026-01-05", 11.5, 11.5, 9.0, 9.0),
            bar("2026-01-06", 9.0, 11.0, 9.0, 11.0),
            bar("2026-01-07", 11.0, 13.5, 11.0, 13.5),
        ];
        let cfg = NeelyEngineConfig::default();
        let waves = detect_monowaves(&bars, cfg.atr_period);
        let classified = classify_monowaves(&bars, waves, "2330", &cfg);
        assert_eq!(classified.len(), 3);
        // magnitude 都遠 > ATR(~1.5),Rule of Neutrality 不會改 direction
        assert!(matches!(classified[0].monowave.direction, MonowaveDirection::Up));
        assert!(matches!(classified[1].monowave.direction, MonowaveDirection::Down));
        assert!(matches!(classified[2].monowave.direction, MonowaveDirection::Up));
        // metrics 都應 magnitude > 0
        for c in &classified {
            assert!(c.metrics.magnitude > 0.0);
            assert!(c.metrics.duration_bars > 0);
        }
    }

    #[test]
    fn taiex_small_movement_classified_neutral() {
        // TAIEX 小幅 monowave(< 0.5%)應被 neutrality 改 Neutral
        let bars = vec![
            bar("2026-01-01", 22000.0, 22020.0, 21990.0, 22010.0),
            bar("2026-01-02", 22010.0, 22050.0, 22000.0, 22040.0),
            bar("2026-01-03", 22040.0, 22080.0, 22030.0, 22070.0),
        ];
        let cfg = NeelyEngineConfig::default();
        let waves = detect_monowaves(&bars, cfg.atr_period);
        let classified = classify_monowaves(
            &bars,
            waves,
            neutrality::TAIEX_RESERVED_STOCK_ID,
            &cfg,
        );
        // 22010 → 22070(+0.27%)< 0.5% → Neutral
        assert!(classified.iter().all(|c| matches!(
            c.monowave.direction,
            MonowaveDirection::Neutral
        )));
    }
}
