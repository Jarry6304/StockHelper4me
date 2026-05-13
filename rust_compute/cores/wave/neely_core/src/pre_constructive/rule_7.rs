// rule_7.rs — Rule 7(m2 > 261.8% m1)所有 Condition 與 branch
//
// 對齊 m3Spec/neely_rules.md §Rule 7(974-1036 行)。
// Headline:`{any Structure possible}`

use super::context::MonowaveContext;
use super::predicates::*;
use crate::output::{Certainty, MonowaveDirection, StructureLabel, StructureLabelCandidate};

pub fn run(ctx: &MonowaveContext, cands: &mut Vec<StructureLabelCandidate>) {
    let Some(m0) = ctx.m0 else { return };
    let ratio = mag_ratio(m0, ctx.m1);

    if ratio < FIB_1000 {
        cond_7a(ctx, cands);
    } else if ratio < FIB_1618 {
        cond_7b(ctx, cands);
    } else if ratio <= FIB_2618 {
        cond_7c(ctx, cands);
    } else {
        cond_7d(ctx, cands);
    }
}

// ---------------------------------------------------------------------------
// Condition 7a — m0 < 100% m1
// ---------------------------------------------------------------------------

fn cond_7a(ctx: &MonowaveContext, cands: &mut Vec<StructureLabelCandidate>) {
    cond_7a_b(ctx, cands);
}

fn cond_7a_b(ctx: &MonowaveContext, cands: &mut Vec<StructureLabelCandidate>) {
    // (B) m2 ≤ 3 monowaves(預設,polywave 偵測 [缺資料])

    // Branch 1:(任何環境) → add :L5(高機率)
    add_or_promote(cands, StructureLabel::L5, Certainty::Primary);

    // Branch 2:m2 被回測 < 61.8% AND m(-2) < m(-1) AND m(-2)/m0 共享 → add (:L3)
    if let (Some(m0), Some(m_minus_1), Some(m_minus_2), Some(m2), Some(m3)) =
        (ctx.m0, ctx.m_minus_1, ctx.m_minus_2, ctx.m2, ctx.m3)
    {
        let m2_retrace = retracement_pct(&m2.monowave, &m3.monowave);
        let cond_a = m2_retrace < FIB_618;
        let cond_b = magnitude(m_minus_2) < magnitude(m_minus_1);
        let cond_c = share_price_range(&m_minus_2.monowave, &m0.monowave);
        if cond_a && cond_b && cond_c {
            add_or_promote(cands, StructureLabel::L3, Certainty::Possible);
        }
    }
}

// ---------------------------------------------------------------------------
// Condition 7b — 100% ≤ m0 < 161.8% m1
// ---------------------------------------------------------------------------

fn cond_7b(ctx: &MonowaveContext, cands: &mut Vec<StructureLabelCandidate>) {
    cond_7b_b(ctx, cands);
}

fn cond_7b_b(ctx: &MonowaveContext, cands: &mut Vec<StructureLabelCandidate>) {
    let m1 = ctx.m1;
    let Some(m0) = ctx.m0 else { return };
    if let (Some(m2), Some(m3)) = (ctx.m2, ctx.m3) {
        // Branch 1:m0 ≥ 61.8% m1 AND m3 在 100-261.8% m2 → add :c3 (Expanding Triangle);
        //   m4 > 61.8% m3 → add :F3
        let m3_to_m2 = mag_ratio(m3, m2);
        if mag_ratio(m0, m1) >= FIB_618 && (FIB_1000..=FIB_2618).contains(&m3_to_m2) {
            add_or_promote(cands, StructureLabel::C3, Certainty::Primary);
            if let Some(m4) = ctx.m4 {
                if mag_ratio(m4, m3) > FIB_618 {
                    add_or_promote(cands, StructureLabel::F3, Certainty::Primary);
                }
            }
        }
        // Branch 2:m1 接近 61.8% m0(不超太多)AND m2 retraced < 61.8% OR > 100% (≤ m2 time)
        //          AND m2 ≥ m0 mag in ≤ m0 time AND m2 更垂直
        //   → add :L3/:L5
        let m1_near_618_m0 = fib_approx(mag_ratio(m1, m0), FIB_618);
        let m2_retrace = retracement_pct(&m2.monowave, &m3.monowave);
        let m2_quick = duration(m3) <= duration(m2)
            && !(FIB_618..=1.0).contains(&m2_retrace);
        let m2_long_fast = magnitude(m2) >= magnitude(m0) && duration(m2) <= duration(m0);
        let m2_steeper = more_vertical_and_longer(m2, m0);
        if m1_near_618_m0 && m2_quick && m2_long_fast && m2_steeper {
            add_or_promote(cands, StructureLabel::L3, Certainty::Primary);
            add_or_promote(cands, StructureLabel::L5, Certainty::Primary);
        }
        // Branch 3:m2 retraced ≥ 61.8% < 100% → add :c3
        if (FIB_618..1.0).contains(&m2_retrace) {
            add_or_promote(cands, StructureLabel::C3, Certainty::Primary);
        }
        // Branch 4:m2 在自身時間內被完全回測 → add :L5 (Trending Impulse 完成);
        //   m(-1) < m0 AND m0 not shortest AND market returns to m(-2) start → add :sL3
        if completely_retraced_within_time(m2, m3) {
            add_or_promote(cands, StructureLabel::L5, Certainty::Primary);
            if let (Some(m_minus_1), Some(m_minus_2)) = (ctx.m_minus_1, ctx.m_minus_2) {
                let m_neg_1_lt_m0 = magnitude(m_minus_1) < magnitude(m0);
                let not_shortest = !is_shortest_of_three(m0, Some(m_minus_2), Some(m2));
                let dur_chain = duration(m_minus_2)
                    + duration(m_minus_1) + duration(m0) + duration(m1) + duration(m2);
                let returns = ctx.m4.is_some_and(|m4| {
                    retracement_pct(&m_minus_2.monowave, &m4.monowave) >= 1.0
                        && duration(m4) * 2 <= dur_chain
                });
                if m_neg_1_lt_m0 && not_shortest && returns {
                    add_or_promote(cands, StructureLabel::SL3, Certainty::Primary);
                }
            }
        }
        // Branch 5:m1 在自身時間內被完全回測 AND m2 ≥ 161.8% m0 AND m1 突破 m(-3)/m(-1) 連線
        //   → add :L5
        if let (Some(m_minus_3), Some(m_minus_1)) = (ctx.m_minus_3, ctx.m_minus_1) {
            let cond_a = completely_retraced_within_time(m1, m2);
            let cond_b = mag_ratio(m2, m0) >= FIB_1618;
            let cond_c = m2_breaches_2_4_line_within_m1_time(m_minus_3, m_minus_1, m0, m1);
            if cond_a && cond_b && cond_c {
                add_or_promote(cands, StructureLabel::L5, Certainty::Primary);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Condition 7c — 161.8% ≤ m0 ≤ 261.8% m1
// ---------------------------------------------------------------------------

fn cond_7c(ctx: &MonowaveContext, cands: &mut Vec<StructureLabelCandidate>) {
    let m1 = ctx.m1;
    let Some(m0) = ctx.m0 else { return };
    if let (Some(m2), Some(m3)) = (ctx.m2, ctx.m3) {
        // Branch 1:m1 耗時 ≥ m0 OR m1 耗時 ≥ m2 → add :F3
        let m1_long_time = duration(m1) >= duration(m0) || duration(m1) >= duration(m2);
        if m1_long_time {
            add_or_promote(cands, StructureLabel::F3, Certainty::Primary);
        }
        // Branch 2:m2 ≥ m0 in ≤ m0 time AND m2 更垂直 AND m(-4) > m(-2) → add :L3
        let m2_long_fast = magnitude(m2) >= magnitude(m0) && duration(m2) <= duration(m0);
        let m2_steeper = more_vertical_and_longer(m2, m0);
        let m_neg_4_gt = ctx
            .m_minus_3
            .zip(ctx.m_minus_2)
            .is_some_and(|(m_neg_3, m_neg_2)| magnitude(m_neg_3) > magnitude(m_neg_2));
        if m2_long_fast && m2_steeper && m_neg_4_gt {
            add_or_promote(cands, StructureLabel::L3, Certainty::Primary);
        }
        // Branch 3:同上條件 + m(-2) ≥ 161.8% m0 AND m(-2) ≥ 61.8% m2 AND m(-1) Structure 含 :F3
        //   → add :L5 (Irregular Failure Flat)
        if let (Some(m_minus_1), Some(m_minus_2)) = (ctx.m_minus_1, ctx.m_minus_2) {
            let cond_a = m2_long_fast && m2_steeper;
            let cond_b = mag_ratio(m_minus_2, m0) >= FIB_1618;
            let cond_c = mag_ratio(m_minus_2, m2) >= FIB_618;
            let cond_d = structure_includes(m_minus_1, StructureLabel::F3);
            if cond_a && cond_b && cond_c && cond_d {
                add_or_promote(cands, StructureLabel::L5, Certainty::Primary);
            }
        }
        // Branch 4:m2 在自身時間內被完全回測 AND market returns AND m0 > m(-2) → add :sL3
        if let Some(m_minus_2) = ctx.m_minus_2 {
            if completely_retraced_within_time(m2, m3) {
                let dur_chain = duration(m_minus_2)
                    + duration(ctx.m_minus_1.unwrap_or(m_minus_2))
                    + duration(m0) + duration(m1) + duration(m2);
                let returns = ctx.m4.is_some_and(|m4| {
                    retracement_pct(&m_minus_2.monowave, &m4.monowave) >= 1.0
                        && duration(m4) * 2 <= dur_chain
                });
                let m0_dominant = magnitude(m0) > magnitude(m_minus_2);
                if returns && m0_dominant {
                    add_or_promote(cands, StructureLabel::SL3, Certainty::Primary);
                }
            }
        }
        // Branch 5:m1 在自身時間內被完全回測 AND m2 ≥ 161.8% m0 AND m1 breaches m(-3)/m(-1) line
        //   → add :L5 (Running Correction)
        if let (Some(m_minus_3), Some(m_minus_1)) = (ctx.m_minus_3, ctx.m_minus_1) {
            let cond_a = completely_retraced_within_time(m1, m2);
            let cond_b = mag_ratio(m2, m0) >= FIB_1618;
            let cond_c = m2_breaches_2_4_line_within_m1_time(m_minus_3, m_minus_1, m0, m1);
            if cond_a && cond_b && cond_c {
                add_or_promote(cands, StructureLabel::L5, Certainty::Primary);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Condition 7d — m0 > 261.8% m1
// ---------------------------------------------------------------------------

fn cond_7d(ctx: &MonowaveContext, cands: &mut Vec<StructureLabelCandidate>) {
    let m1 = ctx.m1;
    let Some(m0) = ctx.m0 else { return };
    if let (Some(m2), Some(m3)) = (ctx.m2, ctx.m3) {
        // Branch 1:m0(minus 1)≤ m1 OR m2(minus 1)≤ m1 AND m1 不同時短於 m0 與 m2 → add :F3
        let m0_short = duration(m0).saturating_sub(1) <= duration(m1);
        let m2_short = duration(m2).saturating_sub(1) <= duration(m1);
        let m1_not_both_shorter = !(duration(m1) < duration(m0) && duration(m1) < duration(m2));
        if (m0_short || m2_short) && m1_not_both_shorter {
            add_or_promote(cands, StructureLabel::F3, Certainty::Primary);
        }
        // Branch 2:Double Zigzag x-wave 場景 → add x:c3
        if let (Some(m_minus_1), Some(m_minus_2), Some(m4)) =
            (ctx.m_minus_1, ctx.m_minus_2, ctx.m4)
        {
            let m1_short =
                duration(m1) <= duration(m0) || duration(m1) <= duration(m2);
            let cond_b = mag_ratio(m_minus_2, m_minus_1) >= FIB_1618;
            let cond_c = magnitude(m_minus_1) < magnitude(m0);
            // m1 < 61.8% × m(-2)~m0 距離(用 m(-2) 起點到 m0 終點的距離)
            let m_neg_2_to_m0_distance = (m0.monowave.end_price - m_minus_2.monowave.start_price).abs();
            let cond_d = magnitude(m1) < FIB_618 * m_neg_2_to_m0_distance;
            let cond_e = magnitude(m3) > magnitude(m2);
            let cond_f = magnitude(m4) < magnitude(m3);
            // m(-2)~m2 已被回測 ≥ 61.8% (在 m2 終點前)
            let retraced_61 = retracement_pct(
                &crate::output::Monowave {
                    start_date: m_minus_2.monowave.start_date,
                    end_date: m2.monowave.end_date,
                    start_price: m_minus_2.monowave.start_price,
                    end_price: m2.monowave.end_price,
                    direction: m_minus_2.monowave.direction,
                },
                &m3.monowave,
            ) >= FIB_618;
            if m1_short && cond_b && cond_c && cond_d && cond_e && cond_f && retraced_61 {
                add_or_promote(cands, StructureLabel::XC3, Certainty::Primary);
            }
        }
        // Branch 3:Complex starting Flat ending Flat/Triangle x-wave → add x:c3
        if let (Some(m_minus_1), Some(m4)) = (ctx.m_minus_1, ctx.m4) {
            let m1_short =
                duration(m1) <= duration(m0) || duration(m1) <= duration(m2);
            let cond_b = (FIB_1000..=FIB_1618).contains(&mag_ratio(m0, m_minus_1));
            let cond_c = mag_ratio(m2, m0) <= FIB_1618;
            let cond_d = mag_ratio(m4, m2) >= FIB_382;
            let cond_e = magnitude(m3) > magnitude(m2) && magnitude(m4) < magnitude(m3);
            if m1_short && cond_b && cond_c && cond_d && cond_e {
                add_or_promote(cands, StructureLabel::XC3, Certainty::Primary);
            }
        }
        // Branch 4:m1 ≤ m0 AND m1 ≤ m2 → add :c3;若 m(-1) ≈ m1 AND m(-1) < m0 → potentially x:c3
        let m1_both_short = duration(m1) <= duration(m0) && duration(m1) <= duration(m2);
        if m1_both_short {
            add_or_promote(cands, StructureLabel::C3, Certainty::Primary);
            if let Some(m_minus_1) = ctx.m_minus_1 {
                let m_neg_1_similar = approx_equal(magnitude(m_minus_1), magnitude(m1))
                    || approx_equal(magnitude(m_minus_1), magnitude(m1) * FIB_618);
                let m_neg_1_lt_m0 = magnitude(m_minus_1) < magnitude(m0);
                if let Some(m_minus_2) = ctx.m_minus_2 {
                    let m0_not_shortest =
                        !is_shortest_of_three(m0, Some(m_minus_2), Some(m2));
                    let mut mags = [magnitude(m_minus_2), magnitude(m0), magnitude(m2)];
                    mags.sort_by(|a, b| a.partial_cmp(b).unwrap());
                    let ratio_check =
                        mags[2] <= FIB_1618 * mags[1] && mags[1] <= FIB_1618 * mags[0];
                    if m_neg_1_similar && m_neg_1_lt_m0 && m0_not_shortest && ratio_check {
                        prefix_c3_with_x(cands);
                    }
                    let m0_longest =
                        magnitude(m0) > magnitude(m_minus_2) && magnitude(m0) > magnitude(m2);
                    if m0_longest {
                        add_or_promote(cands, StructureLabel::XC3, Certainty::MissingWaveBundle);
                        add_or_promote(cands, StructureLabel::S5, Certainty::MissingWaveBundle);
                    }
                }
            }
        }
        // Branch 5:Running Correction 完成 → add :L5(同 6d-branch5 / 7c-branch5)
        if let Some(m_minus_1) = ctx.m_minus_1 {
            let cond_a = completely_retraced_plus_one_time_unit(m1, m2);
            let cond_b = approx_equal(magnitude(m_minus_1), magnitude(m1));
            let cond_c = mag_ratio(m2, m0) >= FIB_1618;
            let cond_d = !share_price_range(&m1.monowave, &m_minus_1.monowave);
            let cond_e = !(duration(m3) < duration(m2)
                && retracement_pct(&m2.monowave, &m3.monowave) >= FIB_618);
            if cond_a && cond_b && cond_c && cond_d && cond_e {
                add_or_promote(cands, StructureLabel::L5, Certainty::Primary);
            }
        }
        // Branch 6/7/8 同 6d-branches 6/7/8 結構
        if let Some(m_minus_1) = ctx.m_minus_1 {
            let m2_retrace = retracement_pct(&m2.monowave, &m3.monowave);
            // Branch 6:add (:L3)
            if m2_retrace < FIB_618 && more_vertical_and_longer(m2, m0)
                && mag_ratio(m_minus_1, m0) <= FIB_1618
                && share_price_range(&m_minus_1.monowave, &m1.monowave)
                && (structure_includes(m0, StructureLabel::Three)
                    || structure_includes(m0, StructureLabel::C3)
                    || structure_includes(m0, StructureLabel::F3)
                    || structure_includes(m0, StructureLabel::L3))
            {
                add_or_promote(cands, StructureLabel::L3, Certainty::Possible);
                if approx_equal(magnitude(m_minus_1), magnitude(m1)) {
                    add_or_promote(cands, StructureLabel::L5, Certainty::Primary);
                }
            }
            // Branch 7:add x:c3
            let m2_mag_range = (FIB_618..=FIB_1618).contains(&mag_ratio(m2, m0));
            if m2_retrace < FIB_618
                && m2_mag_range
                && magnitude(m_minus_1) < magnitude(m0)
                && mag_ratio(m_minus_1, m0) <= FIB_1618
            {
                add_or_promote(cands, StructureLabel::XC3, Certainty::Primary);
            }
        }
        // Branch 8:m2(plus 1)retraced AND m3 retraced ≤ 61.8% AND m(-1) < m0 AND m(-1)/m1 share
        //          AND m0 not shortest AND m3 returns → add :sL3
        if completely_retraced_plus_one_time_unit(m2, m3) {
            if let (Some(m_minus_1), Some(m_minus_2), Some(m4)) =
                (ctx.m_minus_1, ctx.m_minus_2, ctx.m4)
            {
                let cond_a = retracement_pct(&m3.monowave, &m4.monowave) <= FIB_618;
                let cond_b = magnitude(m_minus_1) < magnitude(m0);
                let cond_c = share_price_range(&m_minus_1.monowave, &m1.monowave);
                let cond_d = !is_shortest_of_three(m0, Some(m_minus_2), Some(m2));
                let dur_chain = duration(m_minus_2)
                    + duration(m_minus_1) + duration(m0) + duration(m1) + duration(m2);
                let cond_e = retracement_pct(&m_minus_2.monowave, &m3.monowave) >= 1.0
                    && duration(m3) * 2 <= dur_chain;
                if cond_a && cond_b && cond_c && cond_d && cond_e {
                    add_or_promote(cands, StructureLabel::SL3, Certainty::Primary);
                }
            }
        }
    }
    let _ = MonowaveDirection::Up;
}
