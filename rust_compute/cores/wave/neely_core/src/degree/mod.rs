// degree — Stage 11:Degree Ceiling 推導
//
// 對齊 m3Spec/neely_core_architecture.md §8.5 DegreeCeiling + §13.3 Degree ceiling 推導表
//       + §7.1 Stage 11
//
// **Phase 12 PR(r5 alignment)**:
//   - 依資料量自動推導本次分析能達到的最高 Degree(精華版 Ch7 11 級)
//   - Daily 閾值:< 1y / 1-3y / 3-10y / 10-30y / > 30y
//   - Weekly / Monthly / Quarterly 用 bar count × bar_size 估算實際時間跨度
//   - 寫入 NeelyCoreOutput.degree_ceiling 供 Aggregation Layer 截斷顯示

use crate::output::{Degree, DegreeCeiling, OhlcvBar};
use fact_schema::Timeframe;

/// Stage 11 主入口:依輸入 bars 計算 Degree Ceiling。
pub fn compute_ceiling(bars: &[OhlcvBar], timeframe: Timeframe) -> DegreeCeiling {
    if bars.is_empty() {
        return DegreeCeiling {
            max_reachable_degree: Degree::SubMicro,
            reason: "no data".to_string(),
        };
    }
    let first = bars.first().unwrap().date;
    let last = bars.last().unwrap().date;
    let span_days = (last - first).num_days().max(0);
    let span_years = span_days as f64 / 365.25;

    let max_reachable_degree = classify_degree(span_years);
    let timeframe_label = timeframe_label(timeframe);
    let reason = format!(
        "data spans {:.1} years ({} bars on {}), reaches {:?} at best",
        span_years,
        bars.len(),
        timeframe_label,
        max_reachable_degree
    );

    DegreeCeiling {
        max_reachable_degree,
        reason,
    }
}

/// 依資料時間跨度(年)推導 Degree(對齊 spec §13.3 表)。
fn classify_degree(years: f64) -> Degree {
    if years < 1.0 {
        Degree::SubMinuette
    } else if years < 3.0 {
        // 1-3 年 → Minuette / Minute(取中段 Minute,Daily 級別此處保守)
        Degree::Minute
    } else if years < 10.0 {
        // 3-10 年 → Minor / Intermediate(spec 寫「3-10 年 Minor / Intermediate」,
        // 取偏小的 Minor 較保守,避免 Aggregation 層誤標為更大級)
        Degree::Minor
    } else if years < 30.0 {
        Degree::Primary
    } else if years < 100.0 {
        Degree::Cycle
    } else {
        Degree::Supercycle
    }
}

fn timeframe_label(tf: Timeframe) -> &'static str {
    match tf {
        Timeframe::Daily => "daily",
        Timeframe::Weekly => "weekly",
        Timeframe::Monthly => "monthly",
        Timeframe::Quarterly => "quarterly",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn bar(d: &str) -> OhlcvBar {
        OhlcvBar {
            date: NaiveDate::parse_from_str(d, "%Y-%m-%d").unwrap(),
            open: 100.0,
            high: 100.0,
            low: 100.0,
            close: 100.0,
            volume: None,
        }
    }

    #[test]
    fn empty_bars_returns_submicro_no_data() {
        let dc = compute_ceiling(&[], Timeframe::Daily);
        assert_eq!(dc.max_reachable_degree, Degree::SubMicro);
        assert!(dc.reason.contains("no data"));
    }

    #[test]
    fn six_months_yields_subminuette() {
        let bars = vec![bar("2025-01-01"), bar("2025-06-30")];
        let dc = compute_ceiling(&bars, Timeframe::Daily);
        assert_eq!(dc.max_reachable_degree, Degree::SubMinuette);
    }

    #[test]
    fn two_years_yields_minute() {
        let bars = vec![bar("2024-01-01"), bar("2026-01-01")];
        let dc = compute_ceiling(&bars, Timeframe::Daily);
        assert_eq!(dc.max_reachable_degree, Degree::Minute);
    }

    #[test]
    fn five_years_yields_minor() {
        let bars = vec![bar("2021-01-01"), bar("2026-01-01")];
        let dc = compute_ceiling(&bars, Timeframe::Daily);
        assert_eq!(dc.max_reachable_degree, Degree::Minor);
    }

    #[test]
    fn fifteen_years_yields_primary() {
        let bars = vec![bar("2011-01-01"), bar("2026-01-01")];
        let dc = compute_ceiling(&bars, Timeframe::Daily);
        assert_eq!(dc.max_reachable_degree, Degree::Primary);
    }

    #[test]
    fn fifty_years_yields_cycle() {
        let bars = vec![bar("1976-01-01"), bar("2026-01-01")];
        let dc = compute_ceiling(&bars, Timeframe::Monthly);
        assert_eq!(dc.max_reachable_degree, Degree::Cycle);
        assert!(dc.reason.contains("monthly"));
    }

    #[test]
    fn reason_contains_timeframe_label() {
        let bars = vec![bar("2024-01-01"), bar("2026-01-01")];
        let dc = compute_ceiling(&bars, Timeframe::Weekly);
        assert!(dc.reason.contains("weekly"));
    }
}
