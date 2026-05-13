// wave_rules.rs — Validator Ch5 Equality + Alternation 規則
//
// 對齊 m3Spec/neely_rules.md §Conditional Construction Rules(1309-1325 行)
//       + m3Spec/neely_core_architecture.md §9.3
//
// **Ch5_Equality**(spec 1321-1325 行):
//   對「非延伸的兩個」(在 W1/W3/W5 中)而言,二者傾向價/時相等,或以 Fibonacci 比關係。
//   - 價格的重要性高於時間
//   - 在 3rd Extension 時最有用;1st Extension 與 Terminal 時最弱
//   實作:5-wave candidate 中找出 Extension(最長 wave),剩下兩條傾向等價(±10% 或 ~61.8%)
//
// **Ch5_Alternation { Construction }**(spec 1311-1319 行):
//   比較同級相鄰或相間波(W2/W4),Construction 軸 alternation:
//   一個是 Flat、另一個是 Zigzag(或 Triangle 等不同 Construction 類型)
//   實作:檢查 W2 與 W4 的 structure_label_candidates 是否含不同 :3 構型標記
//
// 兩條規則對 wave_count == 5 適用;wave_count == 3 走 NotApplicable

use super::RuleResult;
use crate::candidates::WaveCandidate;
use crate::monowave::ClassifiedMonowave;
use crate::output::{AlternationAxis, RuleId, RuleRejection, StructureLabel};

/// 一般近似容差 ±10%(architecture §4.2 第 1 檔)
const APPROX_TOL: f64 = 0.10;
/// Fibonacci 61.8% 容差 ±4%
const FIB_618: f64 = 0.618;
const FIB_TOL: f64 = 0.04;

pub fn run(
    candidate: &WaveCandidate,
    classified: &[ClassifiedMonowave],
) -> Vec<RuleResult> {
    vec![
        rule_equality(candidate, classified),
        rule_alternation_construction(candidate, classified),
    ]
}

/// Ch5_Equality:5-wave 中非延伸的兩條傾向等價或 Fibonacci 61.8% 關係。
///
/// 演算法:
///   1. 找 W1/W3/W5 中 magnitude 最長的為 Extension wave
///   2. 剩下兩條 (non_ext_a, non_ext_b)
///   3. 檢查 |non_ext_a / non_ext_b - 1.0| ≤ 0.10(等價)OR ≈ 0.618(Fib 61.8%)
fn rule_equality(
    candidate: &WaveCandidate,
    classified: &[ClassifiedMonowave],
) -> RuleResult {
    let rid = RuleId::Ch5_Equality;
    if candidate.wave_count != 5 || candidate.monowave_indices.len() < 5 {
        return RuleResult::NotApplicable(rid);
    }
    let mi = &candidate.monowave_indices;
    let mag_w1 = magnitude(classified, mi[0]);
    let mag_w3 = magnitude(classified, mi[2]);
    let mag_w5 = magnitude(classified, mi[4]);

    // 找 Extension(最長者)
    let mags = [(mag_w1, "W1"), (mag_w3, "W3"), (mag_w5, "W5")];
    let max_idx = mags
        .iter()
        .enumerate()
        .max_by(|a, b| a.1 .0.partial_cmp(&b.1 .0).unwrap())
        .map(|(i, _)| i)
        .unwrap();
    let non_ext_mags: Vec<f64> = mags
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != max_idx)
        .map(|(_, (m, _))| *m)
        .collect();
    let (a, b) = (non_ext_mags[0], non_ext_mags[1]);

    if b < 1e-12 {
        return RuleResult::NotApplicable(rid);
    }
    let ratio = a / b;

    // 容差檢查:等價 ±10% 或 Fib 61.8% / 161.8% ±4%
    let approx_equal = (ratio - 1.0).abs() <= APPROX_TOL;
    let fib_618 = (ratio - FIB_618).abs() <= FIB_TOL || (ratio - 1.0 / FIB_618).abs() <= FIB_TOL;

    if approx_equal || fib_618 {
        RuleResult::Pass
    } else {
        RuleResult::Fail(RuleRejection {
            candidate_id: candidate.id.clone(),
            rule_id: rid,
            expected: format!(
                "Equality:非延伸兩 wave 須等價(±10%)或 Fib 61.8/161.8%(±4%),Ext = {}",
                mags[max_idx].1
            ),
            actual: format!(
                "non-extended ratio = {:.4}(magnitudes {:.4} / {:.4})",
                ratio, a, b
            ),
            gap: (ratio - 1.0).abs() * 100.0,
            neely_page: "neely_rules.md §Rule of Equality 1321-1325 行".to_string(),
        })
    }
}

/// Ch5_Alternation { Construction }:W2 與 W4 的 Construction 軸不同。
///
/// 演算法:
///   1. 取 W2(mi[1])與 W4(mi[3])的 structure_label_candidates
///   2. 抽出各自的 ":3" Construction type:
///      - Flat 特徵:含 F3 + C3 + L3 序列(本 PR 簡化為 c3 為 dominant)
///      - Zigzag 特徵:含 Five(:5) 或 F5 / L5
///      - Triangle 特徵:含 c3 序列(較難從單一 monowave 判斷)
///   3. 簡化判定:W2/W4 的 dominant 標籤類型若相同 → Fail(缺 Alternation)
///      若不同 → Pass
///   4. 若任一 monowave 無 Structure Label(Stage 0 未填)→ NotApplicable
fn rule_alternation_construction(
    candidate: &WaveCandidate,
    classified: &[ClassifiedMonowave],
) -> RuleResult {
    let rid = RuleId::Ch5_Alternation {
        axis: AlternationAxis::Construction,
    };
    if candidate.wave_count != 5 || candidate.monowave_indices.len() < 5 {
        return RuleResult::NotApplicable(rid);
    }
    let mi = &candidate.monowave_indices;
    let w2_labels = &classified[mi[1]].structure_label_candidates;
    let w4_labels = &classified[mi[3]].structure_label_candidates;

    if w2_labels.is_empty() || w4_labels.is_empty() {
        return RuleResult::NotApplicable(rid);
    }

    // 抽出各 monowave 的 "Construction" 類型 — 簡化為「含哪種 :5 系列 vs :3 系列」
    let w2_kind = dominant_construction_kind(w2_labels);
    let w4_kind = dominant_construction_kind(w4_labels);

    match (w2_kind, w4_kind) {
        (Some(k2), Some(k4)) if k2 == k4 => RuleResult::Fail(RuleRejection {
            candidate_id: candidate.id.clone(),
            rule_id: rid,
            expected: "Alternation(Construction):W2 與 W4 須不同 Construction 類型".to_string(),
            actual: format!(
                "W2 = {:?}, W4 = {:?}(同 Construction → 缺 Alternation)",
                k2, k4
            ),
            gap: 0.0,
            neely_page: "neely_rules.md §Rule of Alternation 1311-1319 行".to_string(),
        }),
        (Some(_), Some(_)) => RuleResult::Pass,
        _ => RuleResult::NotApplicable(rid),
    }
}

/// 簡化 Construction 類型分類:
///   Impulsive(:5 系列)/ Corrective(:3 系列)/ Mixed
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConstructionKind {
    Impulsive,
    Corrective,
}

fn dominant_construction_kind(
    labels: &[crate::output::StructureLabelCandidate],
) -> Option<ConstructionKind> {
    use crate::output::Certainty;
    // 只看 Primary
    let primary: Vec<&crate::output::StructureLabelCandidate> = labels
        .iter()
        .filter(|c| matches!(c.certainty, Certainty::Primary))
        .collect();
    if primary.is_empty() {
        return None;
    }
    let has_five = primary.iter().any(|c| {
        matches!(
            c.label,
            StructureLabel::Five
                | StructureLabel::F5
                | StructureLabel::L5
                | StructureLabel::UnknownFive
                | StructureLabel::S5
                | StructureLabel::SL5
        )
    });
    let has_three = primary.iter().any(|c| {
        matches!(
            c.label,
            StructureLabel::Three
                | StructureLabel::F3
                | StructureLabel::C3
                | StructureLabel::L3
                | StructureLabel::UnknownThree
                | StructureLabel::SL3
                | StructureLabel::XC3
                | StructureLabel::BC3
                | StructureLabel::BF3
        )
    });

    match (has_five, has_three) {
        (true, false) => Some(ConstructionKind::Impulsive),
        (false, true) => Some(ConstructionKind::Corrective),
        (true, true) | (false, false) => None, // 混合 / 無 — N/A
    }
}

#[inline]
fn magnitude(classified: &[ClassifiedMonowave], idx: usize) -> f64 {
    classified[idx].metrics.magnitude
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::monowave::ProportionMetrics;
    use crate::output::{Certainty, Monowave, MonowaveDirection, StructureLabelCandidate};
    use chrono::NaiveDate;

    fn cmw_with_labels(mag: f64, labels: Vec<StructureLabel>) -> ClassifiedMonowave {
        let candidates = labels
            .into_iter()
            .map(|l| StructureLabelCandidate {
                label: l,
                certainty: Certainty::Primary,
            })
            .collect();
        ClassifiedMonowave {
            monowave: Monowave {
                start_date: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                end_date: NaiveDate::from_ymd_opt(2026, 1, 5).unwrap(),
                start_price: 100.0,
                end_price: 100.0 + mag,
                direction: MonowaveDirection::Up,
            },
            atr_at_start: 1.0,
            metrics: ProportionMetrics {
                magnitude: mag,
                duration_bars: 5,
                atr_relative: 1.0,
                slope_vs_45deg: 1.0,
            },
            structure_label_candidates: candidates,
        }
    }

    fn make_5wave(
        mags: [f64; 5],
        labels: [Vec<StructureLabel>; 5],
    ) -> (Vec<ClassifiedMonowave>, WaveCandidate) {
        let classified = mags
            .iter()
            .zip(labels.into_iter())
            .map(|(&m, l)| cmw_with_labels(m, l))
            .collect();
        let candidate = WaveCandidate {
            id: "c5-test".to_string(),
            monowave_indices: vec![0, 1, 2, 3, 4],
            wave_count: 5,
            initial_direction: MonowaveDirection::Up,
        };
        (classified, candidate)
    }

    #[test]
    fn equality_passes_when_non_ext_near_equal() {
        // W1=10, W3=20 (Extension), W5=10.5 → non-ext = 10 vs 10.5 → ratio 0.95 → pass (≤10%)
        let (classified, candidate) = make_5wave(
            [10.0, 5.0, 20.0, 5.0, 10.5],
            [vec![], vec![], vec![], vec![], vec![]],
        );
        assert!(rule_equality(&candidate, &classified).is_pass());
    }

    #[test]
    fn equality_passes_when_non_ext_618_fib() {
        // W1=10, W3=20 (Extension), W5=6.18 → ratio 0.618 → pass
        let (classified, candidate) = make_5wave(
            [10.0, 5.0, 20.0, 5.0, 6.18],
            [vec![], vec![], vec![], vec![], vec![]],
        );
        assert!(rule_equality(&candidate, &classified).is_pass());
    }

    #[test]
    fn equality_fails_when_non_ext_unrelated() {
        // W1=10, W3=20 (Extension), W5=3 → ratio 0.3 → 不在 1.0 ±10% 也不在 0.618 ±4% → fail
        let (classified, candidate) = make_5wave(
            [10.0, 5.0, 20.0, 5.0, 3.0],
            [vec![], vec![], vec![], vec![], vec![]],
        );
        assert!(rule_equality(&candidate, &classified).is_fail());
    }

    #[test]
    fn alternation_construction_passes_when_w2_w4_differ() {
        // W2 = Impulsive (含 :5), W4 = Corrective (含 :c3) → Alternation ✓
        let (classified, candidate) = make_5wave(
            [10.0, 5.0, 20.0, 5.0, 10.0],
            [
                vec![],
                vec![StructureLabel::Five],
                vec![],
                vec![StructureLabel::C3],
                vec![],
            ],
        );
        assert!(rule_alternation_construction(&candidate, &classified).is_pass());
    }

    #[test]
    fn alternation_construction_fails_when_w2_w4_same_kind() {
        // W2 = Corrective, W4 = Corrective → 缺 Alternation
        let (classified, candidate) = make_5wave(
            [10.0, 5.0, 20.0, 5.0, 10.0],
            [
                vec![],
                vec![StructureLabel::F3],
                vec![],
                vec![StructureLabel::C3],
                vec![],
            ],
        );
        assert!(rule_alternation_construction(&candidate, &classified).is_fail());
    }

    #[test]
    fn alternation_construction_n_a_when_labels_empty() {
        let (classified, candidate) = make_5wave(
            [10.0, 5.0, 20.0, 5.0, 10.0],
            [vec![], vec![], vec![], vec![], vec![]],
        );
        assert!(matches!(
            rule_alternation_construction(&candidate, &classified),
            RuleResult::NotApplicable(_)
        ));
    }
}
