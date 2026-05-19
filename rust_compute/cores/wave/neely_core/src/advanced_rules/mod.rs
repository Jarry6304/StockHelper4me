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
/// v4.2 P1.2 #12:Ch12 Localized Progress Label Changes 偵測(原 spec-only,本 PR 落地)
pub mod ch12_localized;

/// Stage 7.5 主入口:對每個 Scenario 跑 Channeling + Ch9 Advanced Rules + Ch12 Waterfall/Localized,
/// 將 AdvisoryFinding 寫進 scenario.advisory_findings。
///
/// v4.2 P1.2 變動(2026-05-19):
///   - 加 Ch9 Independent / Simultaneous / Exception Aspect 2 三 advisory checks
///   - 加 Ch12 Waterfall Effect 偵測(`fibonacci::waterfall`)
///   - 加 Ch12 Localized Changes 偵測(`advanced_rules::ch12_localized`)
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

        // v4.2 P1.2 #8:Ch9 Simultaneous Occurrence(Impulse 預期 Ch5_Essential R1-R7 全 passed)
        if let Some(f) = ch9::check_simultaneous_occurrence(scenario) {
            findings.push(f);
        }

        // v4.2 P1.2 #11:Ch12 Waterfall Effect(W3 / W5 超 2.618 + 5% 容差)
        if let Some(f) = crate::fibonacci::waterfall::check_waterfall_effect(scenario, classified) {
            findings.push(f);
        }

        // v4.2 P1.2 #12:Ch12 Localized Progress Label Changes
        if let Some(f) = ch12_localized::detect_localized_changes(scenario) {
            findings.push(f);
        }

        // v4.3a P1.3a:Ch11 Trending Impulse Wave-by-Wave 變體規則(advisory only)
        findings.extend(crate::validator::ch11_trending_impulse::analyze(
            scenario, classified,
        ));

        // v4.3b P1.3b:Ch11 Terminal Impulse Wave-by-Wave 變體規則(Diagonal pattern)
        findings.extend(crate::validator::ch11_terminal_impulse::analyze(
            scenario, classified,
        ));

        // v4.3c P1.3c:Ch11 Flat 七變體 wave-a/b/c 規則
        findings.extend(crate::validator::ch11_flat_variants::analyze(
            scenario, classified,
        ));

        // v4.3d P1.3d:Ch11 Zigzag wave-a/b/c 進階規則 + Appendix B 項 F
        findings.extend(crate::validator::ch11_zigzag::analyze(scenario, classified));

        // v4.3e P1.3e:Ch11 Triangle 9 變體 wave-a-e 規則(P1.3 最後 sub-PR)
        findings.extend(crate::validator::ch11_triangle_variants::analyze(
            scenario, classified,
        ));

        // 寫入 advisory_findings(此處 set,需在 Independent / Exception Aspect 2 之前)
        scenario.advisory_findings = findings;

        // v4.2 P1.2 #7:Ch9 Independent Rule advisory — 依「scenario 啟動章節數」判定,
        // 須在前面 findings 寫入後才執行(count_active_chapters 讀 scenario.advisory_findings)
        if let Some(f) = ch9::check_independent_rule(scenario) {
            scenario.advisory_findings.push(f);
        }

        // v4.2 P1.2 #9:Ch9 Exception Aspect 2 — Trendline Strong + Diagonal → 觸發 Terminal Impulse
        if let Some(f) = ch9::detect_exception_aspect_2(scenario) {
            scenario.advisory_findings.push(f);
        }
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
