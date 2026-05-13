// ch9.rs — Ch9 Advanced Rules(Trendline Touchpoints / Time Rule / Exception)
//
// 對齊 m3Spec/neely_rules.md §Ch9 Basic Neely Extensions(1930-1994 行)。
//
// **Phase 7 PR**:
//   - Trendline Touchpoints Rule(spec 1957-1961):5+ 點觸線 → Impulse 不可能
//   - Time Rule(spec 1963-1971):3 相鄰同級波不可時間皆等
//   - Exception Rule Aspect 1/2(spec 1979-1990):本 PR 提供 predicate helpers,
//     由 caller(validator dispatcher / classifier)決定何時觸發
//   - Structure Integrity(spec 1992-1994):純宣示,advisory_findings 寫一條 Info
//   - Independent / Simultaneous(spec 1973-1977):meta rules,本 PR 不單獨檢測

use super::scenario_monowaves;
use crate::monowave::ClassifiedMonowave;
use crate::output::{
    AdvisoryFinding, AdvisorySeverity, ExceptionSituation, NeelyPatternType, RuleId, Scenario,
};

/// Ch9 Trendline Touchpoints Rule:
///   - 5 段形態 6 個轉折,同級 ≤ 4 點觸線
///   - 5+ 點觸 0-2 或 1-3 線 → 該段不可能是 Impulse(通常 Double/Triple Zigzag)
///
/// 演算法:檢查 scenario 的 6 個轉折點(0/W1.end/W2.end/W3.end/W4.end/W5.end)
/// 對 1-3 trendline 的「觸線距離」≤ 容差(±2% 價格)的點數。
///
/// **諮詢性**:Phase 7 不直接 reject Impulse,只寫 AdvisoryFinding。
pub fn check_trendline_touchpoints(
    scenario: &Scenario,
    classified: &[ClassifiedMonowave],
) -> Option<AdvisoryFinding> {
    if !matches!(scenario.pattern_type, NeelyPatternType::Impulse) {
        return None;
    }
    let waves = scenario_monowaves(scenario, classified);
    if waves.len() < 5 {
        return None;
    }

    // 6 個轉折點:W1.start(=0)/ W1.end / W2.end / W3.end / W4.end / W5.end
    let pivots: Vec<(chrono::NaiveDate, f64)> = std::iter::once((
        waves[0].monowave.start_date,
        waves[0].monowave.start_price,
    ))
    .chain(
        waves
            .iter()
            .take(5)
            .map(|c| (c.monowave.end_date, c.monowave.end_price)),
    )
    .collect();

    // 1-3 trendline:從 W1.end 到 W3.end
    let w1_end = pivots[1];
    let w3_end = pivots[3];
    let dt = (w3_end.0 - w1_end.0).num_days() as f64;
    if dt.abs() < 1e-12 {
        return None;
    }
    let slope = (w3_end.1 - w1_end.1) / dt;

    // 對每個 pivot 計算到 1-3 線的「相對偏差」
    let tolerance_pct = 0.02;
    let touchpoints = pivots
        .iter()
        .filter(|(t, y)| {
            let dt_t = (*t - w1_end.0).num_days() as f64;
            let line_y = w1_end.1 + slope * dt_t;
            if line_y.abs() < 1e-12 {
                return false;
            }
            ((y - line_y) / line_y).abs() <= tolerance_pct
        })
        .count();

    if touchpoints >= 5 {
        Some(AdvisoryFinding {
            rule_id: RuleId::Ch9_TrendlineTouchpoints,
            severity: AdvisorySeverity::Strong,
            message: format!(
                "Ch9 Trendline Touchpoints:{} 點觸 1-3 線(≥5),該段不可能是 Impulse — 通常為 Double/Triple Zigzag 或 Combination(spec 1959)",
                touchpoints
            ),
        })
    } else {
        Some(AdvisoryFinding {
            rule_id: RuleId::Ch9_TrendlineTouchpoints,
            severity: AdvisorySeverity::Info,
            message: format!("Ch9 Trendline Touchpoints:{} 點觸 1-3 線(< 5,符合 Impulse 預期)", touchpoints),
        })
    }
}

/// Ch9 Time Rule:任何 3 相鄰同級波不可時間皆等。
///
/// 演算法:遍歷 scenario 內 sliding window of 3 consecutive monowaves,
/// 若任一窗口 3 段時間 ±10% 內等價 → fail Time Rule。
pub fn check_time_rule(
    scenario: &Scenario,
    classified: &[ClassifiedMonowave],
) -> Option<AdvisoryFinding> {
    let waves = scenario_monowaves(scenario, classified);
    if waves.len() < 3 {
        return None;
    }
    let tol = 0.10;
    for window in waves.windows(3) {
        let d1 = window[0].metrics.duration_bars as f64;
        let d2 = window[1].metrics.duration_bars as f64;
        let d3 = window[2].metrics.duration_bars as f64;
        if d1 < 1e-12 || d2 < 1e-12 || d3 < 1e-12 {
            continue;
        }
        let r12 = (d1 - d2).abs() / d1.max(d2);
        let r23 = (d2 - d3).abs() / d2.max(d3);
        if r12 <= tol && r23 <= tol {
            return Some(AdvisoryFinding {
                rule_id: RuleId::Ch9_TimeRule,
                severity: AdvisorySeverity::Warning,
                message: format!(
                    "Ch9 Time Rule:3 相鄰 monowaves 時間皆相等(d={:.0}/{:.0}/{:.0},±10% 內) — 第三段未完成或非同級(spec 1971)",
                    d1, d2, d3
                ),
            });
        }
    }
    Some(AdvisoryFinding {
        rule_id: RuleId::Ch9_TimeRule,
        severity: AdvisorySeverity::Info,
        message: "Ch9 Time Rule:無 3 相鄰同時時間 window".to_string(),
    })
}

/// Ch9 Exception Rule Aspect 1 — predicate helper:
///   檢測 scenario 是否符合允許「單規則失靈」的三情境之一。
///
/// 適用場景(spec 1980-1986):
///   - A:Multiwave 或更大形態的結尾
///   - B:Terminal (diagonal triangle) 的 wave-5 或 c-wave
///   - C:進入或離開 Contracting/Expanding Triangle 位置
///
/// **本 PR best-guess**:用 pattern_type 直接對應:
///   - Diagonal { .. } → TerminalW5OrC
///   - Triangle { .. } → TriangleEntryExit
///   - 其他 → None(不符合三情境)
pub fn exception_aspect_1_situation(scenario: &Scenario) -> Option<ExceptionSituation> {
    match &scenario.pattern_type {
        NeelyPatternType::Diagonal { .. } => Some(ExceptionSituation::TerminalW5OrC),
        NeelyPatternType::Triangle { .. } => Some(ExceptionSituation::TriangleEntryExit),
        // Multiwave 結尾需 polywave 嵌套偵測(留 P8/P9)
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::monowave::ProportionMetrics;
    use crate::output::*;
    use chrono::NaiveDate;

    fn cmw(start_p: f64, end_p: f64, dur: usize, day_offset: i64) -> ClassifiedMonowave {
        let base = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        ClassifiedMonowave {
            monowave: Monowave {
                start_date: base + chrono::Duration::days(day_offset),
                end_date: base + chrono::Duration::days(day_offset + dur as i64 - 1),
                start_price: start_p,
                end_price: end_p,
                direction: if end_p > start_p {
                    MonowaveDirection::Up
                } else {
                    MonowaveDirection::Down
                },
            },
            atr_at_start: 1.0,
            metrics: ProportionMetrics {
                magnitude: (end_p - start_p).abs(),
                duration_bars: dur,
                atr_relative: 1.0,
                slope_vs_45deg: 1.0,
            },
            structure_label_candidates: Vec::new(),
        }
    }

    fn make_scenario(pattern: NeelyPatternType, start: NaiveDate, end: NaiveDate) -> Scenario {
        Scenario {
            id: "test".to_string(),
            wave_tree: WaveNode {
                label: "test".to_string(),
                start,
                end,
                children: Vec::new(),
            },
            pattern_type: pattern,
            initial_direction: MonowaveDirection::Up,
            compacted_base_label: StructureLabel::Five,
            structure_label: "test".to_string(),
            complexity_level: ComplexityLevel::Simple,
            power_rating: PowerRating::Neutral,
            max_retracement: 0.0,
            post_pattern_behavior: PostBehavior::Indeterminate,
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
        }
    }

    #[test]
    fn time_rule_3_equal_warns() {
        // 3 個 monowaves 時間皆 = 5 → Time Rule fail
        let classified = vec![
            cmw(100.0, 110.0, 5, 0),
            cmw(110.0, 105.0, 5, 5),
            cmw(105.0, 115.0, 5, 10),
        ];
        let scenario = make_scenario(
            NeelyPatternType::Zigzag {
                sub_kind: ZigzagKind::Single,
            },
            classified[0].monowave.start_date,
            classified[2].monowave.end_date,
        );
        let result = check_time_rule(&scenario, &classified).expect("expect finding");
        assert!(matches!(result.severity, AdvisorySeverity::Warning));
    }

    #[test]
    fn time_rule_unequal_durations_info() {
        let classified = vec![
            cmw(100.0, 110.0, 5, 0),
            cmw(110.0, 105.0, 8, 5),
            cmw(105.0, 115.0, 3, 13),
        ];
        let scenario = make_scenario(
            NeelyPatternType::Zigzag {
                sub_kind: ZigzagKind::Single,
            },
            classified[0].monowave.start_date,
            classified[2].monowave.end_date,
        );
        let result = check_time_rule(&scenario, &classified).expect("expect finding");
        assert!(matches!(result.severity, AdvisorySeverity::Info));
    }

    #[test]
    fn exception_aspect_1_diagonal_yields_terminal_situation() {
        let scenario = make_scenario(
            NeelyPatternType::Diagonal {
                sub_kind: DiagonalKind::Leading,
            },
            NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2026, 1, 10).unwrap(),
        );
        let sit = exception_aspect_1_situation(&scenario);
        assert!(matches!(sit, Some(ExceptionSituation::TerminalW5OrC)));
    }

    #[test]
    fn exception_aspect_1_triangle_yields_triangle_situation() {
        let scenario = make_scenario(
            NeelyPatternType::Triangle {
                sub_kind: TriangleKind::Contracting,
            },
            NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2026, 1, 10).unwrap(),
        );
        let sit = exception_aspect_1_situation(&scenario);
        assert!(matches!(sit, Some(ExceptionSituation::TriangleEntryExit)));
    }

    #[test]
    fn exception_aspect_1_impulse_none() {
        let scenario = make_scenario(
            NeelyPatternType::Impulse,
            NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2026, 1, 10).unwrap(),
        );
        assert!(exception_aspect_1_situation(&scenario).is_none());
    }
}
