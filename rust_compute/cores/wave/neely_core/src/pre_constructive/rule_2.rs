// rule_2.rs — Rule 2(38.2% ≤ m2 < 61.8% m1)所有 Condition 與 branch
//
// 對齊 m3Spec/neely_rules.md §Rule 2(577-630 行)。
// Headline:`{:5/(:sL3)/[:c3]/[:s5]}`

use super::context::MonowaveContext;
use super::predicates::*;
use crate::output::{Certainty, StructureLabel, StructureLabelCandidate};

pub fn run(ctx: &MonowaveContext, cands: &mut Vec<StructureLabelCandidate>) {
    let Some(m0) = ctx.m0 else { return };
    let ratio = mag_ratio(m0, ctx.m1);

    if ratio < FIB_382 {
        cond_2a(ctx, cands);
    } else if ratio < FIB_618 {
        cond_2b(ctx, cands);
    } else if ratio < FIB_1000 {
        cond_2c(ctx, cands);
    } else if ratio <= FIB_1618 {
        cond_2d(ctx, cands);
    } else {
        cond_2e(ctx, cands);
    }
}

// ---------------------------------------------------------------------------
// Condition 2a — m0 < 38.2% m1(6 branches)
// ---------------------------------------------------------------------------

fn cond_2a(ctx: &MonowaveContext, cands: &mut Vec<StructureLabelCandidate>) {
    let m1 = ctx.m1;
    let Some(m0) = ctx.m0 else { return };

    // Branch 1:(無前置)→ add :5;m4 不超出 m0 終點 → add :s5 + m2 終點加 x:c3?
    add_or_promote(cands, StructureLabel::Five, Certainty::Primary);
    if let Some(m4) = ctx.m4 {
        let m4_within = match m1.monowave.direction {
            crate::output::MonowaveDirection::Up => m4.monowave.end_price <= m0.monowave.end_price,
            crate::output::MonowaveDirection::Down => {
                m4.monowave.end_price >= m0.monowave.end_price
            }
            _ => false,
        };
        if m4_within {
            add_or_promote(cands, StructureLabel::S5, Certainty::Primary);
            add_or_promote(cands, StructureLabel::XC3, Certainty::MissingWaveBundle);
        }
    }

    // Branch 2:m0 polywave + m1 完全回測 m0 → 記(diagnostic,本 PR no-op)

    // Branch 3:m1 不為 m(-1)/m1/m3 中最短 AND 最長 ≈(或 >) 161.8% × 次長
    //          AND m3 被回測 ≥ 61.8%
    //   → 「市場可能形成 Impulse with m1 為 wave-3」(暗示但無直接 candidate;標 c3 為 Possible)
    if let (Some(m_minus_1), Some(m3), Some(m4)) = (ctx.m_minus_1, ctx.m3, ctx.m4) {
        let m1_not_shortest = !is_shortest_of_three(m1, Some(m_minus_1), Some(m3));
        // 最長 ≈ 或 > 161.8% × 次長
        let mut mags = [magnitude(m_minus_1), magnitude(m1), magnitude(m3)];
        mags.sort_by(|a, b| b.partial_cmp(a).unwrap());
        let ratio = if mags[1] > 1e-12 { mags[0] / mags[1] } else { 0.0 };
        let dominant_ratio = ratio >= FIB_1618 * (1.0 - FIB_TOL);
        let m3_retraced = retracement_pct(&m3.monowave, &m4.monowave) >= FIB_618;
        if m1_not_shortest && dominant_ratio && m3_retraced {
            // m1 wave-3 暗示 → C3 not directly; 留 diagnostic
            // 為 alignment 起見,加 SL3 Rare(Wave-3 Terminal 暗示)
            add_or_promote(cands, StructureLabel::SL3, Certainty::Rare);
        }
    }

    // Branch 4:m0 ≈ m2(價/時 或 61.8%)AND m(-1) ≥ 161.8% m1
    //          AND m3(及後續)在 ≤ m(-1) 時間內 ≥ m(-1) 價長
    //   → add [:c3](Running Correction)
    if let (Some(m_minus_1), Some(m2), Some(m3)) = (ctx.m_minus_1, ctx.m2, ctx.m3) {
        let m0_m2_similar = approx_equal(magnitude(m0), magnitude(m2))
            || approx_equal(duration(m0) as f64, duration(m2) as f64)
            || approx_equal(magnitude(m0), magnitude(m2) * FIB_618);
        let m_neg_1_long = mag_ratio(m_minus_1, m1) >= FIB_1618;
        let combined_mag = magnitude(m3) + ctx.m4.map_or(0.0, magnitude) + ctx.m5.map_or(0.0, magnitude);
        let combined_dur = duration(m3) + ctx.m4.map_or(0, duration) + ctx.m5.map_or(0, duration);
        if m0_m2_similar
            && m_neg_1_long
            && combined_mag >= magnitude(m_minus_1)
            && combined_dur <= duration(m_minus_1)
        {
            add_or_promote(cands, StructureLabel::C3, Certainty::Rare);
        }
    }

    // Branch 5:m0 ≈ m2(時相等)AND m3 < 161.8% m1 AND m(-1) > m0
    //   → x-wave 三處之一(missing-wave bundle)
    if let (Some(m_minus_1), Some(m2), Some(m3)) = (ctx.m_minus_1, ctx.m2, ctx.m3) {
        let m0_m2_dur_similar = approx_equal(duration(m0) as f64, duration(m2) as f64);
        let m3_under = mag_ratio(m3, m1) < FIB_1618;
        let m_neg_1_gt_m0 = magnitude(m_minus_1) > magnitude(m0);
        if m0_m2_dur_similar && m3_under && m_neg_1_gt_m0 {
            add_or_promote(cands, StructureLabel::XC3, Certainty::MissingWaveBundle);
        }
    }

    // Branch 6:m(-1) > m0 AND m0 < m1 AND m1 不為 m(-1)/m1/m3 最短
    //          AND m3(plus one time unit)被完全回測
    //   → add :c3(Terminal wave-3 完成於 m1)
    if let (Some(m_minus_1), Some(m3), Some(m4)) = (ctx.m_minus_1, ctx.m3, ctx.m4) {
        if magnitude(m_minus_1) > magnitude(m0)
            && magnitude(m0) < magnitude(m1)
            && !is_shortest_of_three(m1, Some(m_minus_1), Some(m3))
            && completely_retraced_plus_one_time_unit(m3, m4)
        {
            add_or_promote(cands, StructureLabel::C3, Certainty::Primary);
        }
    }
}

// ---------------------------------------------------------------------------
// Condition 2b — 38.2% ≤ m0 < 61.8% m1(6 branches)
// ---------------------------------------------------------------------------

fn cond_2b(ctx: &MonowaveContext, cands: &mut Vec<StructureLabelCandidate>) {
    let m1 = ctx.m1;
    let Some(m0) = ctx.m0 else { return };

    // Branch 1:(無前置)→ add :5;m4 不超出 m0 終點 → add :s5 + m2 終點加 x:c3?
    add_or_promote(cands, StructureLabel::Five, Certainty::Primary);
    if let Some(m4) = ctx.m4 {
        let m4_within = match m1.monowave.direction {
            crate::output::MonowaveDirection::Up => m4.monowave.end_price <= m0.monowave.end_price,
            crate::output::MonowaveDirection::Down => {
                m4.monowave.end_price >= m0.monowave.end_price
            }
            _ => false,
        };
        if m4_within {
            add_or_promote(cands, StructureLabel::S5, Certainty::Primary);
            add_or_promote(cands, StructureLabel::XC3, Certainty::MissingWaveBundle);
        }
    }

    // Branch 2:m0 polywave AND m1 完全回測 m0 → 記(diagnostic)

    // Branch 3:m0 ≈ m2 AND m(-1) ≥ 161.8% m1 AND m3...≥ m(-1) → add [:c3]
    if let (Some(m_minus_1), Some(m2), Some(m3)) = (ctx.m_minus_1, ctx.m2, ctx.m3) {
        let m0_m2_similar = approx_equal(magnitude(m0), magnitude(m2))
            || approx_equal(magnitude(m0), magnitude(m2) * FIB_618);
        let m_neg_1_long = mag_ratio(m_minus_1, m1) >= FIB_1618;
        let combined_mag = magnitude(m3) + ctx.m4.map_or(0.0, magnitude) + ctx.m5.map_or(0.0, magnitude);
        let combined_dur = duration(m3) + ctx.m4.map_or(0, duration) + ctx.m5.map_or(0, duration);
        if m0_m2_similar
            && m_neg_1_long
            && combined_mag >= magnitude(m_minus_1)
            && combined_dur <= duration(m_minus_1)
        {
            add_or_promote(cands, StructureLabel::C3, Certainty::Rare);
        }
    }

    // Branch 4:m0 ≈ m2 AND m3 < 161.8% m1 AND m3 在自身時間內被完全回測
    //   → x-wave 三處之一(missing-wave bundle)
    if let (Some(m2), Some(m3), Some(m4)) = (ctx.m2, ctx.m3, ctx.m4) {
        let m0_m2_similar = approx_equal(magnitude(m0), magnitude(m2));
        let m3_under = mag_ratio(m3, m1) < FIB_1618;
        let m3_retraced = completely_retraced_within_time(m3, m4);
        if m0_m2_similar && m3_under && m3_retraced {
            add_or_promote(cands, StructureLabel::XC3, Certainty::MissingWaveBundle);
        }
    }

    // Branch 5:m3 < 61.8% m1 → 提高「x-wave 隱藏在 m1 中心」機率(升 cert)
    if let Some(m3) = ctx.m3 {
        if mag_ratio(m3, m1) < FIB_618 {
            change_certainty(cands, StructureLabel::XC3, Certainty::Possible);
        }
    }

    // Branch 6:m2 部分價區與 m0 共享 AND m0 與 m2 時間差 ≥ 61.8%
    //          AND m1 不為 m1/m3/m(-1) 中最短 AND 自 m3 終點起市場快速回到 m1 起點
    //   → add :c3(Terminal wave-3 完成)
    if let (Some(m_minus_1), Some(m2), Some(m3), Some(m4)) =
        (ctx.m_minus_1, ctx.m2, ctx.m3, ctx.m4)
    {
        let share = share_price_range(&m2.monowave, &m0.monowave);
        let m0_m2_time_diff = (duration(m0) as f64 - duration(m2) as f64).abs()
            >= duration(m1) as f64 * FIB_618;
        let m1_not_shortest = !is_shortest_of_three(m1, Some(m_minus_1), Some(m3));
        // 市場快速回到 m1 起點(用 m4 對 m_minus_1 (= m1 起點之前)的回測代理)
        let market_returns = retracement_pct(&m1.monowave, &m4.monowave) >= 1.0;
        if share && m0_m2_time_diff && m1_not_shortest && market_returns {
            add_or_promote(cands, StructureLabel::C3, Certainty::Primary);
        }
    }
}

// ---------------------------------------------------------------------------
// Condition 2c — 61.8% ≤ m0 < 100% m1(7 branches)
// ---------------------------------------------------------------------------

fn cond_2c(ctx: &MonowaveContext, cands: &mut Vec<StructureLabelCandidate>) {
    let m1 = ctx.m1;
    let Some(m0) = ctx.m0 else { return };

    // Branch 1:(必置)→ add :5;m4 不超出 m0 終點 → add :s5 + x:c3?
    add_or_promote(cands, StructureLabel::Five, Certainty::Primary);
    if let Some(m4) = ctx.m4 {
        let m4_within = match m1.monowave.direction {
            crate::output::MonowaveDirection::Up => m4.monowave.end_price <= m0.monowave.end_price,
            crate::output::MonowaveDirection::Down => {
                m4.monowave.end_price >= m0.monowave.end_price
            }
            _ => false,
        };
        if m4_within {
            add_or_promote(cands, StructureLabel::S5, Certainty::Primary);
            add_or_promote(cands, StructureLabel::XC3, Certainty::MissingWaveBundle);
        }
    }

    // Branch 2:x-wave 在 m0 場景成立:m(-2) < m(-1) AND m(-4) > m(-3)
    //   → 已透過 missing-wave bundle 隱含,本 branch 強化 cert
    if let (Some(m_minus_1), Some(m_minus_2)) = (ctx.m_minus_1, ctx.m_minus_2) {
        let cond_a = magnitude(m_minus_2) < magnitude(m_minus_1);
        // m(-4) 在 m_minus_3 之前,我們的 context 只到 m(-3) — 用 m(-3) 代替
        let cond_b = ctx.m_minus_3.is_some_and(|m_neg_3| {
            magnitude(m_neg_3) > magnitude(m_minus_2)
        });
        if cond_a && cond_b {
            change_certainty(cands, StructureLabel::XC3, Certainty::Possible);
        }
    }

    // Branch 3:x-wave 在 m2 場景成立:m(-2) > m(-1) AND m1 ≥ 38.2% m(-1)
    if let (Some(m_minus_1), Some(m_minus_2)) = (ctx.m_minus_1, ctx.m_minus_2) {
        let cond_a = magnitude(m_minus_2) > magnitude(m_minus_1);
        let cond_b = mag_ratio(m1, m_minus_1) >= FIB_382;
        if cond_a && cond_b {
            add_or_promote(cands, StructureLabel::XC3, Certainty::Possible);
        }
    }

    // Branch 4:m(-1) > m0 AND m(-1) < 261.8% m1 AND m3 < m1
    //          AND 自 m3 終點市場快速回到 m1 起點
    //   → add :c3(Terminal 完成於 m3)
    if let (Some(m_minus_1), Some(m3), Some(m4)) = (ctx.m_minus_1, ctx.m3, ctx.m4) {
        let m_neg_1_dominant =
            magnitude(m_minus_1) > magnitude(m0) && mag_ratio(m_minus_1, m1) < FIB_2618;
        let m3_short = magnitude(m3) < magnitude(m1);
        let fast_return = retracement_pct(&m1.monowave, &m4.monowave) >= 1.0;
        if m_neg_1_dominant && m3_short && fast_return {
            add_or_promote(cands, StructureLabel::C3, Certainty::Primary);
        }
    }

    // Branch 5:m0 polywave AND m1 完全回測 m0 → 記(diagnostic)

    // Branch 6:m2 在自身時間內被完全回測 AND m3 比 m1 更長更垂直 AND m(-1) ≤ 161.8% m1
    //   → add :sL3(Running Triangle 終結於 m2)
    if let (Some(m_minus_1), Some(m2), Some(m3)) = (ctx.m_minus_1, ctx.m2, ctx.m3) {
        let m2_retraced = completely_retraced_within_time(m2, m3);
        let m3_steeper = more_vertical_and_longer(m3, m1);
        let m_neg_1_short = mag_ratio(m_minus_1, m1) <= FIB_1618;
        if m2_retraced && m3_steeper && m_neg_1_short {
            add_or_promote(cands, StructureLabel::SL3, Certainty::Primary);
        }
    }

    // Branch 7:m3 與 m(-1) 同時 ≥ 161.8% m1 → add :c3(Irregular Failure 完成於 m2)
    if let (Some(m_minus_1), Some(m3)) = (ctx.m_minus_1, ctx.m3) {
        if mag_ratio(m3, m1) >= FIB_1618 && mag_ratio(m_minus_1, m1) >= FIB_1618 {
            add_or_promote(cands, StructureLabel::C3, Certainty::Primary);
        }
    }
}

// ---------------------------------------------------------------------------
// Condition 2d — 100% ≤ m0 ≤ 161.8% m1(4 branches)
// ---------------------------------------------------------------------------

fn cond_2d(ctx: &MonowaveContext, cands: &mut Vec<StructureLabelCandidate>) {
    let m1 = ctx.m1;
    let Some(m0) = ctx.m0 else { return };

    // Branch 1:m2 耗時 ≥ m1 OR m2 耗時 ≥ m3 → add :5
    if let Some(m2) = ctx.m2 {
        let cond_a = duration(m2) >= duration(m1);
        let cond_b = ctx.m3.is_some_and(|m3| duration(m2) >= duration(m3));
        if cond_a || cond_b {
            add_or_promote(cands, StructureLabel::Five, Certainty::Primary);
        }
    }

    // Branch 2:m2 在自身時間內被完全回測 AND m3 比 m1 更長更垂直
    //          AND m0/m1 時間相近(61.8% 容差) AND m2 ≥ 61.8% m0 或 m1 時間
    //          AND m0 ≤ 138.2% m1 → add :c3
    if let (Some(m2), Some(m3)) = (ctx.m2, ctx.m3) {
        let m2_retraced = completely_retraced_within_time(m2, m3);
        let m3_steeper = more_vertical_and_longer(m3, m1);
        let m0_m1_time_close = approx_equal(duration(m0) as f64, duration(m1) as f64)
            || approx_equal(duration(m0) as f64, duration(m1) as f64 * FIB_618);
        let m2_long_enough = duration(m2) as f64 >= duration(m0) as f64 * FIB_618
            || duration(m2) as f64 >= duration(m1) as f64 * FIB_618;
        let m0_under_1382 = mag_ratio(m0, m1) <= FIB_1382;
        if m2_retraced && m3_steeper && m0_m1_time_close && m2_long_enough && m0_under_1382 {
            add_or_promote(cands, StructureLabel::C3, Certainty::Primary);
        }
    }

    // Branch 3:m3 比 m1 更長更垂直 AND (m3 完全被回測 OR m3 被回測 ≤ 61.8%)
    //          AND m0 Structure 含 :c3 AND m(-3) > m(-2) AND m(-2) 或 m(-1) > m0
    //   → add (:sL3)
    if let (Some(m_minus_3), Some(m_minus_2), Some(m_minus_1), Some(m3), Some(m4)) = (
        ctx.m_minus_3,
        ctx.m_minus_2,
        ctx.m_minus_1,
        ctx.m3,
        ctx.m4,
    ) {
        let m3_steeper = more_vertical_and_longer(m3, m1);
        let m3_retraced = completely_retraced(&m3.monowave, &m4.monowave)
            || retracement_pct(&m3.monowave, &m4.monowave) <= FIB_618;
        let m0_has_c3 = structure_includes(m0, StructureLabel::C3);
        let m_neg_3_gt = magnitude(m_minus_3) > magnitude(m_minus_2);
        let m_neg_2_or_neg_1_gt_m0 =
            magnitude(m_minus_2) > magnitude(m0) || magnitude(m_minus_1) > magnitude(m0);
        if m3_steeper && m3_retraced && m0_has_c3 && m_neg_3_gt && m_neg_2_or_neg_1_gt_m0 {
            add_or_promote(cands, StructureLabel::SL3, Certainty::Possible);
        }
    }

    // Branch 4:m3 < m1 AND m3 被回測 ≥ 61.8% AND m1 耗時 < m0 AND m2 耗時 ≥ m1
    //   → add :5(Zigzag 完成於 m3)
    if let (Some(m2), Some(m3), Some(m4)) = (ctx.m2, ctx.m3, ctx.m4) {
        let m3_short = magnitude(m3) < magnitude(m1);
        let m3_retraced = retracement_pct(&m3.monowave, &m4.monowave) >= FIB_618;
        let m1_dur_short = duration(m1) < duration(m0);
        let m2_dur_long = duration(m2) >= duration(m1);
        if m3_short && m3_retraced && m1_dur_short && m2_dur_long {
            add_or_promote(cands, StructureLabel::Five, Certainty::Primary);
        }
    }
}

// ---------------------------------------------------------------------------
// Condition 2e — m0 > 161.8% m1(3 branches)
// ---------------------------------------------------------------------------

fn cond_2e(ctx: &MonowaveContext, cands: &mut Vec<StructureLabelCandidate>) {
    let m1 = ctx.m1;

    // Branch 1:(無前置)→ add :5
    add_or_promote(cands, StructureLabel::Five, Certainty::Primary);

    // Branch 2:m3 < m1 AND m3 較不垂直 → :5 為唯一可能
    if let Some(m3) = ctx.m3 {
        let m3_short = magnitude(m3) < magnitude(m1);
        let m3_less_vertical = !more_vertical_and_longer(m3, m1);
        if m3_short && m3_less_vertical {
            // 清掉其他 candidate(:5 唯一)
            cands.retain(|c| matches!(c.label, StructureLabel::Five));
        }
    }

    // Branch 3:m2 在自身時間內被完全回測 AND m3 比 m1 更長更垂直 AND m(-1) 與 m1 無共享價區
    //   → add :c3(Complex Correction with missing x in m0 中心)
    if let (Some(m_minus_1), Some(m2), Some(m3)) = (ctx.m_minus_1, ctx.m2, ctx.m3) {
        let m2_retraced = completely_retraced_within_time(m2, m3);
        let m3_steeper = more_vertical_and_longer(m3, m1);
        let no_share = !share_price_range(&m_minus_1.monowave, &m1.monowave);
        if m2_retraced && m3_steeper && no_share {
            add_or_promote(cands, StructureLabel::C3, Certainty::Primary);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::monowave::ProportionMetrics;
    use crate::output::{Monowave, MonowaveDirection};
    use chrono::NaiveDate;

    fn cmw(start_p: f64, end_p: f64, dir: MonowaveDirection, dur: usize) -> crate::monowave::ClassifiedMonowave {
        crate::monowave::ClassifiedMonowave {
            monowave: Monowave {
                start_date: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                end_date: NaiveDate::from_ymd_opt(2026, 1, 5).unwrap(),
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

    #[test]
    fn cond_2e_emits_five() {
        // m0 / m1 = 2.0 (> 161.8%);m2 / m1 = 0.45 (in 38.2-61.8%)
        let classified = vec![
            cmw(100.0, 100.0, MonowaveDirection::Up, 1),    // m_minus_1
            cmw(100.0, 80.0, MonowaveDirection::Down, 5),   // m0 mag 20
            cmw(80.0, 90.0, MonowaveDirection::Up, 5),      // m1 mag 10 (m0/m1 = 2.0)
            cmw(90.0, 85.5, MonowaveDirection::Down, 5),    // m2 mag 4.5 (m2/m1 = 0.45)
        ];
        let ctx = MonowaveContext::build(&classified, 2).expect("build");
        let mut cands = Vec::new();
        run(&ctx, &mut cands);
        assert!(cands.iter().any(|c| matches!(c.label, StructureLabel::Five)));
    }

    #[test]
    fn cond_2a_branch_1_emits_five() {
        // m0 / m1 = 0.3 (< 0.382); m2 / m1 = 0.5 (in 0.382-0.618)
        let classified = vec![
            cmw(100.0, 100.0, MonowaveDirection::Up, 1),
            cmw(100.0, 97.0, MonowaveDirection::Down, 3), // m0 mag 3 (m0/m1=0.3)
            cmw(97.0, 107.0, MonowaveDirection::Up, 5),    // m1 mag 10
            cmw(107.0, 102.0, MonowaveDirection::Down, 3), // m2 mag 5
        ];
        let ctx = MonowaveContext::build(&classified, 2).expect("build");
        let mut cands = Vec::new();
        run(&ctx, &mut cands);
        assert!(cands.iter().any(|c| matches!(c.label, StructureLabel::Five)));
    }
}
