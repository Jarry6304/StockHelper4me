// classifier — Stage 5:Pattern Classifier(PR-4b 完整 sub_kind 識別)
//
// 對齊 m3Spec/neely_core_architecture.md r5 §9.6 + neely_rules.md Ch5/Ch11。
//
// 給通過 Validator 的 candidate 命名 pattern_type + sub_kind:
//   - 5-wave + R3 pass + 非 Triangle → Impulse(+ ImpulseExtension sub_kind)
//   - 5-wave + R3 fail → TerminalImpulse
//   - 5-wave + Triangle 規則 pass → Triangle{TriangleVariant}
//   - 3-wave Zigzag-like(b/a ≤ 65.8%)→ Zigzag{ZigzagVariant}
//   - 3-wave Flat-like(b/a ≥ 57.8%)→ Flat{FlatVariant}
//
// **PR-4b 階段(2026-05-13)**:
//   - 用 validator 的 pass/not_applicable results + monowave magnitudes 決定 sub_kind
//   - 對齊 neely_rules.md Ch5 + Ch11 sub-variant 判定條件
//   - Impulse Extension(1st/3rd/5th/Non/FifthFailure)by longest wave 比較
//   - Flat 7 變體(Common/BFailure/CFailure/Irregular/IrregularFailure/Elongated/DoubleFailure)
//     by b/a + c/b ratios
//   - Zigzag 3 變體(Normal/Truncated/Elongated)by c/a ratio
//   - Triangle 9 變體(Limiting × 3 / Non-Limiting × 3 / Expanding × 3)
//     由 b/a + leg contraction patterns 推
//   - RunningCorrection top-level variant(b > 138.2% × a,r5 §9.6 獨立 variant)

use crate::candidates::WaveCandidate;
use crate::output::{
    ComplexityLevel, FibZone, FlatVariant, ImpulseExtension, NeelyPatternType,
    PostBehavior, PowerRating, RuleId, Scenario, StructuralFacts, Trigger,
    TriangleVariant, WaveNode, WaveNumber, ZigzagVariant,
};
use crate::monowave::ClassifiedMonowave;
use crate::validator::ValidationReport;

/// Flat b-wave 邊界 (Ch5 p.5-34~36)
const FLAT_B_MIN_PCT: f64 = 57.8;       // 61.8% - 4% 容差
#[allow(dead_code)]
const FLAT_B_NORMAL_MAX_PCT: f64 = 104.0; // Common Flat 上限(PR-4b 內部判定用,classify_flat_variant 內 inline)
const FLAT_B_IRREGULAR_MAX_PCT: f64 = 142.2; // Irregular 上限 138.2%+4%
// > 142.2% → RunningCorrection

/// Flat c-wave 邊界
const FLAT_C_FAILURE_MAX_PCT: f64 = 100.0;
const FLAT_C_ELONGATED_MIN_PCT: f64 = 138.2;

/// Zigzag c-wave 邊界(Ch5 p.5-41~42 / Ch11 p.11-17)
const ZIGZAG_C_TRUNCATED_MAX_PCT: f64 = 61.8;
const ZIGZAG_C_NORMAL_MAX_PCT: f64 = 161.8;

/// Triangle b/a 邊界(Ch5 line 1552-1554):
/// Horizontal: b ≤ 100% × a;Irregular: 100% < b < 261.8%;Running: b > a 為三角中最長段
const TRI_B_HORIZONTAL_MAX_PCT: f64 = 104.0; // 100% + 4% 容差
const TRI_B_IRREGULAR_MAX_PCT: f64 = 265.8; // 261.8% + 4%

/// Impulse Extension 判定:longest wave / next-longest ≥ 1.1(§4.2 ±10%)
const W1_EXTENSION_RATIO: f64 = 1.1;

/// Stage 5 結果:Classifier 給 candidate 命名 pattern + sub_kind + 組裝成 Scenario。
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
        5 => classify_5wave(candidate, report, classified),
        3 => classify_3wave(candidate, classified),
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
        power_rating: PowerRating::Neutral,  // Stage 10a 補
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
            awaiting_l_label: false,
    })
}

// ---------------------------------------------------------------------------
// 5-wave classifier:Impulse / TerminalImpulse / Triangle
// ---------------------------------------------------------------------------

fn classify_5wave(
    candidate: &WaveCandidate,
    report: &ValidationReport,
    classified: &[ClassifiedMonowave],
) -> NeelyPatternType {
    let mi = &candidate.monowave_indices;
    let r3_failed = report.failed.iter().any(|r| r.rule_id == RuleId::Ch5Essential(3));

    // 先檢查是否符合 Triangle(T4 Pass + T5 / T6 Pass)
    let triangle_b_passed = !report.failed.iter().any(|r| r.rule_id == RuleId::Ch5TriangleBRange)
        && triangle_b_in_range(mi, classified);
    let triangle_leg_contracting = triangle_legs_contracting(mi, classified);
    let triangle_leg_equality = triangle_legs_equal(mi, classified);

    if triangle_b_passed && (triangle_leg_contracting || triangle_leg_equality) {
        return NeelyPatternType::Triangle {
            sub_kind: classify_triangle_variant(mi, classified, triangle_leg_contracting),
        };
    }

    // 非 Triangle:Impulse vs TerminalImpulse 判定
    if r3_failed {
        // R3 fail = W4 重疊 W1 → TerminalImpulse(Neely 派)
        NeelyPatternType::TerminalImpulse
    } else {
        // R3 pass = strict Impulse;sub_kind 留 PR-6b classifier(Ch11 wave-by-wave)
        // 注意:r5 spec NeelyPatternType::Impulse 是 unit variant,
        // ImpulseExtension 透過 Ch11ImpulseWaveByWave RuleId 表達(已在 validator)
        let _ext = classify_impulse_extension(mi, classified);
        NeelyPatternType::Impulse
    }
}

fn classify_impulse_extension(
    mi: &[usize],
    classified: &[ClassifiedMonowave],
) -> ImpulseExtension {
    if mi.len() < 5 {
        return ImpulseExtension::NonExt;
    }
    let w1 = classified[mi[0]].metrics.magnitude;
    let w3 = classified[mi[2]].metrics.magnitude;
    let w5 = classified[mi[4]].metrics.magnitude;

    if w1 <= 0.0 || w3 <= 0.0 {
        return ImpulseExtension::NonExt;
    }

    // 找最長 actionable wave(1/3/5)
    let max_mag = w1.max(w3).max(w5);
    // 找次長
    let mut sorted = [w1, w3, w5];
    sorted.sort_by(|a, b| b.partial_cmp(a).unwrap());
    let next_mag = sorted[1];

    // 判 5th Failure(wave-5 < wave-4)在 Stage 5 沒有 wave-4 magnitude vs wave-5 比較
    // 簡化:wave-5 顯著短於 wave-3 + wave-1(< 38.2% W1)→ FifthFailure
    if w5 < w1 * 0.382 && w3 > w1 * W1_EXTENSION_RATIO {
        return ImpulseExtension::FifthFailure;
    }

    // 判 Extension:max > next × 1.1
    if max_mag > next_mag * W1_EXTENSION_RATIO {
        if (max_mag - w1).abs() < 1e-9 {
            return ImpulseExtension::FirstExt;
        }
        if (max_mag - w3).abs() < 1e-9 {
            return ImpulseExtension::ThirdExt;
        }
        if (max_mag - w5).abs() < 1e-9 {
            return ImpulseExtension::FifthExt;
        }
    }

    ImpulseExtension::NonExt
}

fn triangle_b_in_range(mi: &[usize], classified: &[ClassifiedMonowave]) -> bool {
    if mi.len() < 5 {
        return false;
    }
    let a = classified[mi[0]].metrics.magnitude;
    let b = classified[mi[1]].metrics.magnitude;
    if a <= 0.0 {
        return false;
    }
    let pct = b / a * 100.0;
    (34.2..=265.8).contains(&pct)
}

fn triangle_legs_contracting(mi: &[usize], classified: &[ClassifiedMonowave]) -> bool {
    if mi.len() < 5 {
        return false;
    }
    let c = classified[mi[2]].metrics.magnitude;
    let d = classified[mi[3]].metrics.magnitude;
    let e = classified[mi[4]].metrics.magnitude;
    e < d && d < c
}

fn triangle_legs_expanding(mi: &[usize], classified: &[ClassifiedMonowave]) -> bool {
    if mi.len() < 5 {
        return false;
    }
    let a = classified[mi[0]].metrics.magnitude;
    let e = classified[mi[4]].metrics.magnitude;
    e >= a
}

fn triangle_legs_equal(mi: &[usize], classified: &[ClassifiedMonowave]) -> bool {
    if mi.len() < 5 {
        return false;
    }
    let a = classified[mi[0]].metrics.magnitude;
    let b = classified[mi[1]].metrics.magnitude;
    let c = classified[mi[2]].metrics.magnitude;
    let d = classified[mi[3]].metrics.magnitude;
    if a <= 0.0 || b <= 0.0 {
        return false;
    }
    ((a - c).abs() / a * 100.0 <= 5.0) || ((b - d).abs() / b * 100.0 <= 5.0)
}

fn classify_triangle_variant(
    mi: &[usize],
    classified: &[ClassifiedMonowave],
    is_contracting: bool,
) -> TriangleVariant {
    // 9 variants:Horizontal/Irregular/Running × Limiting/NonLimiting/Expanding
    // 簡化版:用 b/a 決定 Horizontal/Irregular/Running;用 contracting/expanding 決定 Limiting
    // 完整 apex 時序 / Thrust 判定留 PR-6b 接 actual time / volume data
    let a = classified[mi[0]].metrics.magnitude;
    let b = classified[mi[1]].metrics.magnitude;
    let b_pct = if a > 0.0 { b / a * 100.0 } else { 0.0 };

    if !is_contracting && triangle_legs_expanding(mi, classified) {
        // Expanding 系列
        if b_pct <= TRI_B_HORIZONTAL_MAX_PCT {
            TriangleVariant::HorizontalExpanding
        } else if b_pct <= TRI_B_IRREGULAR_MAX_PCT {
            TriangleVariant::IrregularExpanding
        } else {
            TriangleVariant::RunningExpanding
        }
    } else {
        // Contracting(Limiting 預設;NonLimiting 區分需 apex 時序,留 PR-6b)
        if b_pct <= TRI_B_HORIZONTAL_MAX_PCT {
            TriangleVariant::HorizontalLimiting
        } else if b_pct <= TRI_B_IRREGULAR_MAX_PCT {
            TriangleVariant::IrregularLimiting
        } else {
            TriangleVariant::RunningLimiting
        }
    }
}

// ---------------------------------------------------------------------------
// 3-wave classifier:Zigzag / Flat / RunningCorrection
// ---------------------------------------------------------------------------

fn classify_3wave(candidate: &WaveCandidate, classified: &[ClassifiedMonowave]) -> NeelyPatternType {
    let mi = &candidate.monowave_indices;
    if mi.len() < 3 {
        return NeelyPatternType::Zigzag { sub_kind: ZigzagVariant::Normal };
    }
    let a = classified[mi[0]].metrics.magnitude;
    let b = classified[mi[1]].metrics.magnitude;
    let c = classified[mi[2]].metrics.magnitude;
    if a <= 0.0 {
        return NeelyPatternType::Zigzag { sub_kind: ZigzagVariant::Normal };
    }
    let b_over_a = b / a * 100.0;

    // RunningCorrection:b > 142.2% × a(r5 §9.6 獨立 top-level variant)
    if b_over_a > FLAT_B_IRREGULAR_MAX_PCT {
        return NeelyPatternType::RunningCorrection;
    }

    // Flat:b/a ≥ 57.8%(Z1 NotApplicable 區段)
    if b_over_a >= FLAT_B_MIN_PCT {
        return NeelyPatternType::Flat {
            sub_kind: classify_flat_variant(a, b, c),
        };
    }

    // Zigzag:b/a < 57.8%
    NeelyPatternType::Zigzag {
        sub_kind: classify_zigzag_variant(a, c),
    }
}

fn classify_zigzag_variant(a_mag: f64, c_mag: f64) -> ZigzagVariant {
    // 對齊 Ch11 p.11-17:Truncated 38.2-61.8% / Normal 61.8-161.8% / Elongated > 161.8%
    if a_mag <= 0.0 {
        return ZigzagVariant::Normal;
    }
    let c_pct = c_mag / a_mag * 100.0;
    if c_pct <= ZIGZAG_C_TRUNCATED_MAX_PCT {
        ZigzagVariant::Truncated
    } else if c_pct <= ZIGZAG_C_NORMAL_MAX_PCT {
        ZigzagVariant::Normal
    } else {
        ZigzagVariant::Elongated
    }
}

fn classify_flat_variant(a_mag: f64, b_mag: f64, c_mag: f64) -> FlatVariant {
    // 對齊 Ch5 p.5-34~36 + r5 §9.6 7 named variants:
    //   Common / BFailure / CFailure / Irregular / IrregularFailure / Elongated / DoubleFailure
    if a_mag <= 0.0 || b_mag <= 0.0 {
        return FlatVariant::Common;
    }
    let b_pct = b_mag / a_mag * 100.0;
    let c_over_b_pct = c_mag / b_mag * 100.0;
    let c_over_a_pct = c_mag / a_mag * 100.0;

    let b_normal = b_pct >= 81.0 && b_pct <= 104.0;       // 81-100% (Common range)
    let b_weak = b_pct >= FLAT_B_MIN_PCT && b_pct < 81.0; // 57.8-80% (B-Failure)
    let b_strong = b_pct > 104.0 && b_pct <= FLAT_B_IRREGULAR_MAX_PCT; // 101-138.2% (Irregular)

    // DoubleFailure:c < 100% × b AND b < 81% × a
    if b_weak && c_over_b_pct < FLAT_C_FAILURE_MAX_PCT {
        return FlatVariant::DoubleFailure;
    }

    // BFailure:b 弱 + c 接近 a
    if b_weak {
        return FlatVariant::BFailure;
    }

    // CFailure:b 正常 + c < 100% × b
    if b_normal && c_over_b_pct < FLAT_C_FAILURE_MAX_PCT {
        return FlatVariant::CFailure;
    }

    // Elongated:c > 138.2% × b
    if c_over_b_pct > FLAT_C_ELONGATED_MIN_PCT {
        return FlatVariant::Elongated;
    }

    // Irregular:b > 100% × a
    if b_strong {
        // IrregularFailure 子分類:b strong + c < 100% × b(c 沒完全回測 b)
        if c_over_b_pct < FLAT_C_FAILURE_MAX_PCT {
            return FlatVariant::IrregularFailure;
        }
        // Irregular(完成版):c 回測 b + c ≤ 161.8% × a
        let _ = c_over_a_pct;
        return FlatVariant::Irregular;
    }

    // 預設 Common(b 81-100% + c 100-138.2% × b 範圍)
    FlatVariant::Common
}

// ---------------------------------------------------------------------------
// 共用 helper
// ---------------------------------------------------------------------------

fn classify_complexity(candidate: &WaveCandidate) -> ComplexityLevel {
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

fn default_passed_rules(
    candidate: &WaveCandidate,
    report: &ValidationReport,
) -> Vec<RuleId> {
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

    let _ = candidate;
    passed
}

// suppress unused import warning(WaveNumber is intentionally kept for future PR-6b use)
#[allow(dead_code)]
fn _wave_number_import_anchor() -> WaveNumber {
    WaveNumber::Three
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::candidates::WaveCandidate;
    use crate::monowave::ProportionMetrics;
    use crate::output::{
        CombinationKind, Monowave, MonowaveDirection,
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
            id: "c5".to_string(),
            monowave_indices: vec![0, 1, 2, 3, 4],
            wave_count: 5,
            initial_direction: MonowaveDirection::Up,
        }
    }

    fn make_candidate_3wave() -> WaveCandidate {
        WaveCandidate {
            id: "c3".to_string(),
            monowave_indices: vec![0, 1, 2],
            wave_count: 3,
            initial_direction: MonowaveDirection::Up,
        }
    }

    fn make_passing_report() -> ValidationReport {
        ValidationReport {
            candidate_id: "c5".to_string(),
            passed: vec![],
            failed: vec![],
            deferred: vec![],
            not_applicable: vec![],
            overall_pass: true,
        }
    }

    // ---------- 5-wave classifier ----------

    #[test]
    fn five_wave_r3_pass_classified_as_impulse() {
        let classified = make_5wave_impulse_classified();
        let candidate = make_candidate_5wave();
        let report = make_passing_report();
        let scenario = classify(&candidate, &report, &classified).expect("Scenario");
        assert!(matches!(scenario.pattern_type, NeelyPatternType::Impulse));
    }

    #[test]
    fn five_wave_r3_fail_classified_as_terminal_impulse() {
        let classified = make_5wave_impulse_classified();
        let candidate = make_candidate_5wave();
        let mut report = make_passing_report();
        report.failed.push(crate::output::RuleRejection {
            candidate_id: "c5".to_string(),
            rule_id: RuleId::Ch5Essential(3),
            expected: "test".to_string(),
            actual: "test".to_string(),
            gap: 0.0,
            neely_page: "test".to_string(),
        });
        report.overall_pass = true;
        let scenario = classify(&candidate, &report, &classified).expect("Scenario");
        assert!(matches!(scenario.pattern_type, NeelyPatternType::TerminalImpulse));
    }

    #[test]
    fn five_wave_triangle_classified() {
        // 5-wave Contracting Triangle: a=10 b=8 c=6 d=4 e=2(b<a,leg contracting,a-c gap=4 > 5%)
        // 簡化 leg equality:b/d = 8/4=2x not equal. 用 leg contraction 即可進 Triangle 分支
        let classified = vec![
            cmw(100.0, 110.0, MonowaveDirection::Up),
            cmw(110.0, 102.0, MonowaveDirection::Down),
            cmw(102.0, 108.0, MonowaveDirection::Up),
            cmw(108.0, 104.0, MonowaveDirection::Down),
            cmw(104.0, 106.0, MonowaveDirection::Up),
        ];
        let candidate = make_candidate_5wave();
        let report = make_passing_report();
        let scenario = classify(&candidate, &report, &classified).expect("Scenario");
        assert!(
            matches!(scenario.pattern_type, NeelyPatternType::Triangle { .. }),
            "Contracting Triangle 應分類為 Triangle, got {:?}",
            scenario.pattern_type
        );
    }

    // ---------- Impulse Extension classifier ----------

    #[test]
    fn impulse_extension_third_ext() {
        // W1=10 W3=25 W5=12 → W3 > W1 × 1.1 + W3 最長 → 3rd Ext
        let classified = vec![
            cmw(100.0, 110.0, MonowaveDirection::Up),
            cmw(110.0, 105.0, MonowaveDirection::Down),
            cmw(105.0, 130.0, MonowaveDirection::Up),
            cmw(130.0, 122.0, MonowaveDirection::Down),
            cmw(122.0, 134.0, MonowaveDirection::Up),
        ];
        let mi = &[0, 1, 2, 3, 4][..];
        assert_eq!(classify_impulse_extension(mi, &classified), ImpulseExtension::ThirdExt);
    }

    #[test]
    fn impulse_extension_fifth_ext() {
        // W1=10 W3=11 W5=25 → W5 最長 + 顯著 → 5th Ext
        let classified = vec![
            cmw(100.0, 110.0, MonowaveDirection::Up),
            cmw(110.0, 105.0, MonowaveDirection::Down),
            cmw(105.0, 116.0, MonowaveDirection::Up),
            cmw(116.0, 110.0, MonowaveDirection::Down),
            cmw(110.0, 135.0, MonowaveDirection::Up),
        ];
        let mi = &[0, 1, 2, 3, 4][..];
        assert_eq!(classify_impulse_extension(mi, &classified), ImpulseExtension::FifthExt);
    }

    #[test]
    fn impulse_extension_fifth_failure() {
        // W1=10 W3=20 W5=2(W5 < W1 × 0.382 = 3.82)→ FifthFailure
        let classified = vec![
            cmw(100.0, 110.0, MonowaveDirection::Up),
            cmw(110.0, 105.0, MonowaveDirection::Down),
            cmw(105.0, 125.0, MonowaveDirection::Up),
            cmw(125.0, 120.0, MonowaveDirection::Down),
            cmw(120.0, 122.0, MonowaveDirection::Up),
        ];
        let mi = &[0, 1, 2, 3, 4][..];
        assert_eq!(classify_impulse_extension(mi, &classified), ImpulseExtension::FifthFailure);
    }

    #[test]
    fn impulse_extension_non_ext() {
        // W1=10 W3=11 W5=10 → 無顯著最長 → NonExt
        let classified = vec![
            cmw(100.0, 110.0, MonowaveDirection::Up),
            cmw(110.0, 105.0, MonowaveDirection::Down),
            cmw(105.0, 116.0, MonowaveDirection::Up),
            cmw(116.0, 110.0, MonowaveDirection::Down),
            cmw(110.0, 120.0, MonowaveDirection::Up),
        ];
        let mi = &[0, 1, 2, 3, 4][..];
        assert_eq!(classify_impulse_extension(mi, &classified), ImpulseExtension::NonExt);
    }

    // ---------- 3-wave classifier ----------

    #[test]
    fn three_wave_zigzag_normal() {
        // b/a = 40%(Zigzag), c/a = 100%(Normal range)
        let classified = vec![
            cmw(100.0, 110.0, MonowaveDirection::Up),
            cmw(110.0, 106.0, MonowaveDirection::Down),
            cmw(106.0, 116.0, MonowaveDirection::Up),
        ];
        let candidate = make_candidate_3wave();
        let scenario = classify(&candidate, &make_passing_report(), &classified).expect("Scenario");
        assert!(matches!(
            scenario.pattern_type,
            NeelyPatternType::Zigzag { sub_kind: ZigzagVariant::Normal }
        ));
    }

    #[test]
    fn three_wave_zigzag_truncated() {
        // b/a = 40%(Zigzag), c/a = 50%(Truncated)
        let classified = vec![
            cmw(100.0, 110.0, MonowaveDirection::Up),
            cmw(110.0, 106.0, MonowaveDirection::Down),
            cmw(106.0, 111.0, MonowaveDirection::Up),
        ];
        let candidate = make_candidate_3wave();
        let scenario = classify(&candidate, &make_passing_report(), &classified).expect("Scenario");
        assert!(matches!(
            scenario.pattern_type,
            NeelyPatternType::Zigzag { sub_kind: ZigzagVariant::Truncated }
        ));
    }

    #[test]
    fn three_wave_zigzag_elongated() {
        // b/a = 40%(Zigzag), c/a = 200%(Elongated)
        let classified = vec![
            cmw(100.0, 110.0, MonowaveDirection::Up),
            cmw(110.0, 106.0, MonowaveDirection::Down),
            cmw(106.0, 126.0, MonowaveDirection::Up),
        ];
        let candidate = make_candidate_3wave();
        let scenario = classify(&candidate, &make_passing_report(), &classified).expect("Scenario");
        assert!(matches!(
            scenario.pattern_type,
            NeelyPatternType::Zigzag { sub_kind: ZigzagVariant::Elongated }
        ));
    }

    #[test]
    fn three_wave_flat_common() {
        // b/a = 90%(Common range 81-100%), c/b = 100%(Common range 100-138.2%)
        let classified = vec![
            cmw(100.0, 110.0, MonowaveDirection::Up),
            cmw(110.0, 101.0, MonowaveDirection::Down),
            cmw(101.0, 110.0, MonowaveDirection::Up),
        ];
        let candidate = make_candidate_3wave();
        let scenario = classify(&candidate, &make_passing_report(), &classified).expect("Scenario");
        assert!(matches!(
            scenario.pattern_type,
            NeelyPatternType::Flat { sub_kind: FlatVariant::Common }
        ));
    }

    #[test]
    fn three_wave_flat_b_failure() {
        // b/a = 70%(Weak,B-Failure range 61.8-80%), c/b = 100% → BFailure
        let classified = vec![
            cmw(100.0, 110.0, MonowaveDirection::Up),
            cmw(110.0, 103.0, MonowaveDirection::Down),
            cmw(103.0, 110.0, MonowaveDirection::Up),
        ];
        let candidate = make_candidate_3wave();
        let scenario = classify(&candidate, &make_passing_report(), &classified).expect("Scenario");
        assert!(matches!(
            scenario.pattern_type,
            NeelyPatternType::Flat { sub_kind: FlatVariant::BFailure }
        ));
    }

    #[test]
    fn three_wave_flat_c_failure() {
        // b/a = 90%(Common range), c/b = 70%(< 100% → CFailure)
        let classified = vec![
            cmw(100.0, 110.0, MonowaveDirection::Up),
            cmw(110.0, 101.0, MonowaveDirection::Down),
            cmw(101.0, 107.3, MonowaveDirection::Up),
        ];
        let candidate = make_candidate_3wave();
        let scenario = classify(&candidate, &make_passing_report(), &classified).expect("Scenario");
        assert!(matches!(
            scenario.pattern_type,
            NeelyPatternType::Flat { sub_kind: FlatVariant::CFailure }
        ));
    }

    #[test]
    fn three_wave_flat_irregular() {
        // b/a = 120%(Strong 101-138.2%), c/b = 110% → Irregular
        let classified = vec![
            cmw(100.0, 110.0, MonowaveDirection::Up),
            cmw(110.0, 98.0, MonowaveDirection::Down),
            cmw(98.0, 111.2, MonowaveDirection::Up),
        ];
        let candidate = make_candidate_3wave();
        let scenario = classify(&candidate, &make_passing_report(), &classified).expect("Scenario");
        assert!(matches!(
            scenario.pattern_type,
            NeelyPatternType::Flat { sub_kind: FlatVariant::Irregular }
        ));
    }

    #[test]
    fn three_wave_flat_elongated() {
        // b/a = 90%(Common), c/b = 150% > 138.2% → Elongated
        let classified = vec![
            cmw(100.0, 110.0, MonowaveDirection::Up),
            cmw(110.0, 101.0, MonowaveDirection::Down),
            cmw(101.0, 114.5, MonowaveDirection::Up),
        ];
        let candidate = make_candidate_3wave();
        let scenario = classify(&candidate, &make_passing_report(), &classified).expect("Scenario");
        assert!(matches!(
            scenario.pattern_type,
            NeelyPatternType::Flat { sub_kind: FlatVariant::Elongated }
        ));
    }

    #[test]
    fn three_wave_flat_double_failure() {
        // b/a = 70%(Weak), c/b = 80%(< 100%) → DoubleFailure
        let classified = vec![
            cmw(100.0, 110.0, MonowaveDirection::Up),
            cmw(110.0, 103.0, MonowaveDirection::Down),
            cmw(103.0, 108.6, MonowaveDirection::Up),
        ];
        let candidate = make_candidate_3wave();
        let scenario = classify(&candidate, &make_passing_report(), &classified).expect("Scenario");
        assert!(matches!(
            scenario.pattern_type,
            NeelyPatternType::Flat { sub_kind: FlatVariant::DoubleFailure }
        ));
    }

    #[test]
    fn three_wave_running_correction() {
        // b/a = 150% > 142.2% → RunningCorrection(r5 §9.6 獨立 variant)
        let classified = vec![
            cmw(100.0, 110.0, MonowaveDirection::Up),
            cmw(110.0, 95.0, MonowaveDirection::Down),
            cmw(95.0, 105.0, MonowaveDirection::Up),
        ];
        let candidate = make_candidate_3wave();
        let scenario = classify(&candidate, &make_passing_report(), &classified).expect("Scenario");
        assert!(matches!(scenario.pattern_type, NeelyPatternType::RunningCorrection));
    }

    #[test]
    fn failed_validation_yields_no_scenario() {
        let classified = make_5wave_impulse_classified();
        let candidate = make_candidate_5wave();
        let mut report = make_passing_report();
        report.overall_pass = false;
        assert!(classify(&candidate, &report, &classified).is_none());
    }

    #[test]
    fn enum_exhaustive_smoke() {
        // 觸發 enum exhaustive 編譯期檢查
        let _: FlatVariant = FlatVariant::Common;
        let _: TriangleVariant = TriangleVariant::HorizontalLimiting;
        let _: CombinationKind = CombinationKind::DoubleThree;
        let _: ImpulseExtension = ImpulseExtension::ThirdExt;
        let _: ZigzagVariant = ZigzagVariant::Normal;
    }
}
