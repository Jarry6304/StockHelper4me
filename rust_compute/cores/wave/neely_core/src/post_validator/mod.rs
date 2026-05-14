// post_validator — Stage 6:Post-Constructive Validator(Ch6 兩階段確認)
//
// 對齊 m3Spec/neely_rules.md §Ch6 Post-Constructive Rules of Logic(1763-1797 行)
//       + m3Spec/neely_core_architecture.md §7.1 Stage 6
//
// **Phase 6 PR(r5 alignment)**:
//   Ch6 兩階段確認完整實作,接收 candidate-end 之後的 post-pattern monowaves
//   作為「未來走勢」的代理。若沒有 subsequent monowaves(scenario 至序列末)→
//   pattern_complete 預設 true(deferred awaiting 待後續 K 棒),pending_conditions
//   填入待驗證項目。
//
// **Ch6 規則**(spec 1765-1797):
//   - Impulse Stage 1:後續須在 ≤ wave-5 時間內突破 2-4 線
//   - Impulse Stage 2:依 Extension 類型判回測範圍(wave-2 / wave-4 區)
//   - Correction Stage 1/2(b<a 或 b>a):0-B 線突破 + 完全回測 wave-c
//   - Triangle Contracting Stage 1/2:b-d 線突破時間 + Thrust 限制
//   - Triangle Expanding:非確認邏輯(完全回測 e-wave 不可發生)

use crate::monowave::ClassifiedMonowave;
use crate::output::{
    DiagonalKind, MonowaveDirection, NeelyPatternType, Scenario, TriangleKind,
};

#[derive(Debug, Clone)]
pub struct PostValidationReport {
    pub scenario_id: String,
    /// 型態完成度判定(Ch6 兩階段全通過 → true;Stage 1 未通過 → false;
    /// 缺 subsequent monowaves → true with pending_conditions)
    pub pattern_complete: bool,
    /// 待驗證的後續條件(spec 1788-1797 「兩階段」描述,空 vec 表「已完全確認」)
    pub pending_conditions: Vec<String>,
}

/// Phase 6 post_validate:接收 scenario + 完整 classified monowave 序列。
///
/// 流程:
///   1. 從 scenario.wave_tree.end 推導 post-pattern monowaves
///   2. 依 pattern_type 套 Ch6 兩階段確認
///   3. 若 post-pattern monowaves 為空 → pending(預設 pattern_complete = true)
///   4. Stage 1 不過 → pattern_complete = false;Stage 2 不過 → pattern_complete = true
///      + pending Stage 2 確認
pub fn post_validate(
    scenario: &Scenario,
    classified: &[ClassifiedMonowave],
) -> PostValidationReport {
    let post_pattern = find_post_pattern_monowaves(scenario, classified);

    if post_pattern.is_empty() {
        return PostValidationReport {
            scenario_id: scenario.id.clone(),
            pattern_complete: true, // 缺 K 棒 → tentatively true
            pending_conditions: vec![
                "post-pattern monowaves 不足,Ch6 兩階段確認 deferred".to_string(),
            ],
        };
    }

    match scenario.pattern_type {
        NeelyPatternType::Impulse => validate_impulse(scenario, classified, post_pattern),
        NeelyPatternType::Diagonal { sub_kind } => {
            validate_terminal_impulse(scenario, classified, post_pattern, sub_kind)
        }
        NeelyPatternType::Triangle { sub_kind } => {
            validate_triangle(scenario, classified, post_pattern, sub_kind)
        }
        NeelyPatternType::Zigzag { .. } | NeelyPatternType::Flat { .. } => {
            validate_correction(scenario, classified, post_pattern)
        }
        NeelyPatternType::Combination { .. } => {
            // Combination Ch6 規則 spec 未細列,留 P9 接 Ch8 Complex Polywaves
            PostValidationReport {
                scenario_id: scenario.id.clone(),
                pattern_complete: true,
                pending_conditions: vec!["Combination Ch6 規則留 P9 Ch8 補完".to_string()],
            }
        }
    }
}

/// 找出 scenario.wave_tree.end 之後的 monowaves(post-pattern 區段)。
fn find_post_pattern_monowaves<'a>(
    scenario: &Scenario,
    classified: &'a [ClassifiedMonowave],
) -> &'a [ClassifiedMonowave] {
    let end_date = scenario.wave_tree.end;
    // 從第一個 start_date > end_date 的 monowave 起算
    let start_idx = classified
        .iter()
        .position(|c| c.monowave.start_date > end_date)
        .unwrap_or(classified.len());
    &classified[start_idx..]
}

// ---------------------------------------------------------------------------
// Impulse(Trending)Ch6 兩階段確認(spec 1765-1775)
// ---------------------------------------------------------------------------

fn validate_impulse(
    scenario: &Scenario,
    classified: &[ClassifiedMonowave],
    post_pattern: &[ClassifiedMonowave],
) -> PostValidationReport {
    let report_id = scenario.id.clone();

    // Stage 1:後續走勢須在「不長於 wave-5」時間內突破 2-4 線
    let scenario_monowaves = find_scenario_monowaves(scenario, classified);
    if scenario_monowaves.len() < 5 {
        return PostValidationReport {
            scenario_id: report_id,
            pattern_complete: true,
            pending_conditions: vec!["scenario 不足 5 monowaves,Ch6 deferred".to_string()],
        };
    }
    let wave_5 = &scenario_monowaves[4];
    let wave_2 = &scenario_monowaves[1];
    let wave_4 = &scenario_monowaves[3];
    let wave_5_dur = wave_5.metrics.duration_bars;

    // 檢查 post_pattern 是否在 ≤ wave_5_dur 內突破 2-4 線
    let stage_1_pass = check_2_4_line_breach(
        &wave_2.monowave,
        &wave_4.monowave,
        scenario.initial_direction,
        post_pattern,
        wave_5_dur,
    );

    let mut pending = Vec::new();
    if !stage_1_pass {
        // Spec 1769:耗時更長 → wave-5 變為 Terminal 或 wave-4 未完
        return PostValidationReport {
            scenario_id: report_id,
            pattern_complete: false,
            pending_conditions: vec![
                "Ch6 Impulse Stage 1 未通過:2-4 線突破時間超過 wave-5 時間".to_string(),
            ],
        };
    }

    // Stage 2:依 Extension 類型判回測範圍(spec 1771-1775)
    let stage_2_result = validate_impulse_stage_2(scenario, scenario_monowaves, post_pattern);
    match stage_2_result {
        Stage2Result::Pass => {}
        Stage2Result::Pending(msg) => pending.push(msg),
        Stage2Result::Fail(msg) => {
            return PostValidationReport {
                scenario_id: report_id,
                pattern_complete: false,
                pending_conditions: vec![msg],
            };
        }
    }

    PostValidationReport {
        scenario_id: report_id,
        pattern_complete: true,
        pending_conditions: pending,
    }
}

enum Stage2Result {
    Pass,
    Pending(String),
    /// 預留:Stage 2 嚴格 fail 場景(目前 spec 1771-1775 只描述「應」回測,不直接 fail;
    /// 留給 Phase 6 後續細化用)
    #[allow(dead_code)]
    Fail(String),
}

fn validate_impulse_stage_2(
    scenario: &Scenario,
    scenario_monowaves: &[ClassifiedMonowave],
    post_pattern: &[ClassifiedMonowave],
) -> Stage2Result {
    // Extension 判定(已在 wave_rules 用過):最長者為 Extension
    let mag_w1 = scenario_monowaves[0].metrics.magnitude;
    let mag_w3 = scenario_monowaves[2].metrics.magnitude;
    let mag_w5 = scenario_monowaves[4].metrics.magnitude;

    let max_mag = mag_w1.max(mag_w3).max(mag_w5);
    let ext_position = if (mag_w1 - max_mag).abs() < 1e-9 {
        1
    } else if (mag_w3 - max_mag).abs() < 1e-9 {
        3
    } else {
        5
    };

    // 後續走勢「整體回測量」 — 用 post_pattern 對 scenario 範圍的最大反向 movement
    let scenario_end_price = scenario_monowaves[4].monowave.end_price;
    let wave_4_end_price = scenario_monowaves[3].monowave.end_price; // wave-4 區終點
    let wave_2_end_price = scenario_monowaves[1].monowave.end_price; // wave-2 區終點

    // 取 post_pattern 中最大反向 price
    let max_retrace_price = post_pattern
        .iter()
        .map(|c| c.monowave.end_price)
        .fold(scenario_end_price, |acc, p| match scenario.initial_direction {
            MonowaveDirection::Up => acc.min(p),
            MonowaveDirection::Down => acc.max(p),
            _ => acc,
        });

    // 「回到 wave-4 區」= retrace 達到 wave_4_end_price
    let reached_wave_4_zone = match scenario.initial_direction {
        MonowaveDirection::Up => max_retrace_price <= wave_4_end_price,
        MonowaveDirection::Down => max_retrace_price >= wave_4_end_price,
        _ => false,
    };

    let _ = wave_2_end_price; // wave_2_end_price 留給 1st Ext 細分擴充

    match ext_position {
        1 => {
            // 1st Wave Extended:後續必回 wave-4 區
            if reached_wave_4_zone {
                Stage2Result::Pass
            } else {
                Stage2Result::Pending(
                    "1st Ext Impulse Stage 2 deferred:等候後續回到 wave-4 區".to_string(),
                )
            }
        }
        3 => {
            // 3rd Wave Extended:必回 wave-4 區
            if reached_wave_4_zone {
                Stage2Result::Pass
            } else {
                Stage2Result::Pending(
                    "3rd Ext Impulse Stage 2 deferred:等候後續回到 wave-4 區".to_string(),
                )
            }
        }
        5 => {
            // 5th Wave Extended:必至少回測 61.8% × wave-5
            let wave_5_mag = mag_w5;
            let retrace_required = wave_5_mag * 0.618;
            let retrace_amount = (scenario_end_price - max_retrace_price).abs();
            if retrace_amount >= retrace_required {
                Stage2Result::Pass
            } else {
                Stage2Result::Pending(
                    "5th Ext Impulse Stage 2 deferred:等候 ≥ 61.8% × wave-5 回測".to_string(),
                )
            }
        }
        _ => Stage2Result::Pending("Extension 判定不明".to_string()),
    }
}

// ---------------------------------------------------------------------------
// Terminal Impulse(Diagonal)Ch6:後續走勢須完全回測整段 Terminal(spec 2056)
// ---------------------------------------------------------------------------

fn validate_terminal_impulse(
    scenario: &Scenario,
    classified: &[ClassifiedMonowave],
    post_pattern: &[ClassifiedMonowave],
    _sub_kind: DiagonalKind,
) -> PostValidationReport {
    let scenario_monowaves = find_scenario_monowaves(scenario, classified);
    if scenario_monowaves.is_empty() {
        return PostValidationReport {
            scenario_id: scenario.id.clone(),
            pattern_complete: true,
            pending_conditions: Vec::new(),
        };
    }
    let start_price = scenario_monowaves[0].monowave.start_price;
    let end_price = scenario_monowaves
        .last()
        .map(|c| c.monowave.end_price)
        .unwrap_or(start_price);

    // 完全回測 = post_pattern 中任一 monowave end_price 觸及 start_price
    let fully_retraced = post_pattern.iter().any(|c| {
        match scenario.initial_direction {
            MonowaveDirection::Up => c.monowave.end_price <= start_price,
            MonowaveDirection::Down => c.monowave.end_price >= start_price,
            _ => false,
        }
    });

    let _ = end_price;
    if fully_retraced {
        PostValidationReport {
            scenario_id: scenario.id.clone(),
            pattern_complete: true,
            pending_conditions: Vec::new(),
        }
    } else {
        PostValidationReport {
            scenario_id: scenario.id.clone(),
            pattern_complete: true, // 仍可能成立,但需等更多 K 棒
            pending_conditions: vec![
                "Terminal Impulse 須 100% 回測整段(spec 2056),deferred".to_string(),
            ],
        }
    }
}

// ---------------------------------------------------------------------------
// Correction(Zigzag/Flat)Ch6 兩階段(spec 1777-1785)
// ---------------------------------------------------------------------------

fn validate_correction(
    scenario: &Scenario,
    classified: &[ClassifiedMonowave],
    post_pattern: &[ClassifiedMonowave],
) -> PostValidationReport {
    let report_id = scenario.id.clone();
    let scenario_monowaves = find_scenario_monowaves(scenario, classified);
    if scenario_monowaves.len() < 3 {
        return PostValidationReport {
            scenario_id: report_id,
            pattern_complete: true,
            pending_conditions: vec!["scenario 不足 3 monowaves,Correction Ch6 deferred".to_string()],
        };
    }
    let wave_a = &scenario_monowaves[0];
    let wave_b = &scenario_monowaves[1];
    let wave_c = &scenario_monowaves[2];
    let mag_a = wave_a.metrics.magnitude;
    let mag_b = wave_b.metrics.magnitude;
    let wave_c_dur = wave_c.metrics.duration_bars;

    let b_less_than_a = mag_b < mag_a;

    if b_less_than_a {
        // wave-b < wave-a: Stage 1 = 突破 0-B 線(≤ wave-c 時間)
        //                   Stage 2 = 完全回測 wave-c(≤ wave-c 時間)
        let stage_1_pass = check_0_b_line_breach(
            &wave_a.monowave,
            &wave_b.monowave,
            scenario.initial_direction,
            post_pattern,
            wave_c_dur,
        );
        if !stage_1_pass {
            return PostValidationReport {
                scenario_id: report_id,
                pattern_complete: false,
                pending_conditions: vec![
                    "Ch6 Correction(b<a)Stage 1 未通過:0-B 線突破超時".to_string(),
                ],
            };
        }
        let stage_2_pass = post_pattern.iter().take(wave_c_dur).any(|c| {
            match scenario.initial_direction {
                MonowaveDirection::Up => c.monowave.end_price <= wave_c.monowave.start_price,
                MonowaveDirection::Down => c.monowave.end_price >= wave_c.monowave.start_price,
                _ => false,
            }
        });
        PostValidationReport {
            scenario_id: report_id,
            pattern_complete: true,
            pending_conditions: if stage_2_pass {
                Vec::new()
            } else {
                vec!["Correction Stage 2 deferred:等候完全回測 wave-c".to_string()]
            },
        }
    } else {
        // wave-b > wave-a: Stage 1 = wave-c 在不長於形成時間內被完全回測
        //                   Stage 2 = 突破 0-B 線(≤ wave-c 時間)
        let stage_1_pass = post_pattern.iter().take(wave_c_dur).any(|c| {
            match scenario.initial_direction {
                MonowaveDirection::Up => c.monowave.end_price <= wave_c.monowave.start_price,
                MonowaveDirection::Down => c.monowave.end_price >= wave_c.monowave.start_price,
                _ => false,
            }
        });
        if !stage_1_pass {
            return PostValidationReport {
                scenario_id: report_id,
                pattern_complete: false,
                pending_conditions: vec![
                    "Ch6 Correction(b>a)Stage 1 未通過:wave-c 完全回測超時".to_string(),
                ],
            };
        }
        let stage_2_pass = check_0_b_line_breach(
            &wave_a.monowave,
            &wave_b.monowave,
            scenario.initial_direction,
            post_pattern,
            wave_c_dur,
        );
        PostValidationReport {
            scenario_id: report_id,
            pattern_complete: true,
            pending_conditions: if stage_2_pass {
                Vec::new()
            } else {
                vec!["Correction Stage 2 deferred:等候 0-B 線突破".to_string()]
            },
        }
    }
}

// ---------------------------------------------------------------------------
// Triangle Ch6 兩階段(spec 1787-1797)
// ---------------------------------------------------------------------------

fn validate_triangle(
    scenario: &Scenario,
    classified: &[ClassifiedMonowave],
    post_pattern: &[ClassifiedMonowave],
    sub_kind: TriangleKind,
) -> PostValidationReport {
    let report_id = scenario.id.clone();
    let scenario_monowaves = find_scenario_monowaves(scenario, classified);
    if scenario_monowaves.len() < 5 {
        return PostValidationReport {
            scenario_id: report_id,
            pattern_complete: true,
            pending_conditions: vec!["Triangle 不足 5 monowaves,Ch6 deferred".to_string()],
        };
    }
    let wave_b = &scenario_monowaves[1];
    let wave_d = &scenario_monowaves[3];
    let wave_e = &scenario_monowaves[4];
    let wave_e_dur = wave_e.metrics.duration_bars;

    match sub_kind {
        TriangleKind::Contracting | TriangleKind::Limiting => {
            // Stage 1:走勢突破 b-d 線時間 ≤ wave-e 時間
            let stage_1_pass = check_2_4_line_breach(
                &wave_b.monowave,
                &wave_d.monowave,
                scenario.initial_direction,
                post_pattern,
                wave_e_dur,
            );
            if !stage_1_pass {
                return PostValidationReport {
                    scenario_id: report_id,
                    pattern_complete: false,
                    pending_conditions: vec![
                        "Contracting Triangle Stage 1 未通過:b-d 線突破超時".to_string(),
                    ],
                };
            }
            // Stage 2:Thrust 必超越三角最高/最低價
            //   simplified:post_pattern 任一 end_price 超越 scenario 範圍 max/min
            let range_max = scenario_monowaves
                .iter()
                .map(|c| c.monowave.end_price.max(c.monowave.start_price))
                .fold(f64::NEG_INFINITY, f64::max);
            let range_min = scenario_monowaves
                .iter()
                .map(|c| c.monowave.end_price.min(c.monowave.start_price))
                .fold(f64::INFINITY, f64::min);
            let thrust_present = post_pattern
                .iter()
                .any(|c| c.monowave.end_price > range_max || c.monowave.end_price < range_min);
            PostValidationReport {
                scenario_id: report_id,
                pattern_complete: true,
                pending_conditions: if thrust_present {
                    Vec::new()
                } else {
                    vec!["Contracting Triangle Stage 2 deferred:等 Thrust 突破範圍".to_string()]
                },
            }
        }
        TriangleKind::Expanding => {
            // 非確認:e-wave 完全回測 + 完全回測時間 ≤ e-wave 時間 → Triangle 失敗
            let e_fully_retraced = post_pattern.iter().take(wave_e_dur).any(|c| {
                match scenario.initial_direction {
                    MonowaveDirection::Up => c.monowave.end_price <= wave_e.monowave.start_price,
                    MonowaveDirection::Down => c.monowave.end_price >= wave_e.monowave.start_price,
                    _ => false,
                }
            });
            if e_fully_retraced {
                // 兩條件都不符合 → Triangle 失敗
                PostValidationReport {
                    scenario_id: report_id,
                    pattern_complete: false,
                    pending_conditions: vec![
                        "Expanding Triangle 非確認 fail:e-wave 在 ≤ e-time 內完全回測".to_string(),
                    ],
                }
            } else {
                // 任一條件成立 → Triangle 判讀成立
                PostValidationReport {
                    scenario_id: report_id,
                    pattern_complete: true,
                    pending_conditions: Vec::new(),
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// 找出 scenario 在 classified 中對應的 monowave slice。
fn find_scenario_monowaves<'a>(
    scenario: &Scenario,
    classified: &'a [ClassifiedMonowave],
) -> &'a [ClassifiedMonowave] {
    let start_date = scenario.wave_tree.start;
    let end_date = scenario.wave_tree.end;
    let start_idx = classified
        .iter()
        .position(|c| c.monowave.start_date >= start_date)
        .unwrap_or(0);
    let end_idx = classified
        .iter()
        .rposition(|c| c.monowave.end_date <= end_date)
        .map(|i| i + 1)
        .unwrap_or(classified.len());
    if start_idx < end_idx {
        &classified[start_idx..end_idx]
    } else {
        &[]
    }
}

/// 檢查 post_pattern 是否在 ≤ max_dur 內突破由 (p1.end, p2.end) 構成的 trendline。
///
/// 共用 helper:
///   - Impulse 2-4 線(p1=wave_2, p2=wave_4)
///   - Triangle b-d 線(p1=wave_b, p2=wave_d)
///
/// 「突破」= 在 impulse direction 的逆向超越 trendline 外推值
fn check_2_4_line_breach(
    p1_mw: &crate::output::Monowave,
    p2_mw: &crate::output::Monowave,
    direction: MonowaveDirection,
    post_pattern: &[ClassifiedMonowave],
    max_dur: usize,
) -> bool {
    let t1 = p1_mw.end_date;
    let t2 = p2_mw.end_date;
    let y1 = p1_mw.end_price;
    let y2 = p2_mw.end_price;
    let dt = (t2 - t1).num_days() as f64;
    if dt.abs() < 1e-12 {
        return false;
    }
    let slope = (y2 - y1) / dt;

    let mut elapsed = 0usize;
    for cmw in post_pattern {
        elapsed += cmw.metrics.duration_bars;
        if elapsed > max_dur {
            return false; // 超時未突破
        }
        let dt_now = (cmw.monowave.end_date - t1).num_days() as f64;
        let line_y = y1 + slope * dt_now;
        let breached = match direction {
            MonowaveDirection::Up => cmw.monowave.end_price < line_y,
            MonowaveDirection::Down => cmw.monowave.end_price > line_y,
            _ => false,
        };
        if breached {
            return true;
        }
    }
    false
}

/// 檢查 post_pattern 是否在 ≤ max_dur 內突破 0-B 線(由 wave-a 起點到 wave-b 終點)。
fn check_0_b_line_breach(
    wave_a: &crate::output::Monowave,
    wave_b: &crate::output::Monowave,
    direction: MonowaveDirection,
    post_pattern: &[ClassifiedMonowave],
    max_dur: usize,
) -> bool {
    let t1 = wave_a.start_date;
    let t2 = wave_b.end_date;
    let y1 = wave_a.start_price;
    let y2 = wave_b.end_price;
    let dt = (t2 - t1).num_days() as f64;
    if dt.abs() < 1e-12 {
        return false;
    }
    let slope = (y2 - y1) / dt;

    let mut elapsed = 0usize;
    for cmw in post_pattern {
        elapsed += cmw.metrics.duration_bars;
        if elapsed > max_dur {
            return false;
        }
        let dt_now = (cmw.monowave.end_date - t1).num_days() as f64;
        let line_y = y1 + slope * dt_now;
        // Correction direction 對 0-B 突破的方向定義:依 wave-a direction 判
        let breached = match direction {
            MonowaveDirection::Up => cmw.monowave.end_price < line_y,
            MonowaveDirection::Down => cmw.monowave.end_price > line_y,
            _ => false,
        };
        if breached {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::monowave::ProportionMetrics;
    use crate::output::*;
    use chrono::NaiveDate;

    fn make_cmw(start_p: f64, end_p: f64, dir: MonowaveDirection, dur: usize, day_offset: i64) -> ClassifiedMonowave {
        let base = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        ClassifiedMonowave {
            monowave: Monowave {
                start_date: base + chrono::Duration::days(day_offset),
                end_date: base + chrono::Duration::days(day_offset + dur as i64 - 1),
                start_price: start_p,
                end_price: end_p,
                direction: dir,
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

    fn make_minimal_scenario() -> Scenario {
        let date = NaiveDate::parse_from_str("2026-01-01", "%Y-%m-%d").unwrap();
        Scenario {
            id: "test".to_string(),
            wave_tree: WaveNode {
                label: "test".to_string(),
                start: date,
                end: date,
                children: Vec::new(),
            },
            pattern_type: NeelyPatternType::Impulse,
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
        }
    }

    #[test]
    fn empty_post_pattern_marks_pattern_complete_true_with_pending() {
        let scenario = make_minimal_scenario();
        let report = post_validate(&scenario, &[]);
        assert!(report.pattern_complete);
        assert!(!report.pending_conditions.is_empty());
    }

    #[test]
    fn impulse_stage_1_passes_when_line_breached_in_time() {
        // 5-wave impulse 100→110, 110→105, 105→125, 125→118, 118→132
        // wave_5 dur = 5, post-pattern 應 ≤ 5 天內突破 2-4 線
        let classified = vec![
            make_cmw(100.0, 110.0, MonowaveDirection::Up, 5, 0),    // W1
            make_cmw(110.0, 105.0, MonowaveDirection::Down, 3, 5),  // W2 (end day 7)
            make_cmw(105.0, 125.0, MonowaveDirection::Up, 5, 8),    // W3
            make_cmw(125.0, 118.0, MonowaveDirection::Down, 3, 13), // W4 (end day 15)
            make_cmw(118.0, 132.0, MonowaveDirection::Up, 5, 16),   // W5
            // Post-pattern:大幅下跌穿破 2-4 線(W2 end 105 day 7,W4 end 118 day 15)
            // line at day 22: y = 105 + (118-105)/(15-7) * (22-7) = 105 + 1.625*15 = 129.375
            // post_pattern end_price 100 < 129.375 → breach ✓
            make_cmw(132.0, 100.0, MonowaveDirection::Down, 3, 21),
        ];
        let mut scenario = make_minimal_scenario();
        scenario.wave_tree.start = classified[0].monowave.start_date;
        scenario.wave_tree.end = classified[4].monowave.end_date;

        let report = post_validate(&scenario, &classified);
        assert!(report.pattern_complete, "Impulse 突破 2-4 線應通過 Stage 1");
    }

    #[test]
    fn correction_zigzag_stage_1_passes() {
        // Zigzag 3-wave:wave-a 100→90 (Down dir,initial), wave-b 90→95, wave-c 95→85
        // wave-b mag 5 < wave-a mag 10 → b<a path
        // wave-c dur = 3 → post-pattern 應 ≤ 3 天內突破 0-B 線
        let classified = vec![
            make_cmw(100.0, 90.0, MonowaveDirection::Down, 5, 0),  // wave-a
            make_cmw(90.0, 95.0, MonowaveDirection::Up, 3, 5),     // wave-b (end day 7)
            make_cmw(95.0, 85.0, MonowaveDirection::Down, 3, 8),   // wave-c
            // Post-pattern 大幅反向上漲 → 突破 0-B 線
            // 0-B line: a.start(day 0, price 100) to b.end(day 7, price 95);slope = (95-100)/7 = -0.714
            // line at day 12: 100 + (-0.714)*12 = 91.43
            // post 105 > 91.43 → breach upward
            make_cmw(85.0, 105.0, MonowaveDirection::Up, 2, 11),
        ];
        let mut scenario = make_minimal_scenario();
        scenario.pattern_type = NeelyPatternType::Zigzag {
            sub_kind: ZigzagKind::Single,
        };
        scenario.initial_direction = MonowaveDirection::Down;
        scenario.wave_tree.start = classified[0].monowave.start_date;
        scenario.wave_tree.end = classified[2].monowave.end_date;
        let report = post_validate(&scenario, &classified);
        assert!(report.pattern_complete);
    }

    #[test]
    fn no_post_pattern_keeps_pattern_complete_true_with_pending() {
        // scenario 至序列末 → post_pattern 空 → tentatively true + pending
        let classified = vec![
            make_cmw(100.0, 110.0, MonowaveDirection::Up, 5, 0),
            make_cmw(110.0, 105.0, MonowaveDirection::Down, 3, 5),
            make_cmw(105.0, 125.0, MonowaveDirection::Up, 5, 8),
            make_cmw(125.0, 118.0, MonowaveDirection::Down, 3, 13),
            make_cmw(118.0, 132.0, MonowaveDirection::Up, 5, 16),
        ];
        let mut scenario = make_minimal_scenario();
        scenario.wave_tree.start = classified[0].monowave.start_date;
        scenario.wave_tree.end = classified[4].monowave.end_date;
        let report = post_validate(&scenario, &classified);
        assert!(report.pattern_complete);
        assert!(!report.pending_conditions.is_empty());
    }
}
