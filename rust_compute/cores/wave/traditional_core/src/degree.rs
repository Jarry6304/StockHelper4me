// degree.rs — Stage 8:相對度數 + Forest 組裝
//
// 對齊 traditional_rules.md L1–3「度數由相對大小/位置決定,不由絕對長度」。v1 以候選的 bar 跨度
// 給 `[工程添加]` 相對度數啟發(daily 級距);真正定度數需 multi-timeframe context(v2)。
// Forest **不選 primary**:超過 forest_max_size 時以 `preference_score` 為 beam 鍵保留 top-N。

use crate::output::{Degree, TraditionalScenario};

/// `[工程添加]` daily 級距:bar 跨度 → 相對度數(9 級)。
pub fn degree_for_span(span_bars: usize) -> Degree {
    match span_bars {
        0..=19 => Degree::Subminuette,
        20..=59 => Degree::Minuette,
        60..=119 => Degree::Minute,
        120..=249 => Degree::Minor,
        250..=499 => Degree::Intermediate,
        500..=999 => Degree::Primary,
        1000..=1999 => Degree::Cycle,
        2000..=3999 => Degree::Supercycle,
        _ => Degree::GrandSupercycle,
    }
}

/// 依 preference_score 降序排列(UI 偏好,**非** primary 標記);超過 max 走 beam fallback 保留 top-N。
/// 回傳 (forest, overflow_triggered)。
pub fn finalize_forest(
    mut scenarios: Vec<TraditionalScenario>,
    max: usize,
) -> (Vec<TraditionalScenario>, bool) {
    scenarios.sort_by(|a, b| b.preference_score.cmp(&a.preference_score));
    if scenarios.len() > max {
        scenarios.truncate(max);
        (scenarios, true)
    } else {
        (scenarios, false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn span_buckets() {
        assert_eq!(degree_for_span(5), Degree::Subminuette);
        assert_eq!(degree_for_span(100), Degree::Minute);
        assert_eq!(degree_for_span(300), Degree::Intermediate);
        assert_eq!(degree_for_span(5000), Degree::GrandSupercycle);
    }
}
