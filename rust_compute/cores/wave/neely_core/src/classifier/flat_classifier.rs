// flat_classifier.rs — Phase 16:Flat 7-variant + RunningCorrection 判定
//
// 對齊 m3Spec/neely_rules.md §Ch5 Flat 詳細規則(line 2157-2239)
//       + §第 10 章 line 2024-2037 Pattern Implications
//       + m3Spec/neely_core_architecture.md §9.1 r5 line 1161
//
// **判定流程**(spec line 2157-2239):
//   wave-a / wave-b / wave-c magnitudes 進來 →
//   依 b/a 比例 + c 是否完全回測 b → FlatKind 變體
//
// **設計**(去耦合不抽象):
//   - 純 fn input/output,不引入 Classifier trait
//   - 比例 / 容差常數 inline 寫死(對齊 architecture §4.5 不外部化)
//   - Running Correction 上提為 NeelyPatternType::RunningCorrection,
//     由 classifier::classify_3wave 在 「b > a AND c 不退至起點」場景下回傳

use crate::output::FlatKind;

/// Phase 16:依 a / b / c monowave magnitudes 判 FlatKind 變體。
///
/// 對齊 spec line 2157-2239 詳細規則:
///   - Common:           b/a ∈ [81%, 100%]  AND  c/b ≥ 100%
///   - BFailure:         b/a ∈ [61.8%, 81%) AND  c/b ≥ 100%
///   - CFailure:         b/a ∈ [81%, 100%]  AND  c/b < 100%
///   - DoubleFailure:    b/a ∈ [61.8%, 81%) AND  c/b < 100%
///   - Irregular:        b/a ∈ (100%, 138.2%]
///                       (含 101-123.6% 與 123.6-138.2% 兩 sub-range,spec 24/05 v1.4 補完)
///   - IrregularFailure: b/a > 138.2%       AND  c/b < 100%(spec line 2238)
///   - Elongated:        b/a > 138.2%       AND  c > a(Triangle/Terminal 內罕見)
///
/// 容差規範:此處的 81% / 100% / 138.2% 等臨界值對齊 Fibonacci 常數,容差由 caller
/// (validator/flat_rules.rs)做最終 ±4% 比對;本 fn 採嚴格邊界(僅供 classifier
/// 給 candidate 分類,validator 才驗 Pass/Fail)。
///
/// **返回 None**:b/a < 61.8%(不符任何 Flat 變體最低要求)
/// → caller(`classifier::classify_3wave`)應改試 Zigzag 或 Triangle 解讀。
pub fn classify_flat(a_mag: f64, b_mag: f64, c_mag: f64) -> Option<FlatKind> {
    if a_mag <= 0.0 {
        return None;
    }
    let b_over_a = b_mag / a_mag;
    let c_over_b = if b_mag > 0.0 { c_mag / b_mag } else { 0.0 };
    let c_over_a = c_mag / a_mag;

    // Elongated 特殊判定:c > a(對齊 spec line 2244-2247 Elongated 場景)
    if b_over_a > 1.382 && c_over_a > 1.0 {
        return Some(FlatKind::Elongated);
    }

    match b_over_a {
        // < 61.8% → 不符 Flat 最低要求
        x if x < 0.618 => None,
        // 61.8% ≤ b/a < 81%
        x if x < 0.81 => {
            if c_over_b >= 1.0 {
                Some(FlatKind::BFailure)
            } else {
                Some(FlatKind::DoubleFailure)
            }
        }
        // 81% ≤ b/a ≤ 100%
        x if x <= 1.0 => {
            if c_over_b >= 1.0 {
                Some(FlatKind::Common)
            } else {
                Some(FlatKind::CFailure)
            }
        }
        // 100% < b/a ≤ 138.2% → Irregular(含 101-123.6% / 123.6-138.2% 兩 sub-range)
        x if x <= 1.382 => Some(FlatKind::Irregular),
        // > 138.2% → IrregularFailure(spec line 2238 明文 b > 138.2% × a)
        _ => {
            if c_over_b < 1.0 {
                Some(FlatKind::IrregularFailure)
            } else {
                // b > 138.2% AND c ≥ 100% × b → 仍偏 Irregular(罕見場景)
                Some(FlatKind::Irregular)
            }
        }
    }
}

/// Phase 16:Running Correction 偵測 — c 不退至 a 起點。
///
/// 對齊 spec line 2024-2037 Running 場景 + r5 line 1161「上提頂層」設計:
///   Running Correction 結構為 3 段(a-b-c)但 c 不退至 a 起點,
///   後續 Impulse 多 > 161.8%(常達 261.8%)。
///
/// 判定:b 強於 a(b/a > 100%)且 c 不退至 a 起點(c < a — 即 c_over_a < 1.0)。
/// 這對應 Elongated 的反面 — Elongated 是 c > a,Running 是 c < a 且 b 已超 a。
pub fn is_running_correction(a_mag: f64, b_mag: f64, c_mag: f64) -> bool {
    if a_mag <= 0.0 {
        return false;
    }
    let b_over_a = b_mag / a_mag;
    let c_over_a = c_mag / a_mag;
    // Running Correction:b > a(b 強)+ c 短於 a(c 不退至 a 起點)
    b_over_a > 1.0 && c_over_a < 1.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_flat_at_85_pct_b_full_c() {
        // b/a = 0.85(81-100% Common range)+ c ≥ b → Common
        assert_eq!(classify_flat(100.0, 85.0, 90.0), Some(FlatKind::Common));
    }

    #[test]
    fn b_failure_at_70_pct_b_full_c() {
        // b/a = 0.70(61.8-81% BFailure range)+ c ≥ b → BFailure
        assert_eq!(classify_flat(100.0, 70.0, 75.0), Some(FlatKind::BFailure));
    }

    #[test]
    fn c_failure_at_85_pct_b_short_c() {
        // b/a = 0.85 + c < b → CFailure
        assert_eq!(classify_flat(100.0, 85.0, 50.0), Some(FlatKind::CFailure));
    }

    #[test]
    fn double_failure_at_70_pct_b_short_c() {
        // b/a = 0.70 + c < b → DoubleFailure
        assert_eq!(classify_flat(100.0, 70.0, 30.0), Some(FlatKind::DoubleFailure));
    }

    #[test]
    fn irregular_at_120_pct_b() {
        // b/a = 1.20(100-138.2% Irregular range)→ Irregular
        assert_eq!(classify_flat(100.0, 120.0, 80.0), Some(FlatKind::Irregular));
    }

    #[test]
    fn irregular_failure_at_150_pct_b_short_c() {
        // b/a = 1.50(> 138.2%)+ c < b → IrregularFailure(spec line 2238)
        assert_eq!(
            classify_flat(100.0, 150.0, 80.0),
            Some(FlatKind::IrregularFailure)
        );
    }

    #[test]
    fn elongated_when_b_huge_and_c_greater_than_a() {
        // b/a = 1.50 + c > a → Elongated(Triangle/Terminal 內罕見)
        assert_eq!(classify_flat(100.0, 150.0, 120.0), Some(FlatKind::Elongated));
    }

    #[test]
    fn returns_none_for_b_too_small() {
        // b/a = 0.50 < 0.618 → 不符 Flat 任一變體
        assert_eq!(classify_flat(100.0, 50.0, 80.0), None);
    }

    #[test]
    fn returns_none_for_a_zero() {
        assert_eq!(classify_flat(0.0, 50.0, 80.0), None);
    }

    #[test]
    fn running_correction_detected_when_b_strong_c_short() {
        // b/a = 1.20 + c/a = 0.80 → Running
        assert!(is_running_correction(100.0, 120.0, 80.0));
    }

    #[test]
    fn running_correction_not_detected_when_b_normal() {
        // b/a = 0.85 → b 不超 a,不是 Running
        assert!(!is_running_correction(100.0, 85.0, 50.0));
    }

    #[test]
    fn running_correction_not_detected_when_c_long() {
        // b/a = 1.20 但 c > a → 是 Elongated 不是 Running
        assert!(!is_running_correction(100.0, 120.0, 120.0));
    }
}
