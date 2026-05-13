// rule_5.rs — Rule 5(100% ≤ m2 < 161.8% m1)所有 Condition 與 branch
//
// 對齊 m3Spec/neely_rules.md §Rule 5(828-900 行)。
// Headline:`{any Structure possible;條件不符時改用 Position Indicator Sequences}`
//
// 子分支 (A) / (B) 依賴「m2 / m3 含 > 3 monowaves」判定。
// [缺資料]:單一 monowave 是否含 > 3 子 monowaves 需 Compaction(P6+)。
// Phase 2 PR 視為 false(視為 ≤ 3),走 (B) 分支。

use super::context::MonowaveContext;
use super::predicates::*;
use crate::output::{Certainty, MonowaveDirection, StructureLabel, StructureLabelCandidate};

pub fn run(ctx: &MonowaveContext, cands: &mut Vec<StructureLabelCandidate>) {
    let Some(m0) = ctx.m0 else { return };
    let ratio = mag_ratio(m0, ctx.m1);

    if ratio < FIB_1000 {
        cond_5a(ctx, cands);
    } else if ratio < FIB_1618 {
        cond_5b(ctx, cands);
    } else if ratio <= FIB_2618 {
        cond_5c(ctx, cands);
    } else {
        cond_5d(ctx, cands);
    }
}

// ---------------------------------------------------------------------------
// Condition 5a — m0 < 100% m1
// 子分支 (A) m2 > 3 monowaves: 12 branches
// 子分支 (B) m2 ≤ 3 monowaves: 5 branches
// ---------------------------------------------------------------------------

fn cond_5a(ctx: &MonowaveContext, cands: &mut Vec<StructureLabelCandidate>) {
    // [缺資料] m2 polywave 偵測。Phase 2 跑 (B) 分支(預設 ≤ 3)
    cond_5a_b(ctx, cands);
}

fn cond_5a_b(ctx: &MonowaveContext, cands: &mut Vec<StructureLabelCandidate>) {
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
        // Branch 3:m1(plus 1)在自身時間內(或更短)被完全回測 → add :L5
        if completely_retraced_plus_one_time_unit(m1, m2) {
            add_or_promote(cands, StructureLabel::L5, Certainty::Primary);
        }
        // Branch 4:m1 在自身時間內被完全回測 AND m3 < m2 AND ... → add (:L3)
        if completely_retraced_within_time(m1, m2) && magnitude(m3) < magnitude(m2) {
            if let Some(m_minus_3) = ctx.m_minus_3 {
                let total_dur = duration(m_minus_3)
                    + duration(ctx.m_minus_2.unwrap_or(m_minus_3))
                    + duration(ctx.m_minus_1.unwrap_or(m_minus_3))
                    + duration(ctx.m0.unwrap_or(m_minus_3))
                    + duration(m1);
                let returns =
                    retracement_pct(&m_minus_3.monowave, &m2.monowave) >= 1.0
                        && duration(m2) * 2 <= total_dur;
                let share = ctx
                    .m0
                    .zip(ctx.m_minus_2)
                    .is_some_and(|(m0, m_neg_2)| {
                        share_price_range(&m0.monowave, &m_neg_2.monowave)
                    });
                if returns && share {
                    add_or_promote(cands, StructureLabel::L3, Certainty::Possible);
                    // m3 在 61.8-100% m2 → m2 可能 x-wave
                    let r = mag_ratio(m3, m2);
                    if (FIB_618..FIB_1000).contains(&r) {
                        add_or_promote(cands, StructureLabel::XC3, Certainty::MissingWaveBundle);
                    }
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
            let m2_within_m_neg_2 = match m1.monowave.direction {
                MonowaveDirection::Up => m2.monowave.end_price >= m_minus_2.monowave.end_price,
                MonowaveDirection::Down => m2.monowave.end_price <= m_minus_2.monowave.end_price,
                _ => false,
            };
            let m_neg_1_long = mag_ratio(m_minus_1, m1) >= FIB_618;
            let m_neg_2_lt_m_neg_1 = magnitude(m_minus_2) < magnitude(m_minus_1);
            if m1_slow && m2_within_m_neg_2 && m_neg_1_long && m_neg_2_lt_m_neg_1 {
                add_or_promote(cands, StructureLabel::F3, Certainty::Primary);
                add_or_promote(cands, StructureLabel::XC3, Certainty::MissingWaveBundle);
                let _ = m0;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Condition 5b — 100% ≤ m0 < 161.8% m1
// ---------------------------------------------------------------------------

fn cond_5b(ctx: &MonowaveContext, cands: &mut Vec<StructureLabelCandidate>) {
    // [缺資料] m3 polywave 偵測。Phase 2 跑 (B) 分支
    cond_5b_b(ctx, cands);
}

fn cond_5b_b(ctx: &MonowaveContext, cands: &mut Vec<StructureLabelCandidate>) {
    let m1 = ctx.m1;
    let Some(m0) = ctx.m0 else { return };
    if let (Some(m2), Some(m3)) = (ctx.m2, ctx.m3) {
        // Branch 1:m1 ≤ m0 OR m1 ≤ m2 時間 AND m(-2) < m(-1) → add x:c3
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
        // Branch 3:m1(plus 1)在自身時間內(或更短)被 m2 完全回測
        //          AND m2 被回測 < 61.8% OR > 100%(在 ≤ m2 時間)
        //          AND m(-1) ≥ 61.8% m1
        //          → add :L5(C-wave of Flat 完成於 m1)
        let m1_retraced = completely_retraced_plus_one_time_unit(m1, m2);
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
        // Branch 4:m1 被 m2 完全回測(在 m1 時間或更短)AND m2 被回測 < 61.8%
        //          AND m(-1) ≥ 61.8% m0 → add :L3/:L5
        if completely_retraced_within_time(m1, m2) && m2_retrace < FIB_618 {
            if let Some(m_minus_1) = ctx.m_minus_1 {
                if mag_ratio(m_minus_1, m0) >= FIB_618 {
                    add_or_promote(cands, StructureLabel::L3, Certainty::Primary);
                    add_or_promote(cands, StructureLabel::L5, Certainty::Primary);
                }
            }
        }
        // Branch 5:m1 慢於自身時間被回測 AND m2 含 ≥ 3 monowaves [缺資料 — 預設 false]
        //   → add :c3 (Triangle 中段)— Phase 2 不觸發

        // Branch 6:m2(plus 1)在自身時間內被完全回測 AND m0 不為 m(-2)/m0/m2 中最短
        //          AND 自 m2 起市場 ≤ 50% m(-2)~m2 時間內接近 m(-2) 起點 → add :sL3
        if completely_retraced_plus_one_time_unit(m2, m3) {
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
        // Branch 7:m3 在 101–161.8% m2 → Expanding Triangle 可能,F3 加方括變 [F3]
        let m3_to_m2 = mag_ratio(m3, m2);
        if (1.01..=FIB_1618).contains(&m3_to_m2) {
            change_certainty(cands, StructureLabel::F3, Certainty::Rare);
        }
    }
}

// ---------------------------------------------------------------------------
// Condition 5c — 161.8% ≤ m0 ≤ 261.8% m1(4 branches)
// ---------------------------------------------------------------------------

fn cond_5c(ctx: &MonowaveContext, cands: &mut Vec<StructureLabelCandidate>) {
    let m1 = ctx.m1;
    let Some(m0) = ctx.m0 else { return };
    if let (Some(m2), Some(m3)) = (ctx.m2, ctx.m3) {
        // Branch 1:(任何情境)→ add :F3(預設)
        add_or_promote(cands, StructureLabel::F3, Certainty::Primary);

        // Branch 2:m1(plus 1)在自身時間內(或更短)被完全回測
        //          AND m2 被回測 < 61.8% AND m2 超越 m0 長度於 ≤ m0 時間
        //          AND m(-1) 在 61.8-161.8% m0 AND m2 > m0 更垂直
        //   → add :L3/(:L5)
        if let Some(m_minus_1) = ctx.m_minus_1 {
            let m1_retraced = completely_retraced_plus_one_time_unit(m1, m2);
            let m2_retrace = retracement_pct(&m2.monowave, &m3.monowave);
            let m2_long_fast = magnitude(m2) > magnitude(m0) && duration(m2) <= duration(m0);
            let m_neg_1_in_range = (FIB_618..=FIB_1618).contains(&mag_ratio(m_minus_1, m0));
            let m2_steeper = more_vertical_and_longer(m2, m0);
            if m1_retraced && m2_retrace < FIB_618 && m2_long_fast && m_neg_1_in_range && m2_steeper
            {
                add_or_promote(cands, StructureLabel::L3, Certainty::Primary);
                add_or_promote(cands, StructureLabel::L5, Certainty::Possible);
            }
        }
        // Branch 3:m2(plus 1)在自身時間內被完全回測 AND m(-1)/m1 共享 AND m0 not shortest of m(-2)/m0/m2
        //          AND 自 m2 起市場 ≤ 50% m(-2)~m2 時間內接近 m(-2) 起點 → add :sL3
        if completely_retraced_plus_one_time_unit(m2, m3) {
            if let Some(m_minus_2) = ctx.m_minus_2 {
                let share = ctx
                    .m_minus_1
                    .is_some_and(|m_neg_1| share_price_range(&m_neg_1.monowave, &m1.monowave));
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
                if share && not_shortest && returns {
                    add_or_promote(cands, StructureLabel::SL3, Certainty::Primary);
                }
            }
        }
        // Branch 4:m3 在 101–161.8% m2 → add (:c3)(極不可能的 Expanding Triangle)
        let m3_to_m2 = mag_ratio(m3, m2);
        if (1.01..=FIB_1618).contains(&m3_to_m2) {
            add_or_promote(cands, StructureLabel::C3, Certainty::Possible);
        }
    }
}

// ---------------------------------------------------------------------------
// Condition 5d — m0 > 261.8% m1(3 branches)
// ---------------------------------------------------------------------------

fn cond_5d(ctx: &MonowaveContext, cands: &mut Vec<StructureLabelCandidate>) {
    let m1 = ctx.m1;
    let Some(m0) = ctx.m0 else { return };
    if let (Some(m2), Some(m3)) = (ctx.m2, ctx.m3) {
        // Branch 1:m0(minus 1)≤ m1 OR m2(minus 1)≤ m1,且 m1 不同時短於 m0 與 m2
        //   → add :F3
        let m0_short = duration(m0).saturating_sub(1) <= duration(m1);
        let m2_short = duration(m2).saturating_sub(1) <= duration(m1);
        let m1_not_shortest =
            !(duration(m1) < duration(m0) && duration(m1) < duration(m2));
        if (m0_short || m2_short) && m1_not_shortest {
            add_or_promote(cands, StructureLabel::F3, Certainty::Primary);
        }
        // Branch 2:m2 被回測 < 61.8% AND m2~m4 合計時間 ≤ m0 AND m2~m4 合計價程 > m0 更垂直
        //   → add (:L3)/[:L5]
        let chain_dur =
            duration(m2) + ctx.m3.map_or(0, duration) + ctx.m4.map_or(0, duration);
        let chain_mag =
            magnitude(m2) + ctx.m3.map_or(0.0, magnitude) + ctx.m4.map_or(0.0, magnitude);
        let m2_retrace = retracement_pct(&m2.monowave, &m3.monowave);
        if m2_retrace < FIB_618 && chain_dur <= duration(m0) && chain_mag > magnitude(m0) {
            add_or_promote(cands, StructureLabel::L3, Certainty::Possible);
            add_or_promote(cands, StructureLabel::L5, Certainty::Rare);
        }
        // Branch 3:m2(plus 1)在自身時間內(或更短)被完全回測 AND m(-1)/m1 共享 AND m0 not shortest
        //          AND 自 m2 起市場 ≤ 50% m(-2)~m2 時間內接近 m(-2) → add :sL3
        if completely_retraced_plus_one_time_unit(m2, m3) {
            if let Some(m_minus_2) = ctx.m_minus_2 {
                let share = ctx
                    .m_minus_1
                    .is_some_and(|m_neg_1| share_price_range(&m_neg_1.monowave, &m1.monowave));
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
                if share && not_shortest && returns {
                    add_or_promote(cands, StructureLabel::SL3, Certainty::Primary);
                }
            }
        }
    }
}
