// post_validator — Ch6 Post-Constructive 確認閘(ladder 內 per-kind)
//
// 對齊 m3Spec/neely_rules.md §Ch6 Post-Constructive Rules of Logic(1763-1797 行)
//       + m3Spec/neely_ch6_gate_running_fix.md(1.2.0:接回 try_ladder 的介面契約)
//
// 端點泛化(鏡射 G2.2 W5 synth_window 作法):輸入為視窗合成波序列
// (`WaveView.waves`,3/5 段;Combination 7/11 段),不再吃 `&Scenario` —
// Ch6 於視窗接受前評估,Scenario 尚未存在。
//
// **Ch6 規則**(spec 1765-1797):
//   - Impulse Stage 1:後續須在 ≤ wave-5 時間內突破 2-4 線(硬閘)
//   - Impulse Stage 2:依 Extension 類型判回測範圍(wave-2 / wave-4 區)
//   - Correction Stage 1/2(b<a 或 b>a):0-B 線突破 + 完全回測 wave-c(Stage 1 硬閘)
//   - Triangle Contracting/Limiting Stage 1/2:b-d 線突破時間 + Thrust(Stage 1 硬閘)
//   - Triangle Expanding:非確認邏輯 — 只產 Pending,不拒絕(spec 邊界)
//   - Combination / RunningCorrection / Diagonal:無 Stage 1 判準,不拒絕

use crate::monowave::ClassifiedMonowave;
use crate::output::{Ch6Status, MonowaveDirection, NeelyPatternType, RuleId, TriangleKind};

/// 視窗端點視圖(synth_window 產物;不持有 Scenario)。
pub struct WaveView<'a> {
    pub pattern_type: &'a NeelyPatternType,
    pub initial_direction: MonowaveDirection,
    /// 視窗合成波序列(3/5 段;Triangle 5 段;Combination 7/11 段)
    pub waves: &'a [ClassifiedMonowave],
}

/// Ch6 per-kind 評估結果(引擎內部,不上 wire;wire 只出 `Scenario.ch6_status`)。
#[derive(Debug, Clone)]
pub struct Ch6Report {
    /// 僅對「接受的 kind」有意義 — Stage 1 fail 的 kind 由 ladder 拒絕不凍結
    pub status: Ch6Status,
    /// Some(true) = Stage 1 通過(passed_rules += rule_id);
    /// Some(false) = Stage 1 硬閘拒絕;
    /// None = 該形態族無 Stage 1 判準,或資訊不足(Deferred)
    pub stage1_pass: Option<bool>,
    /// 待驗證的後續條件(spec 1788-1797;空 = 已完全確認)
    pub pending_conditions: Vec<String>,
    /// Stage 1 對應的 RuleId。spec 契約寫 `rule_id: RuleId`,此處放寬為
    /// Option — Combination / RunningCorrection / Diagonal 無 Stage 1 規則可引
    pub rule_id: Option<RuleId>,
    /// Stage 1 fail 時的偏離量(超時前累計 bars;RuleRejection.gap 用)
    pub stage1_gap_bars: usize,
}

impl Ch6Report {
    pub fn deferred(note: &str) -> Self {
        Ch6Report {
            status: Ch6Status::Deferred,
            stage1_pass: None,
            pending_conditions: vec![note.to_string()],
            rule_id: None,
            stage1_gap_bars: 0,
        }
    }

    fn stage1_rejected(rule_id: RuleId, msg: String, gap_bars: usize) -> Self {
        Ch6Report {
            status: Ch6Status::Pending, // 被拒 kind 不凍結,status 不上 wire
            stage1_pass: Some(false),
            pending_conditions: vec![msg],
            rule_id: Some(rule_id),
            stage1_gap_bars: gap_bars,
        }
    }

    fn accepted(rule_id: Option<RuleId>, stage1: Option<bool>, pending: Vec<String>) -> Self {
        Ch6Report {
            status: if pending.is_empty() {
                Ch6Status::Confirmed
            } else {
                Ch6Status::Pending
            },
            stage1_pass: stage1,
            pending_conditions: pending,
            rule_id,
            stage1_gap_bars: 0,
        }
    }
}

/// Ch6 兩階段確認(ladder 視窗版)。
///
/// `post_pattern` = 全域 classified 中 `start_bar > window.last().end_bar`
/// 的葉序列(caller 於 try_ladder 切片;空 = live edge → Deferred)。
pub fn post_validate_window(view: &WaveView, post_pattern: &[ClassifiedMonowave]) -> Ch6Report {
    if post_pattern.is_empty() {
        return Ch6Report::deferred("post-pattern 葉為空(live edge),Ch6 兩階段確認 deferred");
    }

    match view.pattern_type {
        NeelyPatternType::Impulse => validate_impulse(view, post_pattern),
        NeelyPatternType::Diagonal { .. } => validate_terminal_impulse(view, post_pattern),
        NeelyPatternType::Triangle { sub_kind } => {
            validate_triangle(view, post_pattern, *sub_kind)
        }
        NeelyPatternType::Zigzag { .. } | NeelyPatternType::Flat { .. } => {
            validate_correction(view, post_pattern)
        }
        NeelyPatternType::Combination { sub_kinds } => {
            // DoubleThree* / TripleThree* → 末段預期 Flat/Triangle(spec 1862-1869);
            // 無 Stage 1 判準 → 恆 Pending,不拒絕(spec 邊界)
            let pending: Vec<String> = sub_kinds
                .iter()
                .map(|k| {
                    use crate::output::CombinationKind;
                    match k {
                        CombinationKind::DoubleThree
                        | CombinationKind::DoubleThreeCombination
                        | CombinationKind::DoubleThreeRunning => format!(
                            "{:?}:末段預期 Flat/Triangle(spec 1862-1869 Ch8 X-wave 連結)",
                            k
                        ),
                        CombinationKind::TripleThree
                        | CombinationKind::TripleThreeCombination
                        | CombinationKind::TripleThreeRunning => format!(
                            "{:?}:三段 X-wave 串接,末段預期 Triangle(spec Ch8 Multiwave)",
                            k
                        ),
                        _ => format!("{:?}:末段預期 corrective(spec Ch8)", k),
                    }
                })
                .collect();
            Ch6Report::accepted(None, None, pending)
        }
        NeelyPatternType::RunningCorrection => {
            // spec 2024-2037:後續必為延伸 Impulse;無 Stage 1 判準 → 恆 Pending
            Ch6Report::accepted(
                None,
                None,
                vec!["後續預期延伸 Impulse > 161.8% × 同向 Impulse(spec 2024-2037)".to_string()],
            )
        }
    }
}

// ---------------------------------------------------------------------------
// Impulse(Trending)Ch6 兩階段確認(spec 1765-1775)
// ---------------------------------------------------------------------------

fn validate_impulse(view: &WaveView, post_pattern: &[ClassifiedMonowave]) -> Ch6Report {
    let waves = view.waves;
    if waves.len() < 5 {
        return Ch6Report::deferred("視窗不足 5 段,Impulse Ch6 deferred");
    }
    let wave_2 = &waves[1];
    let wave_4 = &waves[3];
    let wave_5 = &waves[4];
    let wave_5_dur = wave_5.metrics.duration_bars;

    // Stage 1:後續走勢須在 ≤ wave-5 時間內突破 2-4 線
    let check = check_2_4_line_breach(
        &wave_2.monowave,
        &wave_4.monowave,
        view.initial_direction,
        post_pattern,
        wave_5_dur,
    );
    if !check.breached {
        // Spec 1769:耗時更長 → wave-5 變為 Terminal 或 wave-4 未完
        return Ch6Report::stage1_rejected(
            RuleId::Ch6_Impulse_Stage1,
            "Ch6 Impulse Stage 1 未通過:2-4 線突破時間超過 wave-5 時間".to_string(),
            check.elapsed_bars,
        );
    }

    // Stage 2:依 Extension 類型判回測範圍(spec 1771-1775;不拒絕)
    let pending = match validate_impulse_stage_2(view, post_pattern) {
        Stage2Result::Pass => Vec::new(),
        Stage2Result::Pending(msg) => vec![msg],
    };
    Ch6Report::accepted(Some(RuleId::Ch6_Impulse_Stage1), Some(true), pending)
}

enum Stage2Result {
    Pass,
    Pending(String),
}

fn validate_impulse_stage_2(view: &WaveView, post_pattern: &[ClassifiedMonowave]) -> Stage2Result {
    let waves = view.waves;
    // Extension 判定(已在 wave_rules 用過):最長者為 Extension
    let mag_w1 = waves[0].metrics.magnitude;
    let mag_w3 = waves[2].metrics.magnitude;
    let mag_w5 = waves[4].metrics.magnitude;

    let max_mag = mag_w1.max(mag_w3).max(mag_w5);
    let ext_position = if (mag_w1 - max_mag).abs() < 1e-9 {
        1
    } else if (mag_w3 - max_mag).abs() < 1e-9 {
        3
    } else {
        5
    };

    // 後續走勢「整體回測量」 — 用 post_pattern 對視窗範圍的最大反向 movement
    let scenario_end_price = waves[4].monowave.end_price;
    let wave_4_end_price = waves[3].monowave.end_price;

    let max_retrace_price = post_pattern
        .iter()
        .map(|c| c.monowave.end_price)
        .fold(scenario_end_price, |acc, p| match view.initial_direction {
            MonowaveDirection::Up => acc.min(p),
            MonowaveDirection::Down => acc.max(p),
            _ => acc,
        });

    // 「回到 wave-4 區」= retrace 達到 wave_4_end_price
    let reached_wave_4_zone = match view.initial_direction {
        MonowaveDirection::Up => max_retrace_price <= wave_4_end_price,
        MonowaveDirection::Down => max_retrace_price >= wave_4_end_price,
        _ => false,
    };

    match ext_position {
        1 | 3 => {
            // 1st / 3rd Wave Extended:後續必回 wave-4 區
            if reached_wave_4_zone {
                Stage2Result::Pass
            } else {
                Stage2Result::Pending(format!(
                    "{} Ext Impulse Stage 2 deferred:等候後續回到 wave-4 區",
                    if ext_position == 1 { "1st" } else { "3rd" }
                ))
            }
        }
        _ => {
            // 5th Wave Extended:必至少回測 61.8% × wave-5
            let retrace_required = mag_w5 * 0.618;
            let retrace_amount = (scenario_end_price - max_retrace_price).abs();
            if retrace_amount >= retrace_required {
                Stage2Result::Pass
            } else {
                Stage2Result::Pending(
                    "5th Ext Impulse Stage 2 deferred:等候 ≥ 61.8% × wave-5 回測".to_string(),
                )
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Terminal Impulse(Diagonal)Ch6:後續走勢須完全回測整段 Terminal(spec 2056)
// 無 Stage 1 時限硬閘 → 不拒絕
// ---------------------------------------------------------------------------

fn validate_terminal_impulse(view: &WaveView, post_pattern: &[ClassifiedMonowave]) -> Ch6Report {
    let Some(first) = view.waves.first() else {
        return Ch6Report::deferred("視窗為空,Terminal Ch6 deferred");
    };
    let start_price = first.monowave.start_price;

    let fully_retraced = post_pattern.iter().any(|c| match view.initial_direction {
        MonowaveDirection::Up => c.monowave.end_price <= start_price,
        MonowaveDirection::Down => c.monowave.end_price >= start_price,
        _ => false,
    });

    if fully_retraced {
        Ch6Report::accepted(None, None, Vec::new())
    } else {
        Ch6Report::accepted(
            None,
            None,
            vec!["等候後續 100% 回測整段 Terminal(spec 2056)".to_string()],
        )
    }
}

// ---------------------------------------------------------------------------
// Correction(Zigzag/Flat)Ch6 兩階段(spec 1777-1785)
// ---------------------------------------------------------------------------

fn validate_correction(view: &WaveView, post_pattern: &[ClassifiedMonowave]) -> Ch6Report {
    let waves = view.waves;
    if waves.len() < 3 {
        return Ch6Report::deferred("視窗不足 3 段,Correction Ch6 deferred");
    }
    let wave_a = &waves[0];
    let wave_b = &waves[1];
    let wave_c = &waves[2];
    let mag_a = wave_a.metrics.magnitude;
    let mag_b = wave_b.metrics.magnitude;
    let wave_c_dur = wave_c.metrics.duration_bars;

    if mag_b < mag_a {
        // wave-b < wave-a: Stage 1 = 突破 0-B 線(≤ wave-c 時間;硬閘)
        //                   Stage 2 = 完全回測 wave-c(≤ wave-c 時間)
        let check = check_0_b_line_breach(
            &wave_a.monowave,
            &wave_b.monowave,
            view.initial_direction,
            post_pattern,
            wave_c_dur,
        );
        if !check.breached {
            return Ch6Report::stage1_rejected(
                RuleId::Ch6_Correction_BSmall_Stage1,
                "Ch6 Correction(b<a)Stage 1 未通過:0-B 線突破超時".to_string(),
                check.elapsed_bars,
            );
        }
        let stage_2 = full_retrace_within(
            wave_c.monowave.start_price,
            view.initial_direction,
            post_pattern,
            wave_c_dur,
        );
        let pending = if stage_2.breached {
            Vec::new()
        } else {
            vec!["Correction Stage 2 deferred:等候完全回測 wave-c".to_string()]
        };
        Ch6Report::accepted(Some(RuleId::Ch6_Correction_BSmall_Stage1), Some(true), pending)
    } else {
        // wave-b > wave-a: Stage 1 = wave-c 在不長於形成時間內被完全回測(硬閘)
        //                   Stage 2 = 突破 0-B 線(≤ wave-c 時間)
        let check = full_retrace_within(
            wave_c.monowave.start_price,
            view.initial_direction,
            post_pattern,
            wave_c_dur,
        );
        if !check.breached {
            return Ch6Report::stage1_rejected(
                RuleId::Ch6_Correction_BLarge_Stage1,
                "Ch6 Correction(b>a)Stage 1 未通過:wave-c 完全回測超時".to_string(),
                check.elapsed_bars,
            );
        }
        let stage_2 = check_0_b_line_breach(
            &wave_a.monowave,
            &wave_b.monowave,
            view.initial_direction,
            post_pattern,
            wave_c_dur,
        );
        let pending = if stage_2.breached {
            Vec::new()
        } else {
            vec!["Correction Stage 2 deferred:等候 0-B 線突破".to_string()]
        };
        Ch6Report::accepted(Some(RuleId::Ch6_Correction_BLarge_Stage1), Some(true), pending)
    }
}

// ---------------------------------------------------------------------------
// Triangle Ch6 兩階段(spec 1787-1797)
// ---------------------------------------------------------------------------

fn validate_triangle(
    view: &WaveView,
    post_pattern: &[ClassifiedMonowave],
    sub_kind: TriangleKind,
) -> Ch6Report {
    let waves = view.waves;
    if waves.len() < 5 {
        return Ch6Report::deferred("Triangle 視窗不足 5 段,Ch6 deferred");
    }
    let wave_b = &waves[1];
    let wave_d = &waves[3];
    let wave_e = &waves[4];
    let wave_e_dur = wave_e.metrics.duration_bars;

    match sub_kind {
        TriangleKind::Contracting | TriangleKind::Limiting => {
            // Stage 1:走勢突破 b-d 線時間 ≤ wave-e 時間(硬閘)
            let check = check_2_4_line_breach(
                &wave_b.monowave,
                &wave_d.monowave,
                view.initial_direction,
                post_pattern,
                wave_e_dur,
            );
            if !check.breached {
                return Ch6Report::stage1_rejected(
                    RuleId::Ch6_Triangle_Contracting_Stage1,
                    "Contracting Triangle Stage 1 未通過:b-d 線突破超時".to_string(),
                    check.elapsed_bars,
                );
            }
            // Stage 2:Thrust 必超越三角最高/最低價
            let range_max = waves
                .iter()
                .map(|c| c.monowave.end_price.max(c.monowave.start_price))
                .fold(f64::NEG_INFINITY, f64::max);
            let range_min = waves
                .iter()
                .map(|c| c.monowave.end_price.min(c.monowave.start_price))
                .fold(f64::INFINITY, f64::min);
            let thrust_present = post_pattern
                .iter()
                .any(|c| c.monowave.end_price > range_max || c.monowave.end_price < range_min);
            let pending = if thrust_present {
                Vec::new()
            } else {
                vec!["Contracting Triangle Stage 2 deferred:等 Thrust 突破範圍".to_string()]
            };
            Ch6Report::accepted(
                Some(RuleId::Ch6_Triangle_Contracting_Stage1),
                Some(true),
                pending,
            )
        }
        TriangleKind::Expanding => {
            // 非確認邏輯:e-wave 於 ≤ e-time 內完全回測 = Triangle 判讀受創。
            // spec 邊界拍板:只產 Pending,不拒絕(判讀者權衡,引擎不硬閘)
            let e_retraced = full_retrace_within(
                wave_e.monowave.start_price,
                view.initial_direction,
                post_pattern,
                wave_e_dur,
            );
            if e_retraced.breached {
                Ch6Report {
                    status: Ch6Status::Pending,
                    stage1_pass: None,
                    pending_conditions: vec![
                        "Expanding Triangle 非確認條件觸發:e-wave 於 ≤ e-time 內完全回測"
                            .to_string(),
                    ],
                    rule_id: Some(RuleId::Ch6_Triangle_Expanding_NonConfirmation),
                    stage1_gap_bars: 0,
                }
            } else {
                Ch6Report::accepted(None, None, Vec::new())
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// 突破檢查結果:`elapsed_bars` = 判定終止前累計的 post-pattern bars
/// (超時 / 序列耗盡未突破時 = RuleRejection.gap 的「超時 bars」語意)。
struct BreachCheck {
    breached: bool,
    elapsed_bars: usize,
}

/// 檢查 post_pattern 是否在 ≤ max_dur bars 內突破由 (p1.end, p2.end) 構成的 trendline。
///
/// 共用:Impulse 2-4 線(p1=wave_2, p2=wave_4)/ Triangle b-d 線(p1=wave_b, p2=wave_d)。
/// 「突破」= 在 pattern direction 的逆向超越 trendline 外推值。
fn check_2_4_line_breach(
    p1_mw: &crate::output::Monowave,
    p2_mw: &crate::output::Monowave,
    direction: MonowaveDirection,
    post_pattern: &[ClassifiedMonowave],
    max_dur: usize,
) -> BreachCheck {
    let t1 = p1_mw.end_date;
    let t2 = p2_mw.end_date;
    let y1 = p1_mw.end_price;
    let y2 = p2_mw.end_price;
    let dt = (t2 - t1).num_days() as f64;
    if dt.abs() < 1e-12 {
        return BreachCheck { breached: false, elapsed_bars: 0 };
    }
    let slope = (y2 - y1) / dt;

    let mut elapsed = 0usize;
    for cmw in post_pattern {
        elapsed += cmw.metrics.duration_bars;
        if elapsed > max_dur {
            return BreachCheck { breached: false, elapsed_bars: elapsed }; // 超時未突破
        }
        let dt_now = (cmw.monowave.end_date - t1).num_days() as f64;
        let line_y = y1 + slope * dt_now;
        let breached = match direction {
            MonowaveDirection::Up => cmw.monowave.end_price < line_y,
            MonowaveDirection::Down => cmw.monowave.end_price > line_y,
            _ => false,
        };
        if breached {
            return BreachCheck { breached: true, elapsed_bars: elapsed };
        }
    }
    BreachCheck { breached: false, elapsed_bars: elapsed }
}

/// 檢查 post_pattern 是否在 ≤ max_dur bars 內突破 0-B 線(wave-a 起點到 wave-b 終點)。
fn check_0_b_line_breach(
    wave_a: &crate::output::Monowave,
    wave_b: &crate::output::Monowave,
    direction: MonowaveDirection,
    post_pattern: &[ClassifiedMonowave],
    max_dur: usize,
) -> BreachCheck {
    let t1 = wave_a.start_date;
    let t2 = wave_b.end_date;
    let y1 = wave_a.start_price;
    let y2 = wave_b.end_price;
    let dt = (t2 - t1).num_days() as f64;
    if dt.abs() < 1e-12 {
        return BreachCheck { breached: false, elapsed_bars: 0 };
    }
    let slope = (y2 - y1) / dt;

    let mut elapsed = 0usize;
    for cmw in post_pattern {
        elapsed += cmw.metrics.duration_bars;
        if elapsed > max_dur {
            return BreachCheck { breached: false, elapsed_bars: elapsed };
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
            return BreachCheck { breached: true, elapsed_bars: elapsed };
        }
    }
    BreachCheck { breached: false, elapsed_bars: elapsed }
}

/// 檢查 post_pattern 是否在 ≤ max_dur bars 內「完全回測」至 reference_price
/// (Up pattern → end_price ≤ reference;Down → ≥)。時間以 bars 累計,
/// 與 check_*_line_breach 同基準(舊 Scenario 版以 monowave 個數 take(n) 計,
/// 「不長於形成時間」的時間語意應為 bars)。
fn full_retrace_within(
    reference_price: f64,
    direction: MonowaveDirection,
    post_pattern: &[ClassifiedMonowave],
    max_dur: usize,
) -> BreachCheck {
    let mut elapsed = 0usize;
    for cmw in post_pattern {
        elapsed += cmw.metrics.duration_bars;
        if elapsed > max_dur {
            return BreachCheck { breached: false, elapsed_bars: elapsed };
        }
        let reached = match direction {
            MonowaveDirection::Up => cmw.monowave.end_price <= reference_price,
            MonowaveDirection::Down => cmw.monowave.end_price >= reference_price,
            _ => false,
        };
        if reached {
            return BreachCheck { breached: true, elapsed_bars: elapsed };
        }
    }
    BreachCheck { breached: false, elapsed_bars: elapsed }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::monowave::ProportionMetrics;
    use crate::output::*;
    use chrono::NaiveDate;

    fn make_cmw(
        start_p: f64,
        end_p: f64,
        dir: MonowaveDirection,
        dur: usize,
        day_offset: i64,
    ) -> ClassifiedMonowave {
        let base = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        ClassifiedMonowave {
            monowave: Monowave {
                start_date: base + chrono::Duration::days(day_offset),
                end_date: base + chrono::Duration::days(day_offset + dur as i64 - 1),
                start_price: start_p,
                end_price: end_p,
                direction: dir,
                bar_indices: (0, 0),
            },
            atr_at_start: 1.0,
            metrics: ProportionMetrics {
                magnitude: (end_p - start_p).abs(),
                duration_bars: dur,
                atr_relative: 1.0,
                slope_vs_45deg: 1.0,
            },
            structure_label_candidates: Vec::new(),
            polywave_size: 0,
        }
    }

    fn impulse_waves() -> Vec<ClassifiedMonowave> {
        // 5-wave impulse 100→110, 110→105, 105→125, 125→118, 118→132
        vec![
            make_cmw(100.0, 110.0, MonowaveDirection::Up, 5, 0),    // W1
            make_cmw(110.0, 105.0, MonowaveDirection::Down, 3, 5),  // W2 (end day 7)
            make_cmw(105.0, 125.0, MonowaveDirection::Up, 5, 8),    // W3
            make_cmw(125.0, 118.0, MonowaveDirection::Down, 3, 13), // W4 (end day 15)
            make_cmw(118.0, 132.0, MonowaveDirection::Up, 5, 16),   // W5
        ]
    }

    #[test]
    fn empty_post_pattern_is_deferred() {
        let waves = impulse_waves();
        let pt = NeelyPatternType::Impulse;
        let view = WaveView {
            pattern_type: &pt,
            initial_direction: MonowaveDirection::Up,
            waves: &waves,
        };
        let report = post_validate_window(&view, &[]);
        assert_eq!(report.status, Ch6Status::Deferred);
        assert_eq!(report.stage1_pass, None);
        assert!(!report.pending_conditions.is_empty());
    }

    #[test]
    fn impulse_stage_1_passes_when_line_breached_in_time() {
        let waves = impulse_waves();
        // Post-pattern:大幅下跌穿破 2-4 線(W2 end 105 day 7,W4 end 118 day 15)
        // line at day 23: y = 105 + (118-105)/(15-7) * (23-7) = 131;post end 100 < 131 → breach
        let post = vec![make_cmw(132.0, 100.0, MonowaveDirection::Down, 3, 21)];
        let pt = NeelyPatternType::Impulse;
        let view = WaveView {
            pattern_type: &pt,
            initial_direction: MonowaveDirection::Up,
            waves: &waves,
        };
        let report = post_validate_window(&view, &post);
        assert_eq!(report.stage1_pass, Some(true), "Impulse 突破 2-4 線應通過 Stage 1");
        assert_eq!(report.rule_id, Some(RuleId::Ch6_Impulse_Stage1));
    }

    #[test]
    fn impulse_stage_1_rejects_on_timeout() {
        let waves = impulse_waves();
        // W5 dur = 5;post-pattern 6 bars 貼著 2-4 線上方盤整,無突破 → 超時
        let post = vec![
            make_cmw(132.0, 131.5, MonowaveDirection::Down, 3, 21),
            make_cmw(131.5, 133.0, MonowaveDirection::Up, 3, 24),
        ];
        let pt = NeelyPatternType::Impulse;
        let view = WaveView {
            pattern_type: &pt,
            initial_direction: MonowaveDirection::Up,
            waves: &waves,
        };
        let report = post_validate_window(&view, &post);
        assert_eq!(report.stage1_pass, Some(false));
        assert_eq!(report.rule_id, Some(RuleId::Ch6_Impulse_Stage1));
        assert!(report.stage1_gap_bars > 5, "gap 應記錄超時 bars");
    }

    #[test]
    fn correction_zigzag_stage_1_passes() {
        // Zigzag 3-wave:wave-a 100→90(Down initial), wave-b 90→95, wave-c 95→85
        // wave-b mag 5 < wave-a mag 10 → b<a path;wave-c dur = 3
        let waves = vec![
            make_cmw(100.0, 90.0, MonowaveDirection::Down, 5, 0), // wave-a
            make_cmw(90.0, 95.0, MonowaveDirection::Up, 3, 5),    // wave-b (end day 7)
            make_cmw(95.0, 85.0, MonowaveDirection::Down, 3, 8),  // wave-c
        ];
        // 0-B line: a.start(day 0, 100) → b.end(day 7, 95);slope ≈ -0.714
        // line at day 12 ≈ 91.4;post 105 > 91.4 → breach upward(Down pattern)
        let post = vec![make_cmw(85.0, 105.0, MonowaveDirection::Up, 2, 11)];
        let pt = NeelyPatternType::Zigzag { sub_kind: ZigzagKind::Single };
        let view = WaveView {
            pattern_type: &pt,
            initial_direction: MonowaveDirection::Down,
            waves: &waves,
        };
        let report = post_validate_window(&view, &post);
        assert_eq!(report.stage1_pass, Some(true));
        assert_eq!(report.rule_id, Some(RuleId::Ch6_Correction_BSmall_Stage1));
    }

    #[test]
    fn combination_and_running_never_reject() {
        let waves = impulse_waves();
        let post = vec![make_cmw(132.0, 128.0, MonowaveDirection::Down, 2, 21)];
        for pt in [
            NeelyPatternType::Combination {
                sub_kinds: vec![CombinationKind::DoubleThree],
            },
            NeelyPatternType::RunningCorrection,
        ] {
            let view = WaveView {
                pattern_type: &pt,
                initial_direction: MonowaveDirection::Up,
                waves: &waves,
            };
            let report = post_validate_window(&view, &post);
            assert_eq!(report.status, Ch6Status::Pending);
            assert_eq!(report.stage1_pass, None);
            assert_eq!(report.rule_id, None);
        }
    }
}
