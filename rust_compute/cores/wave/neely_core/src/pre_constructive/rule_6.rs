// rule_6.rs — Rule 6(161.8% ≤ m2 ≤ 261.8% m1)所有 Condition 與 branch
//
// 對齊 m3Spec/neely_rules.md §Rule 6(903-970 行)。
// Headline:`{any Structure possible}`
//
// 邏輯結構與 Rule 5 高度類似,因此本檔 reuse 同款 helper 模式。

use super::context::MonowaveContext;
use super::predicates::*;
use crate::output::{Certainty, MonowaveDirection, StructureLabel, StructureLabelCandidate};

pub fn run(ctx: &MonowaveContext, cands: &mut Vec<StructureLabelCandidate>) {
    let Some(m0) = ctx.m0 else { return };
    let ratio = mag_ratio(m0, ctx.m1);

    if ratio < FIB_1000 {
        cond_6a(ctx, cands);
    } else if ratio < FIB_1618 {
        cond_6b(ctx, cands);
    } else if ratio <= FIB_2618 {
        cond_6c(ctx, cands);
    } else {
        cond_6d(ctx, cands);
    }
}

// ---------------------------------------------------------------------------
// Condition 6a — m0 < 100% m1
// ---------------------------------------------------------------------------

fn cond_6a(ctx: &MonowaveContext, cands: &mut Vec<StructureLabelCandidate>) {
    // [缺資料] m2 polywave 偵測 → (B) 分支
    cond_6a_b(ctx, cands);
}

fn cond_6a_b(ctx: &MonowaveContext, cands: &mut Vec<StructureLabelCandidate>) {
    let m1 = ctx.m1;
    if let (Some(m2), Some(m3)) = (ctx.m2, ctx.m3) {
        let m3_retrace_m2 = retracement_pct(&m2.monowave, &m3.monowave);
        // Branch 1:m2 被 m3 回測 < 61.8% → add :L5;m0 與 m(-2) 共享 → add :L3
        if m3_retrace_m2 < FIB_618 {
            add_or_promote(cands, StructureLabel::L5, Certainty::Primary);
            if let (Some(m0), Some(m_minus_2)) = (ctx.m0, ctx.m_minus_2) {
                if share_price_range(&m0.monowave, &m_minus_2.monowave) {
                    add_or_promote(cands, StructureLabel::L3, Certainty::Primary);
                }
            }
        }
        // Branch 2:m2 被 m3 回測 ≥ 61.8% → add :L5
        if m3_retrace_m2 >= FIB_618 {
            add_or_promote(cands, StructureLabel::L5, Certainty::Primary);
        }
        // Branch 3:m1 在自身時間內被完全回測 → add :L5
        if completely_retraced_within_time(m1, m2) {
            add_or_promote(cands, StructureLabel::L5, Certainty::Primary);
        }
        // Branch 4:m1 在自身時間內被完全回測 AND m3 < m2
        //          AND m2 ≤ 50% m(-3)~m1 時間內接近 m(-3) 起點 AND m0/m(-2) 共享
        //   → add (:L3);m3 在 61.8-100% AND :L3 首選 → m2 為 x-wave
        if let Some(m_minus_3) = ctx.m_minus_3 {
            let cond_a = completely_retraced_within_time(m1, m2);
            let cond_b = magnitude(m3) < magnitude(m2);
            let dur_chain = duration(m_minus_3)
                + duration(ctx.m_minus_2.unwrap_or(m_minus_3))
                + duration(ctx.m_minus_1.unwrap_or(m_minus_3))
                + duration(ctx.m0.unwrap_or(m_minus_3))
                + duration(m1);
            let cond_c = retracement_pct(&m_minus_3.monowave, &m2.monowave) >= 1.0
                && duration(m2) * 2 <= dur_chain;
            let cond_d = ctx
                .m0
                .zip(ctx.m_minus_2)
                .is_some_and(|(m0, m_neg_2)| {
                    share_price_range(&m0.monowave, &m_neg_2.monowave)
                });
            if cond_a && cond_b && cond_c && cond_d {
                add_or_promote(cands, StructureLabel::L3, Certainty::Possible);
                let r = mag_ratio(m3, m2);
                if (FIB_618..FIB_1000).contains(&r) {
                    add_or_promote(cands, StructureLabel::XC3, Certainty::MissingWaveBundle);
                }
            }
        }
        // Branch 5:m1 慢於自身時間被回測 AND m2 不超 m(-2) 終點 AND m(-1) ≥ 61.8% m1 AND m(-2) < m(-1)
        //   → add :F3,m0 加 x:c3?
        if let (Some(m0), Some(m_minus_1), Some(m_minus_2)) =
            (ctx.m0, ctx.m_minus_1, ctx.m_minus_2)
        {
            let m1_slow = duration(m2) > duration(m1)
                && retracement_pct(&m1.monowave, &m2.monowave) >= 1.0;
            let m2_within = match m1.monowave.direction {
                MonowaveDirection::Up => m2.monowave.end_price >= m_minus_2.monowave.end_price,
                MonowaveDirection::Down => m2.monowave.end_price <= m_minus_2.monowave.end_price,
                _ => false,
            };
            let m_neg_1_long = mag_ratio(m_minus_1, m1) >= FIB_618;
            let m_neg_2_lt_m_neg_1 = magnitude(m_minus_2) < magnitude(m_minus_1);
            if m1_slow && m2_within && m_neg_1_long && m_neg_2_lt_m_neg_1 {
                add_or_promote(cands, StructureLabel::F3, Certainty::Primary);
                add_or_promote(cands, StructureLabel::XC3, Certainty::MissingWaveBundle);
                let _ = m0;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Condition 6b — 100% ≤ m0 < 161.8% m1
// ---------------------------------------------------------------------------

fn cond_6b(ctx: &MonowaveContext, cands: &mut Vec<StructureLabelCandidate>) {
    cond_6b_b(ctx, cands);
}

fn cond_6b_b(ctx: &MonowaveContext, cands: &mut Vec<StructureLabelCandidate>) {
    let m1 = ctx.m1;
    let Some(m0) = ctx.m0 else { return };
    if let (Some(m2), Some(m3)) = (ctx.m2, ctx.m3) {
        // Branch 1:m1 ≤ m0 時間 OR m1 ≤ m2 時間 AND m(-2) < m(-1) → add x:c3
        if let (Some(m_minus_1), Some(m_minus_2)) = (ctx.m_minus_1, ctx.m_minus_2) {
            let m1_short_time =
                duration(m1) <= duration(m0) || duration(m1) <= duration(m2);
            if m1_short_time && magnitude(m_minus_2) < magnitude(m_minus_1) {
                add_or_promote(cands, StructureLabel::XC3, Certainty::Primary);
            }
        }
        // Branch 2:m1 耗時 ≥ m0 OR m1 耗時 ≥ m2 AND m0 ≈ 161.8% m1 → add :F3
        let m1_long_time =
            duration(m1) >= duration(m0) || duration(m1) >= duration(m2);
        let m0_near_1618 = fib_approx(mag_ratio(m0, m1), FIB_1618);
        if m1_long_time && m0_near_1618 {
            add_or_promote(cands, StructureLabel::F3, Certainty::Primary);
        }
        // Branch 3:m1 在自身時間內被 m2 完全回測 AND m2 被回測 < 61.8% OR > 100% (≤ m2 time)
        //          AND m(-1) ≥ 61.8% m1 → add :L5 (Flat wave-c)
        let m1_retraced = completely_retraced_within_time(m1, m2);
        let m2_retrace = retracement_pct(&m2.monowave, &m3.monowave);
        let m2_quick_retrace = duration(m3) <= duration(m2)
            && !(FIB_618..=1.0).contains(&m2_retrace);
        if m1_retraced && m2_quick_retrace {
            if let Some(m_minus_1) = ctx.m_minus_1 {
                if mag_ratio(m_minus_1, m1) >= FIB_618 {
                    add_or_promote(cands, StructureLabel::L5, Certainty::Primary);
                }
            }
        }
        // Branch 4:m1 被 m2 完全回測 (在 m1 時間內) AND m2 被回測 < 61.8% AND m(-1) ≥ 61.8% m0
        //   → add :L3/:L5
        if completely_retraced_within_time(m1, m2) && m2_retrace < FIB_618 {
            if let Some(m_minus_1) = ctx.m_minus_1 {
                if mag_ratio(m_minus_1, m0) >= FIB_618 {
                    add_or_promote(cands, StructureLabel::L3, Certainty::Primary);
                    add_or_promote(cands, StructureLabel::L5, Certainty::Primary);
                }
            }
        }
        // Branch 5:m1 慢於自身時間被回測 AND m2 含 ≥ 3 monowaves [缺資料]
        //   → add :c3 — Phase 2 不觸發
        // Branch 6:m2 在自身時間內被完全回測 AND m0 not shortest AND market returns
        //   → add :sL3
        if completely_retraced_within_time(m2, m3) {
            if let Some(m_minus_2) = ctx.m_minus_2 {
                let not_shortest = !is_shortest_of_three(m0, Some(m_minus_2), Some(m2));
                let dur_chain =
                    duration(m_minus_2) + duration(ctx.m_minus_1.unwrap_or(m_minus_2))
                        + duration(m0) + duration(m1) + duration(m2);
                let returns = ctx
                    .m4
                    .is_some_and(|m4| {
                        retracement_pct(&m_minus_2.monowave, &m4.monowave) >= 1.0
                            && duration(m4) * 2 <= dur_chain
                    });
                if not_shortest && returns {
                    add_or_promote(cands, StructureLabel::SL3, Certainty::Primary);
                }
            }
        }
        // Branch 7:m3 在 101–161.8% m2 → Expanding Triangle 可能;:F3 → [:F3] 表 :c3 較佳
        let m3_to_m2 = mag_ratio(m3, m2);
        if (1.01..=FIB_1618).contains(&m3_to_m2) {
            change_certainty(cands, StructureLabel::F3, Certainty::Rare);
        }
    }
}

// ---------------------------------------------------------------------------
// Condition 6c — 161.8% ≤ m0 ≤ 261.8% m1(5 branches)
// ---------------------------------------------------------------------------

fn cond_6c(ctx: &MonowaveContext, cands: &mut Vec<StructureLabelCandidate>) {
    let m1 = ctx.m1;
    let Some(m0) = ctx.m0 else { return };
    if let (Some(m2), Some(m3)) = (ctx.m2, ctx.m3) {
        // Branch 1:m1 耗時 ≥ m0 OR m1 耗時 ≥ m2 → add :F3
        let m1_long_time = duration(m1) >= duration(m0) || duration(m1) >= duration(m2);
        if m1_long_time {
            add_or_promote(cands, StructureLabel::F3, Certainty::Primary);
        }
        // Branch 2:m2 達到 m0 長度於 ≤ m0 時間 AND m2 更垂直 AND m(-4) > m(-2) → add :L3
        let m2_long_fast = magnitude(m2) >= magnitude(m0) && duration(m2) <= duration(m0);
        let m2_steeper = more_vertical_and_longer(m2, m0);
        let m_neg_4_gt_m_neg_2 = ctx.m_minus_3.zip(ctx.m_minus_2).is_some_and(|(_m_neg_3, m_neg_2)| {
            // 沒有真正的 m(-4) reference,用 m(-3) 代理
            ctx.m_minus_3
                .is_some_and(|m_neg_3| magnitude(m_neg_3) > magnitude(m_neg_2))
        });
        if m2_long_fast && m2_steeper && m_neg_4_gt_m_neg_2 {
            add_or_promote(cands, StructureLabel::L3, Certainty::Primary);
        }
        // Branch 3:m2 達到 m0 長度於 ≤ m0 時間 AND m2 更垂直 AND m(-2) ≥ 161.8% m0
        //          AND m(-2) ≥ 61.8% m2 AND m(-1) Structure 之一為 :F3
        //   → add :L5 (Irregular Failure Flat 完成於 m1)
        if let (Some(m_minus_1), Some(m_minus_2)) = (ctx.m_minus_1, ctx.m_minus_2) {
            let cond_a = m2_long_fast && m2_steeper;
            let cond_b = mag_ratio(m_minus_2, m0) >= FIB_1618;
            let cond_c = mag_ratio(m_minus_2, m2) >= FIB_618;
            let cond_d = structure_includes(m_minus_1, StructureLabel::F3);
            if cond_a && cond_b && cond_c && cond_d {
                add_or_promote(cands, StructureLabel::L5, Certainty::Primary);
            }
        }
        // Branch 4:m2(plus 1)在自身時間內被完全回測 AND market returns AND m0 > m(-2) → add :sL3
        if completely_retraced_plus_one_time_unit(m2, m3) {
            if let Some(m_minus_2) = ctx.m_minus_2 {
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
        // Branch 5:m1 在自身時間內被完全回測 AND m2 ≥ 161.8% m0 AND m1 突破跨越 m(-3)/m(-1) 連線
        //   → add :L5 (Running Correction 完成於 m1)
        if let (Some(m_minus_3), Some(m_minus_1)) = (ctx.m_minus_3, ctx.m_minus_1) {
            let cond_a = completely_retraced_within_time(m1, m2);
            let cond_b = mag_ratio(m2, m0) >= FIB_1618;
            // 「m1 突破跨越 m(-3) 與 m(-1) 終點的連線」— 用 m2_breaches_2_4_line helper 變形
            let cond_c = m2_breaches_2_4_line_within_m1_time(m_minus_3, m_minus_1, m0, m1);
            if cond_a && cond_b && cond_c {
                add_or_promote(cands, StructureLabel::L5, Certainty::Primary);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Condition 6d — m0 > 261.8% m1(8 branches)
// ---------------------------------------------------------------------------

fn cond_6d(ctx: &MonowaveContext, cands: &mut Vec<StructureLabelCandidate>) {
    let m1 = ctx.m1;
    let Some(m0) = ctx.m0 else { return };
    if let (Some(m2), Some(m3)) = (ctx.m2, ctx.m3) {
        // Branch 1:m0(minus 1)≤ m1 OR m2(minus 1)≤ m1 AND m1 不同時 < m0 與 < m2 → add :F3
        let m0_short = duration(m0).saturating_sub(1) <= duration(m1);
        let m2_short = duration(m2).saturating_sub(1) <= duration(m1);
        let m1_not_both_shorter = !(duration(m1) < duration(m0) && duration(m1) < duration(m2));
        if (m0_short || m2_short) && m1_not_both_shorter {
            add_or_promote(cands, StructureLabel::F3, Certainty::Primary);
        }
        // Branch 2 / Branch 3 (Double Zigzag x-wave / Complex Flat-Triangle x-wave 條件複雜)
        //   → 簡化判定:m1 短時、m(-2) ≥ 161.8% m(-1) AND m(-1) < m0 AND m3 > m2 AND m4 < m3 → add x:c3
        if let (Some(m_minus_1), Some(m_minus_2), Some(m4)) = (ctx.m_minus_1, ctx.m_minus_2, ctx.m4) {
            let m1_short =
                duration(m1) <= duration(m0) || duration(m1) <= duration(m2);
            let m_neg_2_long = mag_ratio(m_minus_2, m_minus_1) >= FIB_1618;
            let m_neg_1_lt_m0 = magnitude(m_minus_1) < magnitude(m0);
            let m3_gt_m2 = magnitude(m3) > magnitude(m2);
            let m4_lt_m3 = magnitude(m4) < magnitude(m3);
            if m1_short && m_neg_2_long && m_neg_1_lt_m0 && m3_gt_m2 && m4_lt_m3 {
                add_or_promote(cands, StructureLabel::XC3, Certainty::Primary);
            }
        }
        // Branch 4:m1 ≤ m0 AND m1 ≤ m2 → add :c3;且若 m(-1) ≈ m1 AND m(-1) < m0 AND m0 not shortest
        //          → 可能改 x:c3 等(missing x in m0 center scenarios)
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
                    let ratio_check = mags[2] <= FIB_1618 * mags[1] && mags[1] <= FIB_1618 * mags[0];
                    if m_neg_1_similar && m_neg_1_lt_m0 && m0_not_shortest && ratio_check {
                        prefix_c3_with_x(cands);
                    }
                    // 若 m0 為三者最長 → missing x 在 m0 中心 bundle
                    let m0_longest = magnitude(m0) > magnitude(m_minus_2) && magnitude(m0) > magnitude(m2);
                    if m0_longest {
                        add_or_promote(cands, StructureLabel::XC3, Certainty::MissingWaveBundle);
                        add_or_promote(cands, StructureLabel::S5, Certainty::MissingWaveBundle);
                    }
                }
            }
        }
        // Branch 5:m1(plus 1)在自身時間內被完全回測 AND m(-1) ≈ m1 AND m2 ≥ 161.8% m0
        //          AND m1/m(-1) 無共享 AND m2 不被快回測 → add :L5
        if let Some(m_minus_1) = ctx.m_minus_1 {
            let cond_a = completely_retraced_plus_one_time_unit(m1, m2);
            let cond_b = approx_equal(magnitude(m_minus_1), magnitude(m1))
                || approx_equal(magnitude(m_minus_1), magnitude(m1) * FIB_618);
            let cond_c = mag_ratio(m2, m0) >= FIB_1618;
            let cond_d = !share_price_range(&m1.monowave, &m_minus_1.monowave);
            let cond_e = !(duration(m3) < duration(m2)
                && retracement_pct(&m2.monowave, &m3.monowave) >= FIB_618);
            if cond_a && cond_b && cond_c && cond_d && cond_e {
                add_or_promote(cands, StructureLabel::L5, Certainty::Primary);
            }
        }
        // Branch 6:m2 < 61.8% retraced AND m2 更垂直 AND m(-1) ≤ 161.8% m0 AND m(-1)/m1 共享
        //          AND m0 Structure 含 :3 任意變體 → add (:L3)
        if let Some(m_minus_1) = ctx.m_minus_1 {
            let m2_retrace = retracement_pct(&m2.monowave, &m3.monowave);
            let cond_a = m2_retrace < FIB_618 && more_vertical_and_longer(m2, m0);
            let cond_b = mag_ratio(m_minus_1, m0) <= FIB_1618;
            let cond_c = share_price_range(&m_minus_1.monowave, &m1.monowave);
            let cond_d = structure_includes(m0, StructureLabel::Three)
                || structure_includes(m0, StructureLabel::C3)
                || structure_includes(m0, StructureLabel::F3)
                || structure_includes(m0, StructureLabel::L3);
            if cond_a && cond_b && cond_c && cond_d {
                add_or_promote(cands, StructureLabel::L3, Certainty::Possible);
                // 子規則:m(-1) ≈ m1 AND 共享 → +L5
                let m_neg_1_similar = approx_equal(magnitude(m_minus_1), magnitude(m1));
                if m_neg_1_similar && cond_c {
                    add_or_promote(cands, StructureLabel::L5, Certainty::Primary);
                }
            }
        }
        // Branch 7:m2 < 61.8% retraced AND m2 mag 在 61.8-161.8% m0 AND m(-1) < m0 AND m(-1) ≤ 161.8% m0
        //   → add x:c3
        if let Some(m_minus_1) = ctx.m_minus_1 {
            let m2_retrace = retracement_pct(&m2.monowave, &m3.monowave);
            let m2_mag_range = (FIB_618..=FIB_1618).contains(&mag_ratio(m2, m0));
            let m_neg_1_lt_m0 = magnitude(m_minus_1) < magnitude(m0);
            let m_neg_1_le_1618 = mag_ratio(m_minus_1, m0) <= FIB_1618;
            if m2_retrace < FIB_618 && m2_mag_range && m_neg_1_lt_m0 && m_neg_1_le_1618 {
                add_or_promote(cands, StructureLabel::XC3, Certainty::Primary);
            }
        }
        // Branch 8:m2(plus 1)在自身時間內被完全回測 AND m3 < 61.8% retraced AND m(-1) < m0
        //          AND m(-1)/m1 共享 AND m0 not shortest AND market returns → add :sL3
        if completely_retraced_plus_one_time_unit(m2, m3) {
            if let (Some(m_minus_1), Some(m_minus_2), Some(m4)) =
                (ctx.m_minus_1, ctx.m_minus_2, ctx.m4)
            {
                let cond_a = retracement_pct(&m3.monowave, &m4.monowave) <= FIB_618;
                let cond_b = magnitude(m_minus_1) < magnitude(m0);
                let cond_c = share_price_range(&m_minus_1.monowave, &m1.monowave);
                let cond_d = !is_shortest_of_three(m0, Some(m_minus_2), Some(m2));
                let dur_chain =
                    duration(m_minus_2) + duration(m_minus_1) + duration(m0) + duration(m1) + duration(m2);
                let cond_e = retracement_pct(&m_minus_2.monowave, &m3.monowave) >= 1.0
                    && duration(m3) * 2 <= dur_chain;
                if cond_a && cond_b && cond_c && cond_d && cond_e {
                    add_or_promote(cands, StructureLabel::SL3, Certainty::Primary);
                }
            }
        }
    }
}
