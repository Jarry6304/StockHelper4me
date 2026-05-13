// advanced_rules — Stage 7.5:Channeling + Ch9 Advanced Rules
//
// 對齊 m3Spec/neely_rules.md §Ch5 Channeling(1346-1363 行)
//       + §Ch9 Basic Neely Extensions(1930-1994 行)
//       + m3Spec/neely_core_architecture.md §7.1 Stage 7.5
//
// **Phase 7 PR**(2026-05-13)— 諮詢性 stage:
//   - Channeling:0-2 / 1-3 / 2-4 / 0-B / B-D 5 條 trendlines 構造 + 突破偵測
//   - Ch9 Trendline Touchpoints:5+ 點觸線 → Impulse 不可能
//   - Ch9 Time Rule:3 相鄰同級波不可時間皆等
//   - Ch9 Exception Rule:單規則失靈但符合 Aspect 1 情境之一 → 容許
//
// **諮詢性**:輸出 AdvisoryFinding 寫進 Scenario.advisory_findings,
// 不直接 retain/filter scenarios。Stage 8 Compaction 後由 Aggregation Layer 用。

use crate::monowave::ClassifiedMonowave;
use crate::output::{
    AdvisoryFinding, AdvisorySeverity, MonowaveDirection, NeelyPatternType, RuleId, Scenario,
    TriangleKind,
};

pub mod channeling;
pub mod ch9;

/// Stage 7.5 主入口:對每個 Scenario 跑 Channeling + Ch9 Advanced Rules,
/// 將 AdvisoryFinding 寫進 scenario.advisory_findings。
pub fn run(scenarios: &mut [Scenario], classified: &[ClassifiedMonowave]) {
    for scenario in scenarios.iter_mut() {
        let mut findings = Vec::new();

        // Channeling 分析(依 pattern_type 選擇 trendlines)
        let channel_findings = channeling::analyze(scenario, classified);
        findings.extend(channel_findings);

        // Ch9 Trendline Touchpoints
        if let Some(f) = ch9::check_trendline_touchpoints(scenario, classified) {
            findings.push(f);
        }

        // Ch9 Time Rule(3 相鄰同級波不可時間皆等)
        if let Some(f) = ch9::check_time_rule(scenario, classified) {
            findings.push(f);
        }

        // Ch9 Structure Integrity(已 compacted_base_label 設定 → integrity 已鎖)
        findings.push(AdvisoryFinding {
            rule_id: RuleId::Ch9_StructureIntegrity,
            severity: AdvisorySeverity::Info,
            message: format!(
                "Structure integrity: base = {:?} (locked,後續不可隨意修改)",
                scenario.compacted_base_label
            ),
        });

        scenario.advisory_findings = findings;
    }
}

/// 一些共用 helpers exposed for module-internal use。
pub(crate) fn pattern_is_correction(pattern: &NeelyPatternType) -> bool {
    matches!(
        pattern,
        NeelyPatternType::Zigzag { .. }
            | NeelyPatternType::Flat { .. }
            | NeelyPatternType::Combination { .. }
    )
}

pub(crate) fn pattern_is_triangle(pattern: &NeelyPatternType) -> bool {
    matches!(pattern, NeelyPatternType::Triangle { .. })
}

pub(crate) fn pattern_is_impulsive(pattern: &NeelyPatternType) -> bool {
    matches!(
        pattern,
        NeelyPatternType::Impulse | NeelyPatternType::Diagonal { .. }
    )
}

/// Trendline 構造:從兩 (date, price) 點得線性外推函式。
pub(crate) fn linear_y_at(
    t1: chrono::NaiveDate,
    y1: f64,
    t2: chrono::NaiveDate,
    y2: f64,
    target: chrono::NaiveDate,
) -> Option<f64> {
    let dt = (t2 - t1).num_days() as f64;
    if dt.abs() < 1e-12 {
        return None;
    }
    let slope = (y2 - y1) / dt;
    let dt_target = (target - t1).num_days() as f64;
    Some(y1 + slope * dt_target)
}

/// 取出 scenario 對應的 monowave slice。
pub(crate) fn scenario_monowaves<'a>(
    scenario: &Scenario,
    classified: &'a [ClassifiedMonowave],
) -> &'a [ClassifiedMonowave] {
    let start_date = scenario.wave_tree.start;
    let end_date = scenario.wave_tree.end;
    let start_idx = classified
        .iter()
        .position(|c| c.monowave.start_date >= start_date)
        .unwrap_or(0);
    let end_idx = classified
        .iter()
        .rposition(|c| c.monowave.end_date <= end_date)
        .map(|i| i + 1)
        .unwrap_or(classified.len());
    if start_idx < end_idx {
        &classified[start_idx..end_idx]
    } else {
        &[]
    }
}

// 抑制未用警告(部分 helper 可能僅在 sub-module 使用)
#[allow(dead_code)]
fn _force_use_imports() {
    let _ = MonowaveDirection::Up;
    let _: TriangleKind = TriangleKind::Contracting;
}
