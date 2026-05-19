// ch9.rs — Ch9 Advanced Rules(Trendline Touchpoints / Time Rule / Independent /
// Simultaneous / Exception Aspect 1+2 / Structure Integrity)
//
// 對齊 m3Spec/neely_rules.md §Ch9 Basic Neely Extensions(1930-1994 行)。
//
// **Phase 7 PR**(2026-05-13):
//   - Trendline Touchpoints Rule(spec 1957-1961):5+ 點觸線 → Impulse 不可能
//   - Time Rule(spec 1963-1971):3 相鄰同級波不可時間皆等
//   - Exception Rule Aspect 1 predicate(spec 1979-1986)
//   - Structure Integrity(spec 1992-1994):advisory_findings 寫一條 Info
//
// **v4.2 P1.2 補完**(2026-05-19):
//   - Ch9 Independent Rule advisory(spec 1973-1974)— 多 chapter 規則互不干涉
//   - Ch9 Simultaneous Occurrence advisory(spec 1976-1977)— 同情境須所有規則齊備
//   - Ch9 Exception Aspect 1 Multiwave 結尾分支補完(原 ch9.rs:149 留空 P8/P9)
//   - Ch9 Exception Aspect 2 dispatch(spec 1988-1990)— 規則失效啟動另一規則

use super::scenario_monowaves;
use crate::monowave::ClassifiedMonowave;
use crate::output::{
    AdvisoryFinding, AdvisorySeverity, CombinationKind, ExceptionSituation, NeelyPatternType,
    RuleId, Scenario,
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
///   - A:Multiwave 或更大形態的結尾(v4.2:Combination Triple* 變體 / RunningCorrection)
///   - B:Terminal (diagonal triangle) 的 wave-5 或 c-wave
///   - C:進入或離開 Contracting/Expanding Triangle 位置
///
/// **v4.2 落地完整 3 情境**(原 P7 留空 Multiwave 分支):
///   - Diagonal { .. } → TerminalW5OrC
///   - Triangle { .. } → TriangleEntryExit
///   - Combination { Triple* } / RunningCorrection → MultiwaveEnd
///   - 其他 → None(不符合三情境)
pub fn exception_aspect_1_situation(scenario: &Scenario) -> Option<ExceptionSituation> {
    match &scenario.pattern_type {
        NeelyPatternType::Diagonal { .. } => Some(ExceptionSituation::TerminalW5OrC),
        NeelyPatternType::Triangle { .. } => Some(ExceptionSituation::TriangleEntryExit),
        // v4.2 P1.2 #10:Multiwave 結尾 = Combination Triple* 變體 + RunningCorrection
        // (對齊 spec 1980「Multiwave 或更大形態的結尾」+ reverse_logic 對 Triple* 視為「near completion」)
        NeelyPatternType::Combination { sub_kinds } => {
            let is_triple = sub_kinds.iter().any(|k| {
                matches!(
                    k,
                    CombinationKind::TripleZigzag
                        | CombinationKind::TripleCombination
                        | CombinationKind::TripleThree
                        | CombinationKind::TripleThreeCombination
                        | CombinationKind::TripleThreeRunning
                )
            });
            if is_triple {
                Some(ExceptionSituation::MultiwaveEnd)
            } else {
                None
            }
        }
        NeelyPatternType::RunningCorrection => Some(ExceptionSituation::MultiwaveEnd),
        _ => None,
    }
}

// ── v4.2 P1.2 新 4 個 Ch9 advisory checks ────────────────────────────────

/// Ch9 Independent Rule advisory(spec 1973-1974)— 各規則彼此獨立。
///
/// **語意**:規則間互不干涉,單條規則的結果不應觸發其他規則的判定。
/// **advisory 用途**:當 scenario 啟動多 chapter 規則(passed/deferred/advisory_findings 跨 ≥ 2 chapters)
/// → Info advisory 標示「多 chapter 規則互不干涉」,供 LLM 看 multi-rule independence。
pub fn check_independent_rule(scenario: &Scenario) -> Option<AdvisoryFinding> {
    let chapters_active = count_active_chapters(scenario);
    if chapters_active >= 2 {
        Some(AdvisoryFinding {
            rule_id: RuleId::Ch9_Independent,
            severity: AdvisorySeverity::Info,
            message: format!(
                "Ch9 Independent Rule:scenario 有 {} 個 NEoWave 章節規則同時啟動 — 各規則彼此獨立評估,不互相觸發(spec 1973-1974)",
                chapters_active
            ),
        })
    } else {
        None
    }
}

/// Ch9 Simultaneous Occurrence advisory(spec 1976-1977)— 同情境須所有規則齊備。
///
/// **語意**:同一情境(例如 5-wave Impulse)的所有規則應同時成立;若部分 Pass 部分 Fail
/// → 該情境本身不成立(non-impulsive)。
/// **advisory 用途**:Impulse pattern 預期 Ch5_Essential R1-R7 全部 passed(7 個都在);
/// 若 < 7 個 → Warning advisory「未同時滿足全部 Essential R1-R7」。
pub fn check_simultaneous_occurrence(scenario: &Scenario) -> Option<AdvisoryFinding> {
    if !matches!(scenario.pattern_type, NeelyPatternType::Impulse) {
        return None;
    }
    let passed_essentials: usize = (1..=7)
        .filter(|n| {
            scenario
                .passed_rules
                .iter()
                .any(|r| matches!(r, RuleId::Ch5_Essential(num) if *num == *n))
        })
        .count();
    if passed_essentials < 7 {
        Some(AdvisoryFinding {
            rule_id: RuleId::Ch9_Simultaneous,
            severity: AdvisorySeverity::Warning,
            message: format!(
                "Ch9 Simultaneous Occurrence:5-wave Impulse 預期 Ch5_Essential R1-R7 全部 passed,實際 {}/7 — 該情境未同時齊備(spec 1976-1977)",
                passed_essentials
            ),
        })
    } else {
        Some(AdvisoryFinding {
            rule_id: RuleId::Ch9_Simultaneous,
            severity: AdvisorySeverity::Info,
            message: "Ch9 Simultaneous Occurrence:Ch5_Essential R1-R7 全部 passed(7/7) — 情境齊備".to_string(),
        })
    }
}

/// Ch9 Exception Rule Aspect 2(spec 1988-1990)— 規則失效啟動另一規則。
///
/// **語意**:當一條規則失靈,該失靈本身啟動另一規則。例如:
///   - 2-4 線突破 → Terminal Impulse 啟動(從 Impulse 升 Diagonal)
///   - Thrust 超時 → Non-Limiting / Terminal Triangle 啟動
///
/// **v4.2 best-guess 實作**:檢測 scenario.advisory_findings 中是否含
/// `Ch9_TrendlineTouchpoints` Strong(2-4 線突破暗示)+ pattern_type 是 Diagonal → 觸發
/// Exception Aspect 2,triggered_new_rule = "Terminal Impulse"。
/// 其他組合留 V4.x 細化。
pub fn detect_exception_aspect_2(scenario: &Scenario) -> Option<AdvisoryFinding> {
    let trendline_strong = scenario.advisory_findings.iter().any(|f| {
        matches!(f.rule_id, RuleId::Ch9_TrendlineTouchpoints)
            && matches!(f.severity, AdvisorySeverity::Strong)
    });
    if trendline_strong && matches!(scenario.pattern_type, NeelyPatternType::Diagonal { .. }) {
        return Some(AdvisoryFinding {
            rule_id: RuleId::Ch9_Exception_Aspect2 {
                triggered_new_rule: "Terminal Impulse (Diagonal)".to_string(),
            },
            severity: AdvisorySeverity::Strong,
            message: "Ch9 Exception Aspect 2:Trendline 5+ 觸點(2-4 線突破)觸發 Terminal Impulse 規則 — Impulse 升為 Diagonal(spec 1988-1990)".to_string(),
        });
    }
    None
}

/// 計算 scenario 啟動的 NEoWave 章節數(passed_rules + deferred_rules + advisory_findings
/// 跨章節去重)。
///
/// 章節編碼:Ch3 / Ch4 / Ch5 / Ch6 / Ch7 / Ch8 / Ch9 / Ch10 / Ch11 / Ch12 / Engineering
/// (從 RuleId enum variant prefix 推導)。
fn count_active_chapters(scenario: &Scenario) -> usize {
    let mut chapters: std::collections::HashSet<&'static str> = std::collections::HashSet::new();
    for r in &scenario.passed_rules {
        if let Some(ch) = rule_chapter(r) {
            chapters.insert(ch);
        }
    }
    for r in &scenario.deferred_rules {
        if let Some(ch) = rule_chapter(r) {
            chapters.insert(ch);
        }
    }
    for f in &scenario.advisory_findings {
        if let Some(ch) = rule_chapter(&f.rule_id) {
            chapters.insert(ch);
        }
    }
    chapters.len()
}

fn rule_chapter(rule: &RuleId) -> Option<&'static str> {
    match rule {
        RuleId::Ch3_PreConstructive { .. }
        | RuleId::Ch3_Proportion_Directional
        | RuleId::Ch3_Proportion_NonDirectional
        | RuleId::Ch3_Neutrality_Aspect1
        | RuleId::Ch3_Neutrality_Aspect2
        | RuleId::Ch3_PatternIsolation_Step(_)
        | RuleId::Ch3_SpecialCircumstances => Some("Ch3"),
        RuleId::Ch4_SimilarityBalance_Price
        | RuleId::Ch4_SimilarityBalance_Time
        | RuleId::Ch4_Round1_Series
        | RuleId::Ch4_Round2_Compaction
        | RuleId::Ch4_Round3_Pause
        | RuleId::Ch4_ZigzagDetour => Some("Ch4"),
        RuleId::Ch5_Essential(_)
        | RuleId::Ch5_Overlap_Trending
        | RuleId::Ch5_Overlap_Terminal
        | RuleId::Ch5_Equality
        | RuleId::Ch5_Alternation { .. }
        | RuleId::Ch5_Flat_Min_BRatio
        | RuleId::Ch5_Flat_Min_CRatio
        | RuleId::Ch5_Zigzag_Max_BRetracement
        | RuleId::Ch5_Zigzag_C_TriangleException
        | RuleId::Ch5_Triangle_BRange
        | RuleId::Ch5_Triangle_LegContraction
        | RuleId::Ch5_Triangle_LegEquality_5Pct
        | RuleId::Ch5_Extension
        | RuleId::Ch5_Extension_Exception1
        | RuleId::Ch5_Extension_Exception2 => Some("Ch5"),
        RuleId::Ch9_TrendlineTouchpoints
        | RuleId::Ch9_TimeRule
        | RuleId::Ch9_Independent
        | RuleId::Ch9_Simultaneous
        | RuleId::Ch9_Exception_Aspect1 { .. }
        | RuleId::Ch9_Exception_Aspect2 { .. }
        | RuleId::Ch9_StructureIntegrity => Some("Ch9"),
        RuleId::Engineering_InsufficientData
        | RuleId::Engineering_ForestOverflow
        | RuleId::Engineering_CompactionTimeout => Some("Engineering"),
        // Ch6 / Ch7 / Ch8 / Ch10 / Ch11 / Ch12 章節(v3.6 spec-only,目前未 dispatch)
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

    #[test]
    fn exception_aspect_1_v4_2_combination_triple_returns_multiwave_end() {
        let scenario = make_scenario(
            NeelyPatternType::Combination {
                sub_kinds: vec![CombinationKind::TripleZigzag],
            },
            NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2026, 1, 10).unwrap(),
        );
        let sit = exception_aspect_1_situation(&scenario);
        assert!(matches!(sit, Some(ExceptionSituation::MultiwaveEnd)));
    }

    #[test]
    fn exception_aspect_1_v4_2_running_correction_returns_multiwave_end() {
        let scenario = make_scenario(
            NeelyPatternType::RunningCorrection,
            NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2026, 1, 10).unwrap(),
        );
        let sit = exception_aspect_1_situation(&scenario);
        assert!(matches!(sit, Some(ExceptionSituation::MultiwaveEnd)));
    }

    #[test]
    fn exception_aspect_1_combination_double_returns_none() {
        // Double Combination 不在 spec 1980「Multiwave 或更大形態的結尾」內 → None
        let scenario = make_scenario(
            NeelyPatternType::Combination {
                sub_kinds: vec![CombinationKind::DoubleZigzag],
            },
            NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2026, 1, 10).unwrap(),
        );
        assert!(exception_aspect_1_situation(&scenario).is_none());
    }

    #[test]
    fn independent_rule_returns_info_when_multi_chapter_active() {
        let mut scenario = make_scenario(
            NeelyPatternType::Impulse,
            NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2026, 1, 10).unwrap(),
        );
        // Ch3 + Ch5 + Ch9 三 chapter 都有規則 → multi-chapter
        scenario
            .passed_rules
            .push(RuleId::Ch3_Proportion_Directional);
        scenario.passed_rules.push(RuleId::Ch5_Essential(1));
        scenario.advisory_findings.push(AdvisoryFinding {
            rule_id: RuleId::Ch9_TimeRule,
            severity: AdvisorySeverity::Info,
            message: "test".to_string(),
        });
        let f = check_independent_rule(&scenario).expect("should fire");
        assert!(matches!(f.severity, AdvisorySeverity::Info));
        assert!(matches!(f.rule_id, RuleId::Ch9_Independent));
    }

    #[test]
    fn independent_rule_returns_none_when_single_chapter() {
        let mut scenario = make_scenario(
            NeelyPatternType::Impulse,
            NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2026, 1, 10).unwrap(),
        );
        scenario.passed_rules.push(RuleId::Ch5_Essential(1));
        scenario.passed_rules.push(RuleId::Ch5_Essential(2));
        // 只 Ch5 一個 chapter → < 2 → None
        assert!(check_independent_rule(&scenario).is_none());
    }

    #[test]
    fn simultaneous_occurrence_warns_when_not_all_essentials_passed() {
        let mut scenario = make_scenario(
            NeelyPatternType::Impulse,
            NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2026, 1, 10).unwrap(),
        );
        // 只 R1-R3 passed,R4-R7 缺 → Warning
        for n in 1..=3 {
            scenario.passed_rules.push(RuleId::Ch5_Essential(n));
        }
        let f = check_simultaneous_occurrence(&scenario).expect("should fire");
        assert!(matches!(f.severity, AdvisorySeverity::Warning));
        assert!(f.message.contains("3/7"));
    }

    #[test]
    fn simultaneous_occurrence_info_when_all_essentials_passed() {
        let mut scenario = make_scenario(
            NeelyPatternType::Impulse,
            NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2026, 1, 10).unwrap(),
        );
        for n in 1..=7 {
            scenario.passed_rules.push(RuleId::Ch5_Essential(n));
        }
        let f = check_simultaneous_occurrence(&scenario).expect("should fire");
        assert!(matches!(f.severity, AdvisorySeverity::Info));
        assert!(f.message.contains("7/7"));
    }

    #[test]
    fn simultaneous_occurrence_none_for_non_impulse() {
        let scenario = make_scenario(
            NeelyPatternType::Zigzag {
                sub_kind: ZigzagKind::Single,
            },
            NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2026, 1, 10).unwrap(),
        );
        assert!(check_simultaneous_occurrence(&scenario).is_none());
    }

    #[test]
    fn exception_aspect_2_fires_for_diagonal_with_trendline_strong() {
        let mut scenario = make_scenario(
            NeelyPatternType::Diagonal {
                sub_kind: DiagonalKind::Leading,
            },
            NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2026, 1, 10).unwrap(),
        );
        scenario.advisory_findings.push(AdvisoryFinding {
            rule_id: RuleId::Ch9_TrendlineTouchpoints,
            severity: AdvisorySeverity::Strong,
            message: "5+ touchpoints".to_string(),
        });
        let f = detect_exception_aspect_2(&scenario).expect("should fire");
        assert!(matches!(f.severity, AdvisorySeverity::Strong));
        match f.rule_id {
            RuleId::Ch9_Exception_Aspect2 { triggered_new_rule } => {
                assert!(triggered_new_rule.contains("Terminal Impulse"));
            }
            _ => panic!("expected Ch9_Exception_Aspect2"),
        }
    }

    #[test]
    fn exception_aspect_2_none_when_impulse_no_trendline_strong() {
        let scenario = make_scenario(
            NeelyPatternType::Impulse,
            NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2026, 1, 10).unwrap(),
        );
        assert!(detect_exception_aspect_2(&scenario).is_none());
    }
}
