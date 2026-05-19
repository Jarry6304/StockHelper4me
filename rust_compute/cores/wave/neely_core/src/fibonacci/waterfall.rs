// waterfall.rs — Ch12 Waterfall Effect ±5% 偵測
//
// 對齊 m3Spec/neely_rules.md §Ch12 Waterfall Effect + Reverse Logic Rule。
//
// **語意**:當 Impulse 第三段(W3)或第五段(W5)價格表現超過 external Fibonacci
// 2.618 倍 + 5% 容差 → 屬「Waterfall Effect」加速 cascade,Reverse Logic 觸發後續
// 觀察「市場處於更大形態中段」。
//
// **v4.2 P1.2 落地**(2026-05-19):
//   - 解凍 `fibonacci/projection.rs:19` 註解「留 P11+ Reverse Logic 偵測時啟用」
//   - 啟用 `fibonacci/ratios.rs::WATERFALL_TOLERANCE_PCT = 5.0` 常數
//   - 只對 Trending Impulse(`NeelyPatternType::Impulse`)觸發
//   - 對 Diagonal / Triangle / Zigzag / Flat / Combination 不觸發
//     (Waterfall 是 Impulse 特有「加速 cascade」現象,其他模式內部 ratios 上限是 161.8% 而非 261.8%)
//
// **判定**:
//   1. 5-wave Impulse + classified slice ≥ 5
//   2. 計算 W3/W1 倍率 — 若 > 2.618 + 5% = 2.668 → Strong advisory「W3 cascade」
//   3. 計算 W5/max(W1, W3) 倍率 — 若 > 2.618 + 5% → Strong advisory「W5 cascade」
//      (W5 extension 場景,超 2.618 倍非延伸段已罕見,Waterfall 暗示更強)

use super::ratios::WATERFALL_TOLERANCE_PCT;
use crate::monowave::ClassifiedMonowave;
use crate::output::{AdvisoryFinding, AdvisorySeverity, NeelyPatternType, RuleId, Scenario};

/// External Fibonacci 上限(對 Trending Impulse W3 extension 標準上限,
/// 對齊 NEELY_FIB_RATIOS 最大值 2.618)。
const FIB_2618: f64 = 2.618;

/// Waterfall 觸發倍率閾值 = 2.618 + 5% = 2.7489。
fn waterfall_threshold() -> f64 {
    FIB_2618 * (1.0 + WATERFALL_TOLERANCE_PCT / 100.0)
}

/// 對 Trending Impulse 偵測 Waterfall Effect ±5%。
///
/// 回傳值:
/// - `Some(AdvisoryFinding)` with `Ch12_WaterfallEffect` rule_id;Strong severity 表示
///   實際 cascade 達標;Info severity 表示 cascade 未達 5% 容差
/// - `None`:非 Impulse 或 monowave 不足 5 段
pub fn check_waterfall_effect(
    scenario: &Scenario,
    classified: &[ClassifiedMonowave],
) -> Option<AdvisoryFinding> {
    if !matches!(scenario.pattern_type, NeelyPatternType::Impulse) {
        return None;
    }
    let waves = crate::advanced_rules::scenario_monowaves(scenario, classified);
    if waves.len() < 5 {
        return None;
    }

    let w1_mag = waves[0].metrics.magnitude;
    let w3_mag = waves[2].metrics.magnitude;
    let w5_mag = waves[4].metrics.magnitude;

    if w1_mag <= 1e-12 {
        return None;
    }

    let threshold = waterfall_threshold();

    // 1) 檢查 W3 / W1 超過 2.618 + 5%
    let w3_ratio = w3_mag / w1_mag;
    if w3_ratio > threshold {
        return Some(AdvisoryFinding {
            rule_id: RuleId::Ch12_WaterfallEffect,
            severity: AdvisorySeverity::Strong,
            message: format!(
                "Ch12 Waterfall Effect:W3/W1 = {:.3}(> 2.618 + 5% = {:.3})— 加速 cascade 偵測,Reverse Logic 觸發後續觀察(spec §12 Waterfall)",
                w3_ratio, threshold
            ),
        });
    }

    // 2) 檢查 W5 / max(W1, W3) 超過 2.618 + 5%(W5 extension 罕見場景)
    let max_other = w1_mag.max(w3_mag);
    if max_other > 1e-12 {
        let w5_ratio = w5_mag / max_other;
        if w5_ratio > threshold {
            return Some(AdvisoryFinding {
                rule_id: RuleId::Ch12_WaterfallEffect,
                severity: AdvisorySeverity::Strong,
                message: format!(
                    "Ch12 Waterfall Effect:W5/max(W1,W3) = {:.3}(> 2.618 + 5% = {:.3})— W5 extension 加速 cascade",
                    w5_ratio, threshold
                ),
            });
        }
    }

    // cascade 未達標 → Info(供 LLM 看「Waterfall 規則已檢查但未觸發」)
    Some(AdvisoryFinding {
        rule_id: RuleId::Ch12_WaterfallEffect,
        severity: AdvisorySeverity::Info,
        message: format!(
            "Ch12 Waterfall Effect:W3/W1 = {:.3} / W5/max = {:.3} 均 ≤ 2.618 + 5% — 無 cascade",
            w3_ratio,
            w5_mag / max_other.max(1e-12)
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::monowave::ProportionMetrics;
    use crate::output::*;
    use chrono::NaiveDate;

    fn cmw(mag: f64, dur: usize, day_offset: i64) -> ClassifiedMonowave {
        let base = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        ClassifiedMonowave {
            monowave: Monowave {
                start_date: base + chrono::Duration::days(day_offset),
                end_date: base + chrono::Duration::days(day_offset + dur as i64 - 1),
                start_price: 100.0,
                end_price: 100.0 + mag,
                direction: MonowaveDirection::Up,
            },
            atr_at_start: 1.0,
            metrics: ProportionMetrics {
                magnitude: mag,
                duration_bars: dur,
                atr_relative: 1.0,
                slope_vs_45deg: 1.0,
            },
            structure_label_candidates: Vec::new(),
        }
    }

    fn make_scenario(pattern: NeelyPatternType, classified: &[ClassifiedMonowave]) -> Scenario {
        Scenario {
            id: "test".to_string(),
            wave_tree: WaveNode {
                label: "test".to_string(),
                start: classified.first().unwrap().monowave.start_date,
                end: classified.last().unwrap().monowave.end_date,
                children: Vec::new(),
            },
            pattern_type: pattern,
            initial_direction: MonowaveDirection::Up,
            compacted_base_label: StructureLabel::Five,
            structure_label: "test".to_string(),
            complexity_level: ComplexityLevel::Simple,
            power_rating: PowerRating::Neutral,
            max_retracement: None,
            post_pattern_behavior: PostBehavior::Unconstrained,
            passed_rules: Vec::new(),
            deferred_rules: Vec::new(),
            rules_passed_count: 0,
            deferred_rules_count: 0,
            invalidation_triggers: Vec::new(),
            expected_fib_zones: Vec::new(),
            structural_facts: StructuralFacts::default(),
            advisory_findings: Vec::new(),
            in_triangle_context: false,
            awaiting_l_label: false,
            monowave_structure_labels: Vec::new(),
            round_state: RoundState::Round1,
            pattern_isolation_anchors: Vec::new(),
            triplexity_detected: false,
        }
    }

    #[test]
    fn waterfall_strong_when_w3_exceeds_2_618_plus_5pct() {
        // W1 mag=10, W3 mag=30 → ratio = 3.0 > 2.668 → Strong
        let classified = vec![
            cmw(10.0, 5, 0),
            cmw(3.0, 3, 5),
            cmw(30.0, 8, 8),
            cmw(8.0, 4, 16),
            cmw(15.0, 5, 20),
        ];
        let scenario = make_scenario(NeelyPatternType::Impulse, &classified);
        let f = check_waterfall_effect(&scenario, &classified).expect("should fire");
        assert!(matches!(f.severity, AdvisorySeverity::Strong));
        assert!(matches!(f.rule_id, RuleId::Ch12_WaterfallEffect));
        assert!(f.message.contains("W3/W1"));
    }

    #[test]
    fn waterfall_strong_when_w5_extension_exceeds_threshold() {
        // W1 = 10, W3 = 15, W5 = 50 → W5/max(10,15) = 3.33 > 2.668 → Strong (W5 cascade)
        let classified = vec![
            cmw(10.0, 5, 0),
            cmw(3.0, 3, 5),
            cmw(15.0, 8, 8),
            cmw(5.0, 4, 16),
            cmw(50.0, 8, 20),
        ];
        let scenario = make_scenario(NeelyPatternType::Impulse, &classified);
        let f = check_waterfall_effect(&scenario, &classified).expect("should fire");
        assert!(matches!(f.severity, AdvisorySeverity::Strong));
        assert!(f.message.contains("W5/max"));
    }

    #[test]
    fn waterfall_info_when_no_cascade() {
        // 一般 5-wave Impulse,W3/W1 = 2.0 < 2.668 → Info
        let classified = vec![
            cmw(10.0, 5, 0),
            cmw(3.0, 3, 5),
            cmw(20.0, 8, 8),
            cmw(5.0, 4, 16),
            cmw(12.0, 6, 20),
        ];
        let scenario = make_scenario(NeelyPatternType::Impulse, &classified);
        let f = check_waterfall_effect(&scenario, &classified).expect("should fire");
        assert!(matches!(f.severity, AdvisorySeverity::Info));
    }

    #[test]
    fn waterfall_none_for_zigzag() {
        let classified = vec![
            cmw(10.0, 5, 0),
            cmw(3.0, 3, 5),
            cmw(30.0, 8, 8),
        ];
        let scenario = make_scenario(
            NeelyPatternType::Zigzag {
                sub_kind: ZigzagKind::Single,
            },
            &classified,
        );
        assert!(check_waterfall_effect(&scenario, &classified).is_none());
    }

    #[test]
    fn waterfall_none_when_less_than_5_waves() {
        let classified = vec![cmw(10.0, 5, 0), cmw(3.0, 3, 5), cmw(20.0, 8, 8)];
        let scenario = make_scenario(NeelyPatternType::Impulse, &classified);
        assert!(check_waterfall_effect(&scenario, &classified).is_none());
    }
}
