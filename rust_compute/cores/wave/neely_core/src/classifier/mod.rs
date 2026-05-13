// classifier — Stage 5:Pattern Classifier
//
// 對齊 m3Spec/neely_core_architecture.md r5 §9.6 + neely_rules.md Ch5/Ch11。
//
// 給通過 Validator 的 candidate 命名 pattern_type:
//   Impulse / TerminalImpulse / Zigzag / Flat / Triangle / Combination / RunningCorrection
//
// **PR-3c-pre 階段(2026-05-13)**:
//   - wave_count == 5 + Ch5Essential(3) pass → Impulse(strict)
//   - wave_count == 5 + Ch5Essential(3) fail → TerminalImpulse(r5 §9.6 取代 r2 Diagonal)
//   - wave_count == 3 → Zigzag { Normal }(預設;Flat / Triangle / Combination 區分留 PR-4b)
//
// 留後續 PR(對齊 §9.6):
//   - TerminalImpulse 完整 sub-wave 結構檢查(留 PR-4b)
//   - Zigzag Normal / Elongated / Truncated 區分(留 PR-3c-1 by Z2 c-wave ratio)
//   - Flat 7 變體(留 PR-3c-1 + PR-4b)
//   - Triangle 9 變體(留 PR-3c-2)
//   - Combination DoubleThree / TripleThree(留 PR-4b)

use crate::candidates::WaveCandidate;
use crate::output::{
    ComplexityLevel, FibZone, NeelyPatternType, PostBehavior, PowerRating,
    RuleId, Scenario, StructuralFacts, Trigger, WaveNode, ZigzagVariant,
};
use crate::monowave::ClassifiedMonowave;
use crate::validator::ValidationReport;

/// Stage 5 結果:Classifier 給 candidate 命名 pattern + 組裝成 Scenario(待 Stage 8 進 Forest)。
///
/// 注意:Scenario 的 power_rating / fibonacci / triggers 等屬性留 Stage 9-10 補完,
/// 本 Stage 5 階段先填預設值(Neutral / 空 vec)。
pub fn classify(
    candidate: &WaveCandidate,
    report: &ValidationReport,
    classified: &[ClassifiedMonowave],
) -> Option<Scenario> {
    if !report.overall_pass {
        return None;
    }
    let mi = &candidate.monowave_indices;
    if mi.is_empty() || mi.iter().any(|&idx| idx >= classified.len()) {
        return None;
    }

    let pattern_type = match candidate.wave_count {
        5 => classify_5wave(candidate, report),
        3 => classify_3wave(candidate, report),
        _ => return None,
    };

    let structure_label = format!(
        "{:?} ({}-wave from mw{} to mw{})",
        pattern_type, candidate.wave_count, mi[0], mi[mi.len() - 1]
    );

    let wave_tree = build_wave_tree(candidate, classified);

    Some(Scenario {
        id: candidate.id.clone(),
        wave_tree,
        pattern_type,
        structure_label,
        complexity_level: classify_complexity(candidate),
        power_rating: PowerRating::Neutral,
        max_retracement: 0.0,
        post_pattern_behavior: PostBehavior::Unconstrained,
        passed_rules: report
            .passed
            .iter()
            .cloned()
            .chain(default_passed_rules(candidate, report))
            .collect(),
        deferred_rules: report.deferred.clone(),
        rules_passed_count: report.passed.len(),
        deferred_rules_count: report.deferred.len(),
        invalidation_triggers: Vec::<Trigger>::new(),
        expected_fib_zones: Vec::<FibZone>::new(),
        structural_facts: StructuralFacts::default(),
    })
}

fn classify_5wave(_candidate: &WaveCandidate, report: &ValidationReport) -> NeelyPatternType {
    // R3(W4 不重疊 W1)是 Impulse vs TerminalImpulse 的判別關鍵
    // r5 §9.6:Neely 派用 TerminalImpulse(取代 Prechter Diagonal Leading/Ending)
    let r3_failed = report.failed.iter().any(|r| r.rule_id == RuleId::Ch5Essential(3));

    if r3_failed {
        // W4 重疊 W1 → TerminalImpulse(Neely 派術語)
        // 1st/3rd/5th Ext / Non-Ext sub_kind 區分留 PR-4b 校準
        NeelyPatternType::TerminalImpulse
    } else {
        // R3 通過 → strict Impulse
        NeelyPatternType::Impulse
    }
}

fn classify_3wave(_candidate: &WaveCandidate, _report: &ValidationReport) -> NeelyPatternType {
    // 3-wave correction 預設 Zigzag { Normal }(典型 61.8-161.8% × a)
    // Elongated / Truncated 區分留 PR-3c-1(by Z2 c-wave ratio)
    // Flat / Triangle / Combination 區分留 PR-4b
    // 注意:Triangle 嚴格定義是 5-wave (A-B-C-D-E),這裡 3-wave 不會是 Triangle
    NeelyPatternType::Zigzag {
        sub_kind: ZigzagVariant::Normal,
    }
}

fn classify_complexity(candidate: &WaveCandidate) -> ComplexityLevel {
    // 基本 Complexity Rule(對齊 m3Spec/ Stage 7):
    //   3 wave → Simple
    //   5 wave → Intermediate
    //   5+ nested wave → Complex(留 PR-4b)
    match candidate.wave_count {
        3 => ComplexityLevel::Simple,
        5 => ComplexityLevel::Intermediate,
        _ => ComplexityLevel::Complex,
    }
}

fn build_wave_tree(candidate: &WaveCandidate, classified: &[ClassifiedMonowave]) -> WaveNode {
    let mi = &candidate.monowave_indices;
    let start = classified[mi[0]].monowave.start_date;
    let end = classified[mi[mi.len() - 1]].monowave.end_date;
    let label = format!(
        "{}-wave {:?}",
        candidate.wave_count, candidate.initial_direction
    );

    // 子節點:每個 sub-wave 是一個 WaveNode(no children — 留 PR-4b 嵌套)
    let children = mi
        .iter()
        .enumerate()
        .map(|(i, &idx)| {
            let mw = &classified[idx].monowave;
            WaveNode {
                label: format!("W{}", i + 1),
                start: mw.start_date,
                end: mw.end_date,
                children: Vec::new(),
            }
        })
        .collect();

    WaveNode {
        label,
        start,
        end,
        children,
    }
}

/// 預設 passed rule list(report.passed 目前 PR-3b 沒填,本 helper 從 deferred / failed 反推)。
/// PR-3c-1 補完 validator 後可移除。
fn default_passed_rules(
    candidate: &WaveCandidate,
    report: &ValidationReport,
) -> Vec<RuleId> {
    // R1-R3 對 wave_count >= 3 適用,若沒在 failed 也沒在 deferred,則視為 passed
    // 對齊 r5 §9.3:RuleId::Ch5Essential(N) 取代 r2 RuleId::Core(N)
    let mut passed = Vec::new();
    let r1 = RuleId::Ch5Essential(1);
    let r2 = RuleId::Ch5Essential(2);
    let r3 = RuleId::Ch5Essential(3);

    let in_failed = |r: &RuleId| report.failed.iter().any(|f| f.rule_id == *r);
    let in_deferred = |r: &RuleId| report.deferred.contains(r);
    let in_n_a = |r: &RuleId| report.not_applicable.contains(r);

    for r in &[r1, r2, r3] {
        if !in_failed(r) && !in_deferred(r) && !in_n_a(r) {
            passed.push(r.clone());
        }
    }

    // wave_count + initial_direction 是 candidate 本身屬性,不在 RuleId 範圍
    let _ = candidate;

    passed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::candidates::WaveCandidate;
    use crate::monowave::ProportionMetrics;
    use crate::output::{
        CombinationKind, FlatVariant, Monowave, MonowaveDirection, TriangleVariant,
    };
    use chrono::NaiveDate;

    fn cmw(start_p: f64, end_p: f64, dir: MonowaveDirection) -> ClassifiedMonowave {
        ClassifiedMonowave {
            monowave: Monowave {
                start_date: NaiveDate::parse_from_str("2026-01-01", "%Y-%m-%d").unwrap(),
                end_date: NaiveDate::parse_from_str("2026-01-05", "%Y-%m-%d").unwrap(),
                start_price: start_p,
                end_price: end_p,
                direction: dir,
            },
            atr_at_start: 1.0,
            metrics: ProportionMetrics {
                magnitude: (end_p - start_p).abs(),
                duration_bars: 5,
                atr_relative: 5.0,
                slope_vs_45deg: 1.0,
            },
        }
    }

    fn make_5wave_impulse_classified() -> Vec<ClassifiedMonowave> {
        vec![
            cmw(100.0, 110.0, MonowaveDirection::Up),
            cmw(110.0, 105.0, MonowaveDirection::Down),
            cmw(105.0, 125.0, MonowaveDirection::Up),
            cmw(125.0, 118.0, MonowaveDirection::Down),
            cmw(118.0, 132.0, MonowaveDirection::Up),
        ]
    }

    fn make_candidate_5wave() -> WaveCandidate {
        WaveCandidate {
            id: "c5-mw0-mw4".to_string(),
            monowave_indices: vec![0, 1, 2, 3, 4],
            wave_count: 5,
            initial_direction: MonowaveDirection::Up,
        }
    }

    fn make_passing_report() -> ValidationReport {
        ValidationReport {
            candidate_id: "c5-mw0-mw4".to_string(),
            passed: vec![],
            failed: vec![],
            deferred: vec![
                RuleId::Ch5Essential(4), RuleId::Ch5Essential(5),
                RuleId::Ch5Essential(6), RuleId::Ch5Essential(7),
                RuleId::Ch5FlatMinBRatio, RuleId::Ch5FlatMinCRatio,
                RuleId::Ch5ZigzagMaxBRetracement, RuleId::Ch5ZigzagCTriangleException,
                RuleId::Ch4ZigzagDetour,
                RuleId::Ch5TriangleBRange, RuleId::Ch5TriangleLegContraction,
                RuleId::Ch5TriangleLegEquality5Pct,
                RuleId::Ch5OverlapTrending, RuleId::Ch5OverlapTerminal,
            ],
            not_applicable: vec![],
            overall_pass: true,
        }
    }

    #[test]
    fn five_wave_r3_pass_classified_as_impulse() {
        let classified = make_5wave_impulse_classified();
        let candidate = make_candidate_5wave();
        let report = make_passing_report();
        let scenario = classify(&candidate, &report, &classified).expect("應產生 Scenario");
        assert!(matches!(scenario.pattern_type, NeelyPatternType::Impulse));
        assert!(matches!(scenario.complexity_level, ComplexityLevel::Intermediate));
        assert_eq!(scenario.id, "c5-mw0-mw4");
        assert_eq!(scenario.wave_tree.children.len(), 5);
    }

    #[test]
    fn five_wave_r3_fail_classified_as_terminal_impulse() {
        let classified = make_5wave_impulse_classified();
        let candidate = make_candidate_5wave();
        let mut report = make_passing_report();
        // 模擬 R3 fail(對齊 r5 §9.6:r2 Diagonal 改 TerminalImpulse)
        report.failed.push(crate::output::RuleRejection {
            candidate_id: "c5-mw0-mw4".to_string(),
            rule_id: RuleId::Ch5Essential(3),
            expected: "test".to_string(),
            actual: "test".to_string(),
            gap: 0.0,
            neely_page: "test".to_string(),
        });
        // overall_pass 仍 true(模擬 PR-4 Post-Validator 容許 TerminalImpulse)
        report.overall_pass = true;
        let scenario = classify(&candidate, &report, &classified)
            .expect("應產生 TerminalImpulse Scenario");
        assert!(matches!(scenario.pattern_type, NeelyPatternType::TerminalImpulse));
    }

    #[test]
    fn three_wave_classified_as_zigzag_normal() {
        let classified = vec![
            cmw(100.0, 110.0, MonowaveDirection::Up),
            cmw(110.0, 100.0, MonowaveDirection::Down),
            cmw(100.0, 115.0, MonowaveDirection::Up),
        ];
        let candidate = WaveCandidate {
            id: "c3-mw0-mw2".to_string(),
            monowave_indices: vec![0, 1, 2],
            wave_count: 3,
            initial_direction: MonowaveDirection::Up,
        };
        let report = make_passing_report();
        let scenario = classify(&candidate, &report, &classified).expect("應產生 Scenario");
        assert!(matches!(
            scenario.pattern_type,
            NeelyPatternType::Zigzag { sub_kind: ZigzagVariant::Normal }
        ));
        assert!(matches!(scenario.complexity_level, ComplexityLevel::Simple));
    }

    #[test]
    fn failed_validation_yields_no_scenario() {
        let classified = make_5wave_impulse_classified();
        let candidate = make_candidate_5wave();
        let mut report = make_passing_report();
        report.overall_pass = false;
        report.failed.push(crate::output::RuleRejection {
            candidate_id: "c5-mw0-mw4".to_string(),
            rule_id: RuleId::Ch5Essential(1),
            expected: "test".to_string(),
            actual: "test".to_string(),
            gap: 0.0,
            neely_page: "test".to_string(),
        });
        assert!(classify(&candidate, &report, &classified).is_none());
    }

    // 觸發 enum exhaustive 檢查:確保 FlatVariant / TriangleVariant / CombinationKind
    // 都有定義(編譯期檢查,不需 runtime test)
    #[allow(dead_code)]
    fn _enum_exhaustive_smoke() {
        let _: FlatVariant = FlatVariant::Common;
        let _: TriangleVariant = TriangleVariant::HorizontalLimiting;
        let _: CombinationKind = CombinationKind::DoubleThree;
    }
}
