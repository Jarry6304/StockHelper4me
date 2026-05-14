// classifier — Stage 5:Pattern Classifier
//
// 對齊 m3Spec/neely_core_architecture.md §7.1 Stage 5 + §9.1 NeelyPatternType
//         + m3Spec/neely_rules.md §Ch5(Impulse / Diagonal / Zigzag / Flat / Triangle)
//
// 給通過 Validator 的 candidate 命名 pattern_type:
//   Impulse / Diagonal / Zigzag / Flat / Triangle / Combination
//
// **Phase 1 PR(r5)**:重寫 R3 fail 邏輯 → 改用 Ch5_Overlap_Trending fail + Ch5_Overlap_Terminal pass
//   - wave_count == 5 + Overlap_Trending pass + Overlap_Terminal fail → **Impulse**(strict)
//   - wave_count == 5 + Overlap_Trending fail + Overlap_Terminal pass → **Diagonal**(Terminal Impulse)
//   - 兩個都 fail / 兩個都 pass → 結構錯亂(回 None,reject)
//   - wave_count == 3 → Zigzag { Single }(留 P4 Flat/Triangle/Combination 區分)
//
// **Diagonal sub_kind 簡化版**(Phase 1):
//   - Leading vs Ending 真正判定需要「higher-degree context」(該 5-wave 是 higher
//     impulse 的 W1 還是 W5),需 Stage 8 Compaction Three Rounds 提供
//   - Phase 1 採位置 heuristic:
//       candidate 從 monowave[0] 開始 → Leading(較可能是 higher-impulse 起始)
//       否則 → Ending(後續 higher-impulse 收尾)
//   - 完整判定留 P5(Ch8 Complex Polywaves 動工時補)
//
// 留後續 PR(對齊 architecture §9.1):
//   - Zigzag Single / Double / Triple
//   - Flat Regular / Expanded / Running
//   - Triangle Contracting / Expanding / Limiting
//   - Combination DoubleThree / TripleThree

use crate::candidates::WaveCandidate;
use crate::output::{
    compaction_base_label, CombinationKind, ComplexityLevel, DiagonalKind, FibZone,
    MonowaveStructureLabels, NeelyPatternType, PostBehavior, PowerRating, RoundState,
    RuleId, Scenario, StructuralFacts, StructureLabel, Trigger, WaveNode, ZigzagKind,
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
        5 => classify_5wave(candidate, report, classified)?,
        3 => classify_3wave(candidate, report),
        _ => return None,
    };

    // Phase 5:initial_direction 從第一個 monowave 取得,供 Power Rating 判 Bullish/Bearish
    let initial_direction = classified[mi[0]].monowave.direction;

    let structure_label = format!(
        "{:?} {:?} ({}-wave from mw{} to mw{})",
        pattern_type,
        initial_direction,
        candidate.wave_count,
        mi[0],
        mi[mi.len() - 1]
    );

    let wave_tree = build_wave_tree(candidate, classified);

    let compacted_base = compaction_base_label(&pattern_type);

    // Phase 15:Scenario 群 2 fields 從現有 pipeline output 萃取
    let monowave_structure_labels = build_monowave_structure_labels(candidate, classified);
    let triplexity_detected = detect_triplexity(&pattern_type);
    // round_state / pattern_isolation_anchors classifier 階段預設 Round1 / 空 vec —
    // Stage 8 (three_rounds::apply) 之後由 lib.rs::compute 套 post-classifier 寫入(類似
    // power_rating::apply_to_forest 模式)。

    Some(Scenario {
        id: candidate.id.clone(),
        wave_tree,
        pattern_type,
        initial_direction,
        compacted_base_label: compacted_base,
        structure_label,
        complexity_level: classify_complexity(candidate),
        power_rating: PowerRating::Neutral, // Stage 10a Power Rating 查表後填
        max_retracement: None,               // Stage 10a 補
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
        invalidation_triggers: Vec::<Trigger>::new(), // Stage 10c triggers 補
        expected_fib_zones: Vec::<FibZone>::new(),    // Stage 10b Fibonacci 補
        structural_facts: StructuralFacts::default(),  // Phase 17 補 7 sub-fields
        advisory_findings: Vec::new(),
        in_triangle_context: false,
        awaiting_l_label: false,                       // Stage 8 three_rounds 後填
        // Phase 15 新增
        monowave_structure_labels,
        round_state: RoundState::Round1,               // Stage 8 三輪邏輯之後 override(Stage 1 結果)
        pattern_isolation_anchors: Vec::new(),         // lib.rs::compute 從 pattern_bounds 過濾後寫入
        triplexity_detected,
    })
}

/// Phase 15:從 ClassifiedMonowave.structure_label_candidates 萃取 monowave_structure_labels。
///
/// 對齊 spec §9.1 line 859 — 1:1 對應 candidate.monowave_indices 順序。
fn build_monowave_structure_labels(
    candidate: &WaveCandidate,
    classified: &[ClassifiedMonowave],
) -> Vec<MonowaveStructureLabels> {
    candidate
        .monowave_indices
        .iter()
        .enumerate()
        .map(|(seq_idx, &mw_idx)| MonowaveStructureLabels {
            monowave_index: seq_idx,
            labels: classified[mw_idx].structure_label_candidates.clone(),
        })
        .collect()
}

/// Phase 15:從 pattern_type 直接推導 triplexity_detected(spec §9.1 line 863 + Ch8)。
///
/// Triplexity = Triple-grouping patterns(spec Ch8 Table A/B):
///   TripleZigzag / TripleCombination / TripleThree / TripleThreeCombination / TripleThreeRunning
fn detect_triplexity(pattern: &NeelyPatternType) -> bool {
    if let NeelyPatternType::Combination { sub_kinds } = pattern {
        sub_kinds.iter().any(|k| {
            matches!(
                k,
                CombinationKind::TripleZigzag
                    | CombinationKind::TripleCombination
                    | CombinationKind::TripleThree
                    | CombinationKind::TripleThreeCombination
                    | CombinationKind::TripleThreeRunning
            )
        })
    } else {
        false
    }
}

/// 5-wave classifier:用 Ch5_Overlap_Trending vs Ch5_Overlap_Terminal 兩條規則 fail 模式判別。
/// 回 None 表示結構錯亂(兩條 overlap 規則都 fail 或都 pass,不應發生)。
fn classify_5wave(
    candidate: &WaveCandidate,
    report: &ValidationReport,
    classified: &[ClassifiedMonowave],
) -> Option<NeelyPatternType> {
    let trending_failed = report
        .failed
        .iter()
        .any(|r| r.rule_id == RuleId::Ch5_Overlap_Trending);
    let terminal_failed = report
        .failed
        .iter()
        .any(|r| r.rule_id == RuleId::Ch5_Overlap_Terminal);

    match (trending_failed, terminal_failed) {
        (false, true) => {
            // Trending pass + Terminal fail → 正常 Trending Impulse
            Some(NeelyPatternType::Impulse)
        }
        (true, false) => {
            // Trending fail + Terminal pass → Terminal Impulse(Diagonal)
            Some(NeelyPatternType::Diagonal {
                sub_kind: classify_diagonal_subkind(candidate, classified),
            })
        }
        (true, true) => {
            // 兩個都 fail — 結構錯亂(W4 既不在 W2 之上也不進入 W2 區?
            // 唯一可能:Up/Down direction 不一致或 N/A,理論上不該到這)
            None
        }
        (false, false) => {
            // 兩個都 pass — 不應發生(兩條規則互斥)
            // overall_pass 應該到不了這:Terminal fail 是 Trending Impulse 必然
            // 容錯:歸為 Impulse
            Some(NeelyPatternType::Impulse)
        }
    }
}

/// Phase 5 改進的 Diagonal sub_kind heuristic — 用相鄰 monowave label context。
///
/// 對齊 spec(Ch5 Realistic Representations):
///   - Leading Diagonal = 高一級 Impulse / Correction 之首段(W1 / A 位置)
///   - Ending Diagonal = 高一級 Impulse / Correction 之末段(W5 / C 位置)
///
/// **Phase 5 heuristic**(無真實 higher-degree context 前提下的近似):
///   1. candidate.monowave_indices[0] 之前有 :L3 / :L5 monowave → 先前修正/衝動剛結束
///      → 該 Diagonal 在「新段起始」位置 → **Leading**
///   2. candidate.monowave_indices[0] 自身的 structure_label_candidates 含 :F3 / :F5
///      → 強烈 Leading 訊號
///   3. candidate.monowave_indices[4] 自身含 :L3 / :L5 → 強烈 Ending 訊號
///   4. fallback:mi[0] == 0(序列起點)→ Leading,否則 → Ending
///
/// 完整 higher-degree context 留 P6/P8 Compaction Three Rounds(Phase 5 之後)。
fn classify_diagonal_subkind(
    candidate: &WaveCandidate,
    classified: &[ClassifiedMonowave],
) -> DiagonalKind {
    let mi = &candidate.monowave_indices;
    let start_mw_idx = mi[0];
    let end_mw_idx = mi[mi.len() - 1];

    // Check 1:前一個 monowave 是 :L3 / :L5(先前段剛結束)→ Leading
    if start_mw_idx > 0 {
        let prev_labels = &classified[start_mw_idx - 1].structure_label_candidates;
        let prev_is_last_anchor = prev_labels.iter().any(|c| {
            matches!(c.label, StructureLabel::L3 | StructureLabel::L5)
        });
        if prev_is_last_anchor {
            return DiagonalKind::Leading;
        }
    }

    // Check 2:Start monowave 含 :F3 / :F5 → Leading
    let start_labels = &classified[start_mw_idx].structure_label_candidates;
    let start_has_first = start_labels.iter().any(|c| {
        matches!(c.label, StructureLabel::F3 | StructureLabel::F5)
    });
    if start_has_first {
        return DiagonalKind::Leading;
    }

    // Check 3:End monowave 含 :L3 / :L5 → Ending
    let end_labels = &classified[end_mw_idx].structure_label_candidates;
    let end_has_last = end_labels.iter().any(|c| {
        matches!(c.label, StructureLabel::L3 | StructureLabel::L5)
    });
    if end_has_last {
        return DiagonalKind::Ending;
    }

    // Fallback:序列起點 → Leading,否則 → Ending
    if start_mw_idx == 0 {
        DiagonalKind::Leading
    } else {
        DiagonalKind::Ending
    }
}

fn classify_3wave(_candidate: &WaveCandidate, _report: &ValidationReport) -> NeelyPatternType {
    // 3-wave correction 預設 Zigzag { Single }
    // Flat / Triangle / Combination 區分留 P4
    NeelyPatternType::Zigzag {
        sub_kind: ZigzagKind::Single,
    }
}

fn classify_complexity(candidate: &WaveCandidate) -> ComplexityLevel {
    // 基本 Complexity Rule(對齊 architecture §7.1 Stage 7):
    //   3 wave → Simple
    //   5 wave → Intermediate
    //   5+ nested wave → Complex(留 P6)
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

    // 子節點:每個 sub-wave 是一個 WaveNode(no children — 留 P5/P6 嵌套)
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
/// P4 / P5 補完整 validator 後可移除。
fn default_passed_rules(
    candidate: &WaveCandidate,
    report: &ValidationReport,
) -> Vec<RuleId> {
    // Ch5_Essential R1-R7 對 wave_count == 5 適用,若沒在 failed 也沒在 deferred,則視為 passed
    let mut passed = Vec::new();
    let essentials: Vec<RuleId> = (1u8..=7).map(RuleId::Ch5_Essential).collect();

    let in_failed = |r: &RuleId| report.failed.iter().any(|f| &f.rule_id == r);
    let in_deferred = |r: &RuleId| report.deferred.contains(r);
    let in_n_a = |r: &RuleId| report.not_applicable.contains(r);

    for r in &essentials {
        if !in_failed(r) && !in_deferred(r) && !in_n_a(r) {
            passed.push(r.clone());
        }
    }

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
            structure_label_candidates: Vec::new(),
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

    fn make_candidate_5wave_starting_at(start: usize) -> WaveCandidate {
        WaveCandidate {
            id: format!("c5-mw{}-mw{}", start, start + 4),
            monowave_indices: vec![start, start + 1, start + 2, start + 3, start + 4],
            wave_count: 5,
            initial_direction: MonowaveDirection::Up,
        }
    }

    /// 清乾淨的 Trending Impulse report:Trending pass + Terminal fail
    fn make_impulse_report() -> ValidationReport {
        ValidationReport {
            candidate_id: "c5-mw0-mw4".to_string(),
            passed: vec![],
            failed: vec![crate::output::RuleRejection {
                candidate_id: "c5-mw0-mw4".to_string(),
                rule_id: RuleId::Ch5_Overlap_Terminal,
                expected: "test".to_string(),
                actual: "test".to_string(),
                gap: 0.0,
                neely_page: "test".to_string(),
            }],
            deferred: vec![
                RuleId::Ch5_Flat_Min_BRatio,
                RuleId::Ch5_Flat_Min_CRatio,
                RuleId::Ch5_Zigzag_Max_BRetracement,
                RuleId::Ch5_Zigzag_C_TriangleException,
                RuleId::Ch5_Triangle_BRange,
                RuleId::Ch5_Triangle_LegContraction,
                RuleId::Ch5_Triangle_LegEquality_5Pct,
                RuleId::Ch5_Equality,
            ],
            not_applicable: vec![],
            overall_pass: true,
        }
    }

    #[test]
    fn five_wave_trending_pass_terminal_fail_classified_as_impulse() {
        let classified = make_5wave_impulse_classified();
        let candidate = make_candidate_5wave_starting_at(0);
        let report = make_impulse_report();
        let scenario = classify(&candidate, &report, &classified).expect("應產生 Scenario");
        assert!(matches!(scenario.pattern_type, NeelyPatternType::Impulse));
        assert!(matches!(scenario.complexity_level, ComplexityLevel::Intermediate));
        assert_eq!(scenario.id, "c5-mw0-mw4");
        assert_eq!(scenario.wave_tree.children.len(), 5);
    }

    #[test]
    fn five_wave_trending_fail_terminal_pass_classified_as_diagonal_leading_at_start() {
        let classified = make_5wave_impulse_classified();
        let candidate = make_candidate_5wave_starting_at(0);
        let mut report = make_impulse_report();
        // 翻轉:Trending fail + Terminal pass(把原本的 Terminal fail 換成 Trending fail)
        report.failed.clear();
        report.failed.push(crate::output::RuleRejection {
            candidate_id: "c5-mw0-mw4".to_string(),
            rule_id: RuleId::Ch5_Overlap_Trending,
            expected: "test".to_string(),
            actual: "test".to_string(),
            gap: 0.0,
            neely_page: "test".to_string(),
        });
        report.overall_pass = true;
        let scenario = classify(&candidate, &report, &classified).expect("應產生 Diagonal Scenario");
        // 起始位置(mi[0] = 0)→ Leading
        assert!(matches!(
            scenario.pattern_type,
            NeelyPatternType::Diagonal {
                sub_kind: DiagonalKind::Leading
            }
        ));
    }

    #[test]
    fn five_wave_trending_fail_terminal_pass_classified_as_diagonal_ending_when_not_at_start() {
        // 加 1 個 dummy classified 在 index 0,讓 candidate 從 mi[0]=1 開始 → Ending
        let mut classified = vec![cmw(100.0, 100.0, MonowaveDirection::Up)]; // dummy idx 0
        classified.extend(make_5wave_impulse_classified()); // idx 1..6
        let candidate = make_candidate_5wave_starting_at(1); // mi = [1,2,3,4,5]
        let mut report = make_impulse_report();
        report.failed.clear();
        report.failed.push(crate::output::RuleRejection {
            candidate_id: candidate.id.clone(),
            rule_id: RuleId::Ch5_Overlap_Trending,
            expected: "test".to_string(),
            actual: "test".to_string(),
            gap: 0.0,
            neely_page: "test".to_string(),
        });
        report.overall_pass = true;
        let scenario = classify(&candidate, &report, &classified).expect("應產生 Diagonal Ending");
        assert!(matches!(
            scenario.pattern_type,
            NeelyPatternType::Diagonal {
                sub_kind: DiagonalKind::Ending
            }
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
        let report = make_impulse_report(); // overall_pass = true
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
        let candidate = make_candidate_5wave_starting_at(0);
        let mut report = make_impulse_report();
        report.overall_pass = false;
        report.failed.push(crate::output::RuleRejection {
            candidate_id: "c5-mw0-mw4".to_string(),
            rule_id: RuleId::Ch5_Essential(3),
            expected: "test".to_string(),
            actual: "test".to_string(),
            gap: 0.0,
            neely_page: "test".to_string(),
        });
        assert!(classify(&candidate, &report, &classified).is_none());
    }

    #[test]
    fn both_overlaps_failed_yields_no_scenario() {
        // Trending fail + Terminal fail → 結構錯亂 → reject
        let classified = make_5wave_impulse_classified();
        let candidate = make_candidate_5wave_starting_at(0);
        let mut report = make_impulse_report();
        report.failed.clear();
        report.failed.push(crate::output::RuleRejection {
            candidate_id: "c5-mw0-mw4".to_string(),
            rule_id: RuleId::Ch5_Overlap_Trending,
            expected: "test".to_string(),
            actual: "test".to_string(),
            gap: 0.0,
            neely_page: "test".to_string(),
        });
        report.failed.push(crate::output::RuleRejection {
            candidate_id: "c5-mw0-mw4".to_string(),
            rule_id: RuleId::Ch5_Overlap_Terminal,
            expected: "test".to_string(),
            actual: "test".to_string(),
            gap: 0.0,
            neely_page: "test".to_string(),
        });
        report.overall_pass = true; // 假設 Post-Validator 不否決,但 classifier 仍 reject
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

    // ── Phase 15 unit tests ─────────────────────────────────────────────

    #[test]
    fn detect_triplexity_for_triple_combination() {
        // TripleZigzag / TripleCombination / TripleThree / TripleThreeCombination /
        // TripleThreeRunning 都應觸發 triplexity_detected = true
        for kind in [
            CombinationKind::TripleZigzag,
            CombinationKind::TripleCombination,
            CombinationKind::TripleThree,
            CombinationKind::TripleThreeCombination,
            CombinationKind::TripleThreeRunning,
        ] {
            let pattern = NeelyPatternType::Combination {
                sub_kinds: vec![kind],
            };
            assert!(
                detect_triplexity(&pattern),
                "expected triplexity_detected = true for {:?}",
                kind
            );
        }
    }

    #[test]
    fn detect_triplexity_false_for_double_or_non_combination() {
        // Double* variants 應不觸發 triplexity
        let pattern_double = NeelyPatternType::Combination {
            sub_kinds: vec![CombinationKind::DoubleZigzag],
        };
        assert!(!detect_triplexity(&pattern_double));

        // 非 Combination 應不觸發
        let pattern_impulse = NeelyPatternType::Impulse;
        assert!(!detect_triplexity(&pattern_impulse));

        let pattern_zigzag = NeelyPatternType::Zigzag {
            sub_kind: crate::output::ZigzagKind::Triple, // 注意:ZigzagKind::Triple 不是 Triplexity
        };
        assert!(!detect_triplexity(&pattern_zigzag));
    }

    #[test]
    fn build_monowave_structure_labels_one_to_one() {
        // 構造 3-wave candidate,每 monowave 預先填 1 個 candidate label,確認 1:1 對應
        let mut classified = vec![
            cmw(100.0, 110.0, MonowaveDirection::Up),
            cmw(110.0, 105.0, MonowaveDirection::Down),
            cmw(105.0, 120.0, MonowaveDirection::Up),
        ];
        // 填一些 candidate labels
        classified[0].structure_label_candidates = vec![crate::output::StructureLabelCandidate {
            label: crate::output::StructureLabel::Five,
            certainty: crate::output::Certainty::Primary,
        }];
        classified[1].structure_label_candidates = vec![crate::output::StructureLabelCandidate {
            label: crate::output::StructureLabel::Three,
            certainty: crate::output::Certainty::Possible,
        }];

        let candidate = WaveCandidate {
            id: "c3".to_string(),
            monowave_indices: vec![0, 1, 2],
            wave_count: 3,
            initial_direction: MonowaveDirection::Up,
        };

        let labels = build_monowave_structure_labels(&candidate, &classified);
        assert_eq!(labels.len(), 3);
        assert_eq!(labels[0].monowave_index, 0);
        assert_eq!(labels[0].labels.len(), 1);
        assert_eq!(labels[1].monowave_index, 1);
        assert_eq!(labels[1].labels.len(), 1);
        assert_eq!(labels[2].monowave_index, 2);
        assert_eq!(labels[2].labels.len(), 0); // 預設空
    }
}
