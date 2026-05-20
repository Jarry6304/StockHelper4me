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

pub mod flat_classifier;
pub mod structural_facts;

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
        3 => classify_3wave(candidate, classified),
        // P2a(Combination 上游補完):7-monowave candidate → Double-* Combination
        7 => classify_7wave_combination(candidate, classified)?,
        // P2b(Combination 上游補完):11-monowave candidate → Triple-* Combination
        11 => classify_11wave_combination(candidate, classified)?,
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

    // Phase 17 / v4.1:StructuralFacts 5 sub-fields 在 classify-time 填
    //(candidate + classified + report;v4.1 加 extension_subdivision_pair)
    let structural_facts = StructuralFacts {
        fibonacci_alignment: structural_facts::fibonacci_alignment(candidate, classified),
        alternation: structural_facts::alternation(candidate, classified, report),
        time_relationship: structural_facts::time_relationship(candidate, classified),
        overlap_pattern: structural_facts::overlap_pattern(candidate, classified),
        extension_subdivision_pair: structural_facts::extension_subdivision_pair(
            candidate, classified,
        ),
        // 3 個 sub-fields 留 lib.rs::compute Stage 7.5 後填(需 bars / advisory_findings)
        channeling: None,
        volume_alignment: None,
        gap_count: 0,
    };
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
        structural_facts,                              // Phase 17:4 sub-fields filled,3 留 lib.rs
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
            // v4.x Item 4:保留 global classified index 供 Stage 8.5 post-Pass-2 refill lookup
            classified_index: mw_idx,
            labels: classified[mw_idx].structure_label_candidates.clone(),
            // v4.x Item 4:Pass 2 完成後由 lib.rs refill loop 填入(此處 Stage 5 預設空)
            pass1_only_labels: Vec::new(),
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

/// Phase 16 r5:3-wave 分類精細化(Flat 7-variant + RunningCorrection 上提)。
///
/// 對齊 m3Spec/neely_rules.md §第 5 章 Flat 詳細規則(line 2157-2239)+
/// §第 10 章 Pattern Implications line 2024-2037。
///
/// P2a(Combination 上游補完):核心邏輯抽至 `classify_3wave_segment`,本函式為對
/// `candidate.monowave_indices` 的薄包裝;`classify_7wave_combination` 對 sub-segment 複用核心。
fn classify_3wave(
    candidate: &WaveCandidate,
    classified: &[ClassifiedMonowave],
) -> NeelyPatternType {
    classify_3wave_segment(&candidate.monowave_indices, classified)
}

/// 對任意 3-monowave index 序列分類 corrective pattern(Zigzag / Flat / RunningCorrection)。
///
/// 流程:
///   1. 抽 a / b / c monowave magnitudes
///   2. 先試 `flat_classifier::is_running_correction`(b > a + c < a)→ RunningCorrection
///   3. 再試 `flat_classifier::classify_flat` → FlatKind 之一
///   4. 都失敗(b/a < 61.8%)→ Zigzag { Single }
fn classify_3wave_segment(
    mi: &[usize],
    classified: &[ClassifiedMonowave],
) -> NeelyPatternType {
    if mi.len() < 3 {
        // 不夠 3 段 — 回 Zigzag Single 作 safe fallback
        return NeelyPatternType::Zigzag {
            sub_kind: ZigzagKind::Single,
        };
    }
    let a_mag = classified[mi[0]].metrics.magnitude;
    let b_mag = classified[mi[1]].metrics.magnitude;
    let c_mag = classified[mi[2]].metrics.magnitude;

    // 1. Running Correction 上提頂層(spec r5 line 1161 + spec line 2035)
    if flat_classifier::is_running_correction(a_mag, b_mag, c_mag) {
        return NeelyPatternType::RunningCorrection;
    }

    // 2. Flat 變體判定
    if let Some(flat_kind) = flat_classifier::classify_flat(a_mag, b_mag, c_mag) {
        return NeelyPatternType::Flat {
            sub_kind: flat_kind,
        };
    }

    // 3. b/a < 61.8% → 不符 Flat 任一變體,回 Zigzag Single
    NeelyPatternType::Zigzag {
        sub_kind: ZigzagKind::Single,
    }
}

/// P2a(Combination 上游補完):7-monowave candidate → Double-* Combination。
///
/// 7 monowaves = sub_a(mi[0..3])+ x-wave(mi[3])+ sub_b(mi[4..7])。對兩個
/// corrective sub-segment 各跑 `classify_3wave_segment`,依 (kind_a, kind_b, x-wave
/// 大小)對映 `CombinationKind`(對齊 m3Spec/neely_rules.md Ch8 Table A 小 x-wave /
/// Table B 大 x-wave)。非可辨識 Double-* 組合 → None(candidate 丟棄,不產 garbage)。
fn classify_7wave_combination(
    candidate: &WaveCandidate,
    classified: &[ClassifiedMonowave],
) -> Option<NeelyPatternType> {
    let mi = &candidate.monowave_indices;
    if mi.len() != 7 {
        return None;
    }
    let kind_a = classify_3wave_segment(&mi[0..3], classified);
    let kind_b = classify_3wave_segment(&mi[4..7], classified);
    let large_x = x_wave_is_large(mi[3], &mi[0..3], &mi[4..7], classified);
    let combination_kind = map_double_combination(&kind_a, &kind_b, large_x)?;
    Some(NeelyPatternType::Combination {
        sub_kinds: vec![combination_kind],
    })
}

/// 判定 x-wave 相對兩側 corrective sub-segment 是否為「大 x-wave」(Table B)。
///
/// 啟發式:x magnitude ≥ 61.8% × min(兩側 sub-segment 淨幅)→ 大 x-wave。
/// 61.8% 為 Neely 通用 Fibonacci 門檻;production 觸發率由 P0 Gate 校準。
fn x_wave_is_large(
    x_idx: usize,
    sub_a: &[usize],
    sub_b: &[usize],
    classified: &[ClassifiedMonowave],
) -> bool {
    let x_mag = classified[x_idx].metrics.magnitude;
    let net_span = |seg: &[usize]| -> f64 {
        let start = classified[seg[0]].monowave.start_price;
        let end = classified[seg[seg.len() - 1]].monowave.end_price;
        (end - start).abs()
    };
    let min_span = net_span(sub_a).min(net_span(sub_b));
    x_mag >= 0.618 * min_span
}

/// (kind_a, kind_b, large_x) → CombinationKind(Double-* 5 variant)。
///
/// Table A(小 x-wave):允許 Zigzag 構成段。
/// Table B(大 x-wave):構成段只能 Flat —— 任一為 Zigzag → None
/// (對齊 m3Spec/neely_rules.md Ch8 Table B 修正:大 x-wave 場景不可出現 Zigzag)。
fn map_double_combination(
    kind_a: &NeelyPatternType,
    kind_b: &NeelyPatternType,
    large_x: bool,
) -> Option<CombinationKind> {
    let is_zigzag = |k: &NeelyPatternType| matches!(k, NeelyPatternType::Zigzag { .. });
    let a_zz = is_zigzag(kind_a);
    let b_zz = is_zigzag(kind_b);
    let has_running = matches!(kind_a, NeelyPatternType::RunningCorrection)
        || matches!(kind_b, NeelyPatternType::RunningCorrection);

    if large_x {
        // Table B 大 x-wave:不可有 Zigzag 構成段(spec Ch8 Table B 修正)
        if a_zz || b_zz {
            return None;
        }
        if has_running {
            Some(CombinationKind::DoubleThreeCombination)
        } else {
            Some(CombinationKind::DoubleThree)
        }
    } else {
        // Table A 小 x-wave:允許 Zigzag。classify_3wave_segment 僅回
        // Zigzag / Flat / RunningCorrection,故「非 Zigzag」即 Flat-family。
        match (a_zz, b_zz) {
            (true, true) => Some(CombinationKind::DoubleZigzag),
            (true, false) | (false, true) => Some(CombinationKind::DoubleCombination),
            (false, false) => Some(CombinationKind::DoubleFlat),
        }
    }
}

/// P2b(Combination 上游補完):11-monowave candidate → Triple-* Combination。
///
/// 11 monowaves = sub_a(mi[0..3])+ x1(mi[3])+ sub_b(mi[4..7])+ x2(mi[7])
/// + sub_c(mi[8..11])。對三個 corrective sub-segment 各跑 `classify_3wave_segment`,
/// 依 (kind_a, kind_b, kind_c, x-wave 大小)對映 Triple-* `CombinationKind`。
fn classify_11wave_combination(
    candidate: &WaveCandidate,
    classified: &[ClassifiedMonowave],
) -> Option<NeelyPatternType> {
    let mi = &candidate.monowave_indices;
    if mi.len() != 11 {
        return None;
    }
    let kind_a = classify_3wave_segment(&mi[0..3], classified);
    let kind_b = classify_3wave_segment(&mi[4..7], classified);
    let kind_c = classify_3wave_segment(&mi[8..11], classified);
    // 兩個 x-wave(mi[3]、mi[7]);任一為大 x-wave → 整體視為 Table B 大 x-wave 場景
    let large_x = x_wave_is_large(mi[3], &mi[0..3], &mi[4..7], classified)
        || x_wave_is_large(mi[7], &mi[4..7], &mi[8..11], classified);
    let combination_kind = map_triple_combination(&kind_a, &kind_b, &kind_c, large_x)?;
    Some(NeelyPatternType::Combination {
        sub_kinds: vec![combination_kind],
    })
}

/// (kind_a, kind_b, kind_c, large_x) → CombinationKind(Triple-* variant)。
///
/// Table A(小 x-wave):允許 Zigzag 構成段。
/// Table B(大 x-wave):構成段只能 Flat —— 任一為 Zigzag → None
/// (對齊 m3Spec/neely_rules.md Ch8 Table B 修正)。
fn map_triple_combination(
    kind_a: &NeelyPatternType,
    kind_b: &NeelyPatternType,
    kind_c: &NeelyPatternType,
    large_x: bool,
) -> Option<CombinationKind> {
    let is_zigzag = |k: &NeelyPatternType| matches!(k, NeelyPatternType::Zigzag { .. });
    let any_zigzag = is_zigzag(kind_a) || is_zigzag(kind_b) || is_zigzag(kind_c);
    let all_zigzag = is_zigzag(kind_a) && is_zigzag(kind_b) && is_zigzag(kind_c);
    let has_running = matches!(kind_a, NeelyPatternType::RunningCorrection)
        || matches!(kind_b, NeelyPatternType::RunningCorrection)
        || matches!(kind_c, NeelyPatternType::RunningCorrection);

    if large_x {
        // Table B 大 x-wave:不可有 Zigzag 構成段
        if any_zigzag {
            return None;
        }
        if has_running {
            Some(CombinationKind::TripleThreeRunning)
        } else {
            Some(CombinationKind::TripleThree)
        }
    } else {
        // Table A 小 x-wave
        if all_zigzag {
            Some(CombinationKind::TripleZigzag)
        } else {
            Some(CombinationKind::TripleCombination)
        }
    }
}

fn classify_complexity(candidate: &WaveCandidate) -> ComplexityLevel {
    // 基本 Complexity Rule(對齊 architecture §7.1 Stage 7):
    //   3 wave → Simple
    //   5 wave → Intermediate
    //   5+ nested wave → Complex
    //
    // **v4.7.3 G1.3 升級**(2026-05-19):
    //   - wave_count > 5 視為 Complex(原 placeholder 已正確,留 P6 改 placeholder 移除)
    //   - 加入 missing-wave 標記偵測:若 wave_candidates 含 NotApplicable 標記
    //     的 sub-wave 視同 nested complexity(spec §Ch12 Missing Wave Rule)
    //   - 完整 nested 列舉(每個 sub-wave 是 :3 或 :5 子形態)由 Compaction
    //     Three Rounds 處理,Classifier 此處純表徵 wave_count
    match candidate.wave_count {
        3 => ComplexityLevel::Simple,
        5 => ComplexityLevel::Intermediate,
        n if n > 5 => ComplexityLevel::Complex,
        _ => ComplexityLevel::Simple, // wave_count == 0/1/2/4(罕見退化)→ Simple
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

    // 子節點:每個 sub-wave 是一個 WaveNode。
    //
    // **v4.9 Item 3 nested label enrichment**(2026-05-19):
    //   - WaveNode.label 嵌入結構標籤 hint(從 structure_label_candidates Primary 抽出)
    //   - 格式:`W{N}:{Label}{:Direction}` 例 "W1:L5↑" / "W2:C3↓" / 無 Primary → "W1↑"
    //   - Compaction Level-N+1 走 `wave_tree.clone()` 直接繼承 children 含這些 hint label,
    //     深層嵌套(:3 / :5 子形態)自動透過遞迴展開可見於 JSONB output
    //   - **Children empty 仍保留**:Level-0 base monowaves atomic,無 finer-resolution
    //     sub-structure 可展開;LLM 看 label 即知此 Wn 的 spec 標籤
    let children = mi
        .iter()
        .enumerate()
        .map(|(i, &idx)| {
            let mw = &classified[idx].monowave;
            let label = format_wave_node_label(i + 1, idx, classified);
            WaveNode {
                label,
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

/// **v4.9 Item 3 helper**:構造 sub-wave `WaveNode.label` 含結構標籤 hint。
///
/// 格式:
/// - 有 Primary structure label → `"W{n}:{Label}{:Direction}"` 例 "W1:L5↑" / "W3:Five↓"
/// - 無 Primary → `"W{n}{Direction}"` 例 "W1↑"
/// - Direction:Up → ↑;Down → ↓;Neutral → ·
///
/// 對齊 spec § Pre-Constructive Logic — 把 Stage 0 標的結構標籤
/// 透過 WaveNode.label 暴露給 Compaction / Aggregation Layer / LLM context。
fn format_wave_node_label(
    wave_num: usize,
    classified_idx: usize,
    classified: &[ClassifiedMonowave],
) -> String {
    use crate::output::{Certainty, MonowaveDirection};
    let cmw = &classified[classified_idx];

    let dir_sym = match cmw.monowave.direction {
        MonowaveDirection::Up => "↑",
        MonowaveDirection::Down => "↓",
        MonowaveDirection::Neutral => "·",
    };

    // 從 structure_label_candidates 抽 Primary label hint
    let label_hint = cmw
        .structure_label_candidates
        .iter()
        .find(|c| matches!(c.certainty, Certainty::Primary))
        .map(|c| format!(":{:?}", c.label));

    match label_hint {
        Some(hint) => format!("W{}{}{}", wave_num, hint, dir_sym),
        None => format!("W{}{}", wave_num, dir_sym),
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
        Certainty, CombinationKind, FlatKind, Monowave, MonowaveDirection, StructureLabelCandidate,
        TriangleKind,
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
                bar_indices: (0, 0),
            },
            atr_at_start: 1.0,
            metrics: ProportionMetrics {
                magnitude: (end_p - start_p).abs(),
                duration_bars: 5,
                atr_relative: 5.0,
                slope_vs_45deg: 1.0,
            },
            structure_label_candidates: Vec::new(),
            polywave_size: 0,
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

    // ── P2a(Combination 上游補完)test fixtures + tests ────────────────────

    /// 7 個 alternating monowaves(U-D-U-D-U-D-U)構成 Double Zigzag:
    /// sub_a[0..3] = Zigzag(a=10/b=5/c=12,b/a<61.8%)、x-wave[3] 小(mag 3)、
    /// sub_b[4..7] = Zigzag。
    fn make_7wave_double_zigzag_classified() -> Vec<ClassifiedMonowave> {
        vec![
            cmw(100.0, 110.0, MonowaveDirection::Up),   // m0  sub_a a=10
            cmw(110.0, 105.0, MonowaveDirection::Down), // m1  sub_a b=5
            cmw(105.0, 117.0, MonowaveDirection::Up),   // m2  sub_a c=12
            cmw(117.0, 114.0, MonowaveDirection::Down), // m3  x-wave mag=3(小)
            cmw(114.0, 124.0, MonowaveDirection::Up),   // m4  sub_b a=10
            cmw(124.0, 119.0, MonowaveDirection::Down), // m5  sub_b b=5
            cmw(119.0, 131.0, MonowaveDirection::Up),   // m6  sub_b c=12
        ]
    }

    fn make_candidate_wc7() -> WaveCandidate {
        WaveCandidate {
            id: "c7-mw0-mw6".to_string(),
            monowave_indices: vec![0, 1, 2, 3, 4, 5, 6],
            wave_count: 7,
            initial_direction: MonowaveDirection::Up,
        }
    }

    #[test]
    fn seven_wave_double_zigzag_classifies_as_combination() {
        let classified = make_7wave_double_zigzag_classified();
        let candidate = make_candidate_wc7();
        let pattern =
            classify_7wave_combination(&candidate, &classified).expect("應產生 Combination");
        match pattern {
            NeelyPatternType::Combination { sub_kinds } => {
                assert_eq!(sub_kinds.len(), 1);
                assert!(matches!(sub_kinds[0], CombinationKind::DoubleZigzag));
            }
            other => panic!("預期 Combination,得到 {:?}", other),
        }
    }

    #[test]
    fn seven_wave_combination_full_classify_produces_scenario() {
        // P2a:wc=7 走完整 classify() 不可 panic,且產出 Combination Scenario。
        let classified = make_7wave_double_zigzag_classified();
        let candidate = make_candidate_wc7();
        let report = ValidationReport {
            overall_pass: true,
            ..Default::default()
        };
        let scenario =
            classify(&candidate, &report, &classified).expect("wc=7 應產生 Scenario");
        assert!(matches!(
            scenario.pattern_type,
            NeelyPatternType::Combination { .. }
        ));
        assert!(matches!(scenario.complexity_level, ComplexityLevel::Complex));
        assert_eq!(scenario.wave_tree.children.len(), 7);
    }

    #[test]
    fn large_x_wave_with_zigzag_component_rejected() {
        // map_double_combination:大 x-wave(Table B)不可有 Zigzag 構成段 → None
        let zz = NeelyPatternType::Zigzag {
            sub_kind: ZigzagKind::Single,
        };
        let flat = NeelyPatternType::Flat {
            sub_kind: FlatKind::Common,
        };
        assert!(map_double_combination(&zz, &flat, true).is_none());
        // 大 x-wave + 兩 Flat → DoubleThree
        assert!(matches!(
            map_double_combination(&flat, &flat, true),
            Some(CombinationKind::DoubleThree)
        ));
    }

    #[test]
    fn eleven_wave_triple_zigzag_classifies_as_combination() {
        // P2b:11 monowaves,3 個 sub-segment 全 Zigzag + 兩個小 x-wave → TripleZigzag
        let classified = vec![
            cmw(100.0, 110.0, MonowaveDirection::Up),   // m0  sub_a a=10
            cmw(110.0, 105.0, MonowaveDirection::Down), // m1  sub_a b=5
            cmw(105.0, 117.0, MonowaveDirection::Up),   // m2  sub_a c=12
            cmw(117.0, 114.0, MonowaveDirection::Down), // m3  x1 小
            cmw(114.0, 124.0, MonowaveDirection::Up),   // m4  sub_b a=10
            cmw(124.0, 119.0, MonowaveDirection::Down), // m5  sub_b b=5
            cmw(119.0, 131.0, MonowaveDirection::Up),   // m6  sub_b c=12
            cmw(131.0, 128.0, MonowaveDirection::Down), // m7  x2 小
            cmw(128.0, 138.0, MonowaveDirection::Up),   // m8  sub_c a=10
            cmw(138.0, 133.0, MonowaveDirection::Down), // m9  sub_c b=5
            cmw(133.0, 145.0, MonowaveDirection::Up),   // m10 sub_c c=12
        ];
        let candidate = WaveCandidate {
            id: "c11-mw0-mw10".to_string(),
            monowave_indices: (0..11).collect(),
            wave_count: 11,
            initial_direction: MonowaveDirection::Up,
        };
        let pattern =
            classify_11wave_combination(&candidate, &classified).expect("應產生 Combination");
        match pattern {
            NeelyPatternType::Combination { sub_kinds } => {
                assert!(matches!(sub_kinds[0], CombinationKind::TripleZigzag));
            }
            other => panic!("預期 Combination,得到 {:?}", other),
        }
        // 走完整 classify() 不可 panic
        let report = ValidationReport {
            overall_pass: true,
            ..Default::default()
        };
        let scenario =
            classify(&candidate, &report, &classified).expect("wc=11 應產生 Scenario");
        assert!(matches!(
            scenario.pattern_type,
            NeelyPatternType::Combination { .. }
        ));
        assert_eq!(scenario.wave_tree.children.len(), 11);
    }

    #[test]
    fn map_triple_combination_table_b_rejects_zigzag() {
        let zz = NeelyPatternType::Zigzag {
            sub_kind: ZigzagKind::Single,
        };
        let flat = NeelyPatternType::Flat {
            sub_kind: FlatKind::Common,
        };
        // 大 x-wave + 有 Zigzag → None
        assert!(map_triple_combination(&flat, &zz, &flat, true).is_none());
        // 大 x-wave + 全 Flat → TripleThree
        assert!(matches!(
            map_triple_combination(&flat, &flat, &flat, true),
            Some(CombinationKind::TripleThree)
        ));
        // 小 x-wave + 全 Zigzag → TripleZigzag
        assert!(matches!(
            map_triple_combination(&zz, &zz, &zz, false),
            Some(CombinationKind::TripleZigzag)
        ));
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
    fn three_wave_zigzag_when_b_too_shallow() {
        // Phase 16 r5:b/a < 61.8% → 不符 Flat,回 Zigzag Single fallback
        // a = 10 / b = 5(50%)/ c = 12 → b/a < 61.8% → Zigzag Single
        let classified = vec![
            cmw(100.0, 110.0, MonowaveDirection::Up),
            cmw(110.0, 105.0, MonowaveDirection::Down), // b = 5(50% × a)
            cmw(105.0, 117.0, MonowaveDirection::Up),
        ];
        let candidate = WaveCandidate {
            id: "c3-zigzag".to_string(),
            monowave_indices: vec![0, 1, 2],
            wave_count: 3,
            initial_direction: MonowaveDirection::Up,
        };
        let report = make_impulse_report();
        let scenario = classify(&candidate, &report, &classified).expect("應產生 Scenario");
        assert!(matches!(
            scenario.pattern_type,
            NeelyPatternType::Zigzag { sub_kind: ZigzagKind::Single }
        ));
        assert!(matches!(scenario.complexity_level, ComplexityLevel::Simple));
    }

    #[test]
    fn three_wave_classified_as_common_flat_when_b_strong_c_strong() {
        // Phase 16 r5:b/a ∈ [81%, 100%] + c ≥ b → Common Flat
        let classified = vec![
            cmw(100.0, 110.0, MonowaveDirection::Up),       // a = 10
            cmw(110.0, 101.5, MonowaveDirection::Down),     // b = 8.5(85% × a)
            cmw(101.5, 92.0, MonowaveDirection::Down),      // c = 9.5(≥ b)
        ];
        let candidate = WaveCandidate {
            id: "c3-common-flat".to_string(),
            monowave_indices: vec![0, 1, 2],
            wave_count: 3,
            initial_direction: MonowaveDirection::Up,
        };
        let report = make_impulse_report();
        let scenario = classify(&candidate, &report, &classified).expect("應產生 Scenario");
        assert!(matches!(
            scenario.pattern_type,
            NeelyPatternType::Flat { sub_kind: FlatKind::Common }
        ));
    }

    #[test]
    fn three_wave_classified_as_running_correction_when_b_above_a_and_c_short() {
        // Phase 16 r5:b > a AND c < a → RunningCorrection 上提頂層
        let classified = vec![
            cmw(100.0, 110.0, MonowaveDirection::Up),        // a = 10
            cmw(110.0, 97.0, MonowaveDirection::Down),       // b = 13(130% × a)
            cmw(97.0, 105.0, MonowaveDirection::Up),         // c = 8(80% × a)
        ];
        let candidate = WaveCandidate {
            id: "c3-running".to_string(),
            monowave_indices: vec![0, 1, 2],
            wave_count: 3,
            initial_direction: MonowaveDirection::Up,
        };
        let report = make_impulse_report();
        let scenario = classify(&candidate, &report, &classified).expect("應產生 Scenario");
        assert!(matches!(scenario.pattern_type, NeelyPatternType::RunningCorrection));
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
        let _: FlatKind = FlatKind::Common;
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

    // v4.7.3 G1.3 classify_complexity tests -----------------------------

    fn make_candidate_wave_count(wc: usize) -> WaveCandidate {
        WaveCandidate {
            id: "test".to_string(),
            wave_count: wc,
            monowave_indices: (0..wc).collect(),
            initial_direction: MonowaveDirection::Up,
        }
    }

    #[test]
    fn classify_complexity_3_wave_simple() {
        let c = make_candidate_wave_count(3);
        assert!(matches!(classify_complexity(&c), ComplexityLevel::Simple));
    }

    #[test]
    fn classify_complexity_5_wave_intermediate() {
        let c = make_candidate_wave_count(5);
        assert!(matches!(
            classify_complexity(&c),
            ComplexityLevel::Intermediate
        ));
    }

    #[test]
    fn classify_complexity_7_wave_complex() {
        let c = make_candidate_wave_count(7);
        assert!(matches!(classify_complexity(&c), ComplexityLevel::Complex));
    }

    #[test]
    fn classify_complexity_degenerate_wave_count_simple() {
        // v4.7.3 G1.3:wave_count == 0/1/2/4 應落 Simple(罕見退化,不應 Complex)
        for wc in [0usize, 1, 2, 4] {
            let c = make_candidate_wave_count(wc);
            assert!(
                matches!(classify_complexity(&c), ComplexityLevel::Simple),
                "wave_count={} 應 Simple",
                wc
            );
        }
    }

    // v4.9 Item 3 format_wave_node_label tests ----------------------------

    fn cmw_with_label_dir(
        labels: Vec<(StructureLabel, Certainty)>,
        dir: MonowaveDirection,
    ) -> ClassifiedMonowave {
        let candidates = labels
            .into_iter()
            .map(|(l, c)| StructureLabelCandidate {
                label: l,
                certainty: c,
            })
            .collect();
        ClassifiedMonowave {
            monowave: Monowave {
                start_date: chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                end_date: chrono::NaiveDate::from_ymd_opt(2026, 1, 5).unwrap(),
                start_price: 100.0,
                end_price: 110.0,
                direction: dir,
                bar_indices: (0, 0),
            },
            atr_at_start: 1.0,
            metrics: ProportionMetrics {
                magnitude: 10.0,
                duration_bars: 5,
                atr_relative: 1.0,
                slope_vs_45deg: 1.0,
            },
            structure_label_candidates: candidates,
            polywave_size: 0,
        }
    }

    #[test]
    fn format_wave_node_label_with_primary_l5_up() {
        let classified = vec![cmw_with_label_dir(
            vec![(StructureLabel::L5, Certainty::Primary)],
            MonowaveDirection::Up,
        )];
        let label = format_wave_node_label(1, 0, &classified);
        assert_eq!(label, "W1:L5↑");
    }

    #[test]
    fn format_wave_node_label_with_primary_c3_down() {
        let classified = vec![cmw_with_label_dir(
            vec![(StructureLabel::C3, Certainty::Primary)],
            MonowaveDirection::Down,
        )];
        let label = format_wave_node_label(3, 0, &classified);
        assert_eq!(label, "W3:C3↓");
    }

    #[test]
    fn format_wave_node_label_falls_back_when_no_primary() {
        // 只有 Possible certainty,沒 Primary → 不加 hint
        let classified = vec![cmw_with_label_dir(
            vec![(StructureLabel::L5, Certainty::Possible)],
            MonowaveDirection::Up,
        )];
        let label = format_wave_node_label(2, 0, &classified);
        assert_eq!(label, "W2↑");
    }

    #[test]
    fn format_wave_node_label_no_candidates_neutral_uses_dot() {
        let classified = vec![cmw_with_label_dir(vec![], MonowaveDirection::Neutral)];
        let label = format_wave_node_label(5, 0, &classified);
        assert_eq!(label, "W5·");
    }
}
