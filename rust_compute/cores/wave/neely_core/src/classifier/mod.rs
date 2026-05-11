// classifier — Stage 5:Pattern Classifier
//
// 對齊 m2Spec/oldm2Spec/neely_core.md §三 / §七 Stage 5 / §九 NeelyPatternType。
//
// 給通過 Validator 的 candidate 命名 pattern_type:
//   Impulse / Diagonal / Zigzag / Flat / Triangle / Combination
//
// **M3 PR-4 階段**(先實踐以後再改):
//   - wave_count == 5 + R3 pass → Impulse(strict)
//   - wave_count == 5 + R3 fail → Diagonal { Leading }(寬鬆 — sub_kind 留 PR-4b 校準)
//   - wave_count == 3 → Zigzag { Single }(預設 — Flat / Triangle / Combination 區分
//     留 PR-4b 對齊 m3Spec/ neely 最新 spec 後校準)
//
// 留後續 PR(對齊 §九):
//   - Diagonal Leading vs Ending 區分(需要 W2/W4 的 sub-wave 結構)
//   - Zigzag Single / Double / Triple
//   - Flat Regular / Expanded / Running
//   - Triangle Contracting / Expanding / Limiting
//   - Combination DoubleThree / TripleThree

use crate::candidates::WaveCandidate;
use crate::output::{
    ComplexityLevel, DiagonalKind, FibZone, NeelyPatternType,
    PostBehavior, PowerRating, RuleId, Scenario, StructuralFacts, Trigger,
    WaveNode, ZigzagKind,
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
        power_rating: PowerRating::Neutral, // PR-6 Power Rating 查表後填
        max_retracement: 0.0,               // PR-6 Power Rating 查表後填
        post_pattern_behavior: PostBehavior::Indeterminate,
        passed_rules: report
            .passed
            .iter()
            .copied()
            .chain(default_passed_rules(candidate, report))
            .collect(),
        deferred_rules: report.deferred.clone(),
        rules_passed_count: report.passed.len(),
        deferred_rules_count: report.deferred.len(),
        invalidation_triggers: Vec::<Trigger>::new(), // PR-6 triggers 補
        expected_fib_zones: Vec::<FibZone>::new(),    // PR-6 Fibonacci 補
        structural_facts: StructuralFacts::default(),  // PR-6 補
    })
}

fn classify_5wave(_candidate: &WaveCandidate, report: &ValidationReport) -> NeelyPatternType {
    // R3(W4 不重疊 W1)是 Impulse vs Diagonal 的判別關鍵
    let r3_failed = report.failed.iter().any(|r| r.rule_id == RuleId::Core(3));

    if r3_failed {
        // W4 重疊 W1 → Diagonal
        // Leading vs Ending 區分需 sub-wave 結構,本階段預設 Leading
        // 留 PR-4b 校準
        NeelyPatternType::Diagonal {
            sub_kind: DiagonalKind::Leading,
        }
    } else {
        // R3 通過 → strict Impulse
        NeelyPatternType::Impulse
    }
}

fn classify_3wave(_candidate: &WaveCandidate, _report: &ValidationReport) -> NeelyPatternType {
    // 3-wave correction 預設 Zigzag { Single }
    // Flat / Triangle / Combination 區分留 PR-4b
    // 注意:Triangle 嚴格定義是 5-wave (A-B-C-D-E),這裡 3-wave 不會是 Triangle
    NeelyPatternType::Zigzag {
        sub_kind: ZigzagKind::Single,
    }
}

fn classify_complexity(candidate: &WaveCandidate) -> ComplexityLevel {
    // 基本 Complexity Rule(對齊 m2Spec/oldm2Spec/neely_core.md §七 Stage 7):
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
/// PR-3c 補完 validator 後可移除。
fn default_passed_rules(
    candidate: &WaveCandidate,
    report: &ValidationReport,
) -> Vec<RuleId> {
    // R1-R3 對 wave_count >= 3 適用,若沒在 failed 也沒在 deferred,則視為 passed
    let mut passed = Vec::new();
    let r1 = RuleId::Core(1);
    let r2 = RuleId::Core(2);
    let r3 = RuleId::Core(3);

    let in_failed = |r: RuleId| report.failed.iter().any(|f| f.rule_id == r);
    let in_deferred = |r: RuleId| report.deferred.contains(&r);
    let in_n_a = |r: RuleId| report.not_applicable.contains(&r);

    for r in &[r1, r2, r3] {
        if !in_failed(*r) && !in_deferred(*r) && !in_n_a(*r) {
            passed.push(*r);
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
        CombinationKind, FlatKind, Monowave, MonowaveDirection, TriangleKind,
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
                RuleId::Core(4), RuleId::Core(5), RuleId::Core(6), RuleId::Core(7),
                RuleId::Flat(1), RuleId::Flat(2),
                RuleId::Zigzag(1), RuleId::Zigzag(2), RuleId::Zigzag(3), RuleId::Zigzag(4),
                RuleId::Triangle(1), RuleId::Triangle(2), RuleId::Triangle(3),
                RuleId::Triangle(4), RuleId::Triangle(5), RuleId::Triangle(6),
                RuleId::Triangle(7), RuleId::Triangle(8), RuleId::Triangle(9), RuleId::Triangle(10),
                RuleId::Wave(1), RuleId::Wave(2),
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
    fn five_wave_r3_fail_classified_as_diagonal() {
        let classified = make_5wave_impulse_classified();
        let candidate = make_candidate_5wave();
        let mut report = make_passing_report();
        // 模擬 R3 fail
        report.failed.push(crate::output::RuleRejection {
            candidate_id: "c5-mw0-mw4".to_string(),
            rule_id: RuleId::Core(3),
            expected: "test".to_string(),
            actual: "test".to_string(),
            gap: 0.0,
            neely_page: "test".to_string(),
        });
        // overall_pass 仍 true(模擬 PR-4 Post-Validator 容許 Diagonal)
        report.overall_pass = true;
        let scenario = classify(&candidate, &report, &classified).expect("應產生 Diagonal Scenario");
        assert!(matches!(
            scenario.pattern_type,
            NeelyPatternType::Diagonal { sub_kind: DiagonalKind::Leading }
        ));
    }

    #[test]
    fn three_wave_classified_as_zigzag_simple() {
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
            NeelyPatternType::Zigzag { sub_kind: ZigzagKind::Single }
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
            rule_id: RuleId::Core(1),
            expected: "test".to_string(),
            actual: "test".to_string(),
            gap: 0.0,
            neely_page: "test".to_string(),
        });
        assert!(classify(&candidate, &report, &classified).is_none());
    }

    // 觸發 enum exhaustive 檢查:確保 FlatKind / TriangleKind / CombinationKind
    // 都有定義(編譯期檢查,不需 runtime test)
    #[allow(dead_code)]
    fn _enum_exhaustive_smoke() {
        let _: FlatKind = FlatKind::Regular;
        let _: TriangleKind = TriangleKind::Contracting;
        let _: CombinationKind = CombinationKind::DoubleThree;
    }
}
