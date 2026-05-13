// rule_4.rs — Rule 4(61.8% < m2 < 100% m1)所有 Condition × Category 與 branch
//
// 對齊 m3Spec/neely_rules.md §Rule 4(701-825 行)。
// 結構:5 Conditions(a-e)× 3 Categories(i/ii/iii based on m3/m2)
//
// Headline 通用:每個 m1 都需先檢查「m1 端點被 m2 突破嗎?」→ add x:c3
// (P2 PR best-guess:m1_endpoint_broken_by_m2 placeholder 返 false,留 P5/P10)

use super::context::MonowaveContext;
use super::predicates::*;
use crate::output::{Certainty, MonowaveDirection, StructureLabel, StructureLabelCandidate};

pub fn run(ctx: &MonowaveContext, cands: &mut Vec<StructureLabelCandidate>) {
    let Some(m0) = ctx.m0 else { return };
    let m1 = ctx.m1;
    let cond_ratio = mag_ratio(m0, m1);

    // 通用前置:m1 端點被 m2 突破 → consider x:c3
    let m1_broken = ctx
        .m2
        .is_some_and(|m2| m1_endpoint_broken_by_m2(m1, m2));

    let cat = ctx.m2.and_then(|m2| ctx.m3.map(|m3| categorize(m3, m2)));

    if cond_ratio < FIB_382 {
        cond_4a(ctx, cands, cat, m1_broken);
    } else if cond_ratio < FIB_1000 {
        cond_4b(ctx, cands, cat, m1_broken);
    } else if cond_ratio < FIB_1618 {
        cond_4c(ctx, cands, cat, m1_broken);
    } else if cond_ratio <= FIB_2618 {
        cond_4d(ctx, cands, cat, m1_broken);
    } else {
        cond_4e(ctx, cands, cat, m1_broken);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::upper_case_acronyms)] // 對齊 spec 「Cat i / ii / iii」羅馬數字命名
enum Category {
    I,   // 100% ≤ m3 < 161.8% m2
    II,  // 161.8% ≤ m3 ≤ 261.8% m2
    III, // m3 > 261.8% m2
}

fn categorize(m3: &crate::monowave::ClassifiedMonowave, m2: &crate::monowave::ClassifiedMonowave) -> Category {
    let r = mag_ratio(m3, m2);
    if r >= FIB_2618 {
        Category::III
    } else if r >= FIB_1618 {
        Category::II
    } else {
        Category::I
    }
}

// ---------------------------------------------------------------------------
// Condition 4a — m0 < 38.2% m1, Headline `{:F3/:c3/:s5/[:sL3]}`
// ---------------------------------------------------------------------------

fn cond_4a(
    ctx: &MonowaveContext,
    cands: &mut Vec<StructureLabelCandidate>,
    cat: Option<Category>,
    _m1_broken: bool,
) {
    let m1 = ctx.m1;
    match cat {
        Some(Category::I) => cat_4a_i(ctx, cands),
        Some(Category::II) => cat_4a_ii(ctx, cands),
        Some(Category::III) => cat_4a_iii(ctx, cands),
        None => {}
    }
    let _ = m1;
}

fn cat_4a_i(ctx: &MonowaveContext, cands: &mut Vec<StructureLabelCandidate>) {
    let m1 = ctx.m1;
    if let (Some(_m0), Some(_m2), Some(m3), Some(m4)) = (ctx.m0, ctx.m2, ctx.m3, ctx.m4) {
        // Branch 1:m3 在自身時間內被完全回測得慢 → add :F3/:s5
        let m3_slow_full =
            completely_retraced(&m3.monowave, &m4.monowave) && duration(m4) > duration(m3);
        if m3_slow_full {
            add_or_promote(cands, StructureLabel::F3, Certainty::Primary);
            add_or_promote(cands, StructureLabel::S5, Certainty::Primary);
            if let Some(m_minus_1) = ctx.m_minus_1 {
                if mag_ratio(m1, m_minus_1) < FIB_618 {
                    drop_label(cands, StructureLabel::S5);
                }
                if let Some(m0) = ctx.m0 {
                    if magnitude(m0) < magnitude(m_minus_1) && magnitude(m0) < magnitude(m1) {
                        drop_label(cands, StructureLabel::S5);
                    }
                }
            }
        }

        // Branch 2:m3 在自身時間內被完全回測得快 → add :F3/:c3
        let m3_fast_full =
            completely_retraced(&m3.monowave, &m4.monowave) && duration(m4) < duration(m3);
        if m3_fast_full {
            add_or_promote(cands, StructureLabel::F3, Certainty::Primary);
            add_or_promote(cands, StructureLabel::C3, Certainty::Primary);
            // m1 被回測 ≤ 70% AND m0/m2 無共享 AND m3 ≈ 161.8% m1 AND ... → add :s5
            if let (Some(m0), Some(m2)) = (ctx.m0, ctx.m2) {
                let m1_retraced_le_70 = retracement_pct(&m1.monowave, &m2.monowave) <= 0.70;
                let no_share = !share_price_range(&m0.monowave, &m2.monowave);
                let m3_near_1618 = fib_approx(mag_ratio(m3, m1), FIB_1618);
                let m0_dur_long =
                    ctx.m_minus_1.is_some_and(|m_neg_1| duration(m0) > duration(m_neg_1))
                        || duration(m0) > duration(m1);
                if m1_retraced_le_70 && no_share && m3_near_1618 && m0_dur_long {
                    add_or_promote(cands, StructureLabel::S5, Certainty::Primary);
                }
                if no_share {
                    drop_label(cands, StructureLabel::C3);
                }
            }
        }

        // Branch 3:m3 被回測 < 100% → add :F3/:s5;m2 多 monowave + 條件 → add :L5
        let m3_partial = retracement_pct(&m3.monowave, &m4.monowave) < 1.0;
        if m3_partial {
            add_or_promote(cands, StructureLabel::F3, Certainty::Primary);
            add_or_promote(cands, StructureLabel::S5, Certainty::Primary);
            // m2 含 > 3 monowaves — [缺資料]:Polywave 偵測需 Compaction(P6)
            // best-guess:暫不觸發
            if let Some(m0) = ctx.m0 {
                if let Some(m_minus_1) = ctx.m_minus_1 {
                    if magnitude(m0) < magnitude(m_minus_1) && magnitude(m0) < magnitude(m1) {
                        drop_label(cands, StructureLabel::S5);
                    }
                }
            }
        }
    }
}

fn cat_4a_ii(ctx: &MonowaveContext, cands: &mut Vec<StructureLabelCandidate>) {
    let m1 = ctx.m1;
    if let (Some(m0), Some(m2), Some(m3), Some(m4)) = (ctx.m0, ctx.m2, ctx.m3, ctx.m4) {
        // Branch 1:m(-1) > 261.8% m1 → 只 :F3
        if let Some(m_minus_1) = ctx.m_minus_1 {
            if mag_ratio(m_minus_1, m1) > FIB_2618 {
                cands.clear();
                add_or_promote(cands, StructureLabel::F3, Certainty::Primary);
                return;
            }
        }
        // Branch 2:m4 > m3 → 只 :F3
        if magnitude(m4) > magnitude(m3) {
            cands.clear();
            add_or_promote(cands, StructureLabel::F3, Certainty::Primary);
            return;
        }
        // Branch 3:m3 被回測 < 100% → add :s5 後依細分(3a-3d)
        if retracement_pct(&m3.monowave, &m4.monowave) < 1.0 {
            add_or_promote(cands, StructureLabel::S5, Certainty::Primary);
            // 3a-3d 細分本 PR 不做精細區分(留 P5);通用 :s5 已添加
        }
        let _ = m0;
        let _ = m2;
    }
}

fn cat_4a_iii(ctx: &MonowaveContext, cands: &mut Vec<StructureLabelCandidate>) {
    let m1 = ctx.m1;
    if let (Some(m3), Some(m4)) = (ctx.m3, ctx.m4) {
        // Branch 1:m(-1) > 261.8% m1 → 只 :F3
        if let Some(m_minus_1) = ctx.m_minus_1 {
            if mag_ratio(m_minus_1, m1) > FIB_2618 {
                cands.clear();
                add_or_promote(cands, StructureLabel::F3, Certainty::Primary);
                return;
            }
        }
        // Branch 2:m3 完全被回測 → 只 :F3
        if completely_retraced(&m3.monowave, &m4.monowave) {
            cands.clear();
            add_or_promote(cands, StructureLabel::F3, Certainty::Primary);
            return;
        }
        // Branch 3:m3 被回測 < 100% → 只 :s5(罕見)
        if retracement_pct(&m3.monowave, &m4.monowave) < 1.0 {
            cands.clear();
            add_or_promote(cands, StructureLabel::S5, Certainty::Rare);
        }
    }
}

// ---------------------------------------------------------------------------
// Condition 4b — 38.2% ≤ m0 < 100% m1, Headline `{:F3/:c3/:s5/(:sL3)/(x:c3)/[:L5]}`
//   含 [:L5] 5th-of-5th 共通條件(多處)
// ---------------------------------------------------------------------------

fn cond_4b(
    ctx: &MonowaveContext,
    cands: &mut Vec<StructureLabelCandidate>,
    cat: Option<Category>,
    m1_broken: bool,
) {
    match cat {
        Some(Category::I) => cat_4b_i(ctx, cands, m1_broken),
        Some(Category::II) => cat_4b_ii(ctx, cands, m1_broken),
        Some(Category::III) => cat_4b_iii(ctx, cands, m1_broken),
        None => {}
    }
}

fn add_l5_if_fifth_of_fifth(ctx: &MonowaveContext, cands: &mut Vec<StructureLabelCandidate>) {
    if let (Some(m_minus_3), Some(m_minus_2), Some(m_minus_1), Some(m0), Some(m2)) =
        (ctx.m_minus_3, ctx.m_minus_2, ctx.m_minus_1, ctx.m0, ctx.m2)
    {
        let m1 = ctx.m1;
        let m1_longest = is_longest_of_three(m1, Some(m_minus_1), Some(m_minus_3));
        let breaches = m2_breaches_2_4_line_within_m1_time(m_minus_2, m0, m1, m2);
        if m1_longest && breaches {
            add_or_promote(cands, StructureLabel::L5, Certainty::Rare);
        }
    }
}

fn cat_4b_i(ctx: &MonowaveContext, cands: &mut Vec<StructureLabelCandidate>, m1_broken: bool) {
    let m1 = ctx.m1;
    if let (Some(m3), Some(m4)) = (ctx.m3, ctx.m4) {
        // Branch 1:m3 在自身時間內被完全回測快 → 只 :F3/:c3
        if completely_retraced(&m3.monowave, &m4.monowave) && duration(m4) < duration(m3) {
            cands.clear();
            add_or_promote(cands, StructureLabel::F3, Certainty::Primary);
            add_or_promote(cands, StructureLabel::C3, Certainty::Primary);
            if m1_broken {
                add_or_promote(cands, StructureLabel::L5, Certainty::Rare);
            }
            return;
        }
        // Branch 2:m3 完全被回測得慢 → add :F3/:c3/:s5
        if completely_retraced(&m3.monowave, &m4.monowave) && duration(m4) > duration(m3) {
            add_or_promote(cands, StructureLabel::F3, Certainty::Primary);
            add_or_promote(cands, StructureLabel::C3, Certainty::Primary);
            add_or_promote(cands, StructureLabel::S5, Certainty::Primary);
            if m1_broken {
                prefix_c3_with_x(cands);
                add_or_promote(cands, StructureLabel::L5, Certainty::Rare);
            }
            if let Some(m_minus_1) = ctx.m_minus_1 {
                if mag_ratio(m1, m_minus_1) < FIB_618 {
                    drop_label(cands, StructureLabel::S5);
                }
                if mag_ratio(m_minus_1, m1) >= FIB_1618
                    && retracement_pct(&m3.monowave, &m4.monowave) < FIB_618
                {
                    drop_label(cands, StructureLabel::F3);
                }
                if let Some(m0) = ctx.m0 {
                    if magnitude(m0) < magnitude(m_minus_1) && magnitude(m0) < magnitude(m1) {
                        drop_label(cands, StructureLabel::S5);
                    }
                }
            }
        }
        // Branch 3:m3 被回測 < 100% → add :c3/:s5
        if retracement_pct(&m3.monowave, &m4.monowave) < 1.0 {
            add_or_promote(cands, StructureLabel::C3, Certainty::Primary);
            add_or_promote(cands, StructureLabel::S5, Certainty::Primary);
            if m1_broken {
                prefix_c3_with_x(cands);
            }
            add_l5_if_fifth_of_fifth(ctx, cands);
        }
        // Branch 4:m3 被回測 < 61.8% → add :c3/:sL3/:s5
        if retracement_pct(&m3.monowave, &m4.monowave) < FIB_618 {
            add_or_promote(cands, StructureLabel::C3, Certainty::Primary);
            add_or_promote(cands, StructureLabel::SL3, Certainty::Primary);
            add_or_promote(cands, StructureLabel::S5, Certainty::Primary);
            if m1_broken {
                prefix_c3_with_x(cands);
                add_or_promote(cands, StructureLabel::L5, Certainty::Rare);
            }
            // m3~m5 未達 161.8% m1 → drop :sL3
            let chain = magnitude(m3)
                + ctx.m4.map_or(0.0, magnitude)
                + ctx.m5.map_or(0.0, magnitude);
            if chain < FIB_1618 * magnitude(m1) {
                drop_label(cands, StructureLabel::SL3);
            }
        }
        // Branch 5 fallback:四條皆不適用 → add :F3/:c3/:sL3/:s5
        // 簡化:若 candidates 為空,使用 fallback
        if cands.is_empty() {
            add_or_promote(cands, StructureLabel::F3, Certainty::Primary);
            add_or_promote(cands, StructureLabel::C3, Certainty::Primary);
            add_or_promote(cands, StructureLabel::SL3, Certainty::Primary);
            add_or_promote(cands, StructureLabel::S5, Certainty::Primary);
            if m1_broken {
                add_or_promote(cands, StructureLabel::XC3, Certainty::Possible);
            }
            add_l5_if_fifth_of_fifth(ctx, cands);
        }
    }
}

fn cat_4b_ii(ctx: &MonowaveContext, cands: &mut Vec<StructureLabelCandidate>, m1_broken: bool) {
    let m1 = ctx.m1;
    if let (Some(m3), Some(m4)) = (ctx.m3, ctx.m4) {
        // Branch 1:m(-1) > 261.8% m1 → 只 :F3/:c3
        if let Some(m_minus_1) = ctx.m_minus_1 {
            if mag_ratio(m_minus_1, m1) > FIB_2618 {
                cands.clear();
                add_or_promote(cands, StructureLabel::F3, Certainty::Primary);
                add_or_promote(cands, StructureLabel::C3, Certainty::Primary);
                if m1_broken {
                    prefix_c3_with_x(cands);
                }
                return;
            }
        }
        // Branch 2:m1 最長 + breach → add [:L5]
        add_l5_if_fifth_of_fifth(ctx, cands);
        // Branch 3:m3 被回測 < 61.8% → 只 :c3/(:sL3)/(:s5)
        if retracement_pct(&m3.monowave, &m4.monowave) < FIB_618 {
            cands.retain(|c| matches!(c.label, StructureLabel::L5));
            add_or_promote(cands, StructureLabel::C3, Certainty::Primary);
            add_or_promote(cands, StructureLabel::SL3, Certainty::Possible);
            add_or_promote(cands, StructureLabel::S5, Certainty::Possible);
            if m1_broken {
                prefix_c3_with_x(cands);
            }
            let chain = magnitude(m3)
                + ctx.m4.map_or(0.0, magnitude)
                + ctx.m5.map_or(0.0, magnitude);
            if chain < FIB_1618 * magnitude(m1) {
                drop_label(cands, StructureLabel::SL3);
            }
        }
        // Branch 4 fallback
        if cands.is_empty()
            || !cands
                .iter()
                .any(|c| matches!(c.label, StructureLabel::F3 | StructureLabel::C3))
        {
            add_or_promote(cands, StructureLabel::F3, Certainty::Primary);
            add_or_promote(cands, StructureLabel::C3, Certainty::Primary);
            add_or_promote(cands, StructureLabel::SL3, Certainty::Primary);
            add_or_promote(cands, StructureLabel::S5, Certainty::Primary);
            if m1_broken {
                add_or_promote(cands, StructureLabel::XC3, Certainty::Possible);
            }
            add_l5_if_fifth_of_fifth(ctx, cands);
        }
    }
}

fn cat_4b_iii(ctx: &MonowaveContext, cands: &mut Vec<StructureLabelCandidate>, m1_broken: bool) {
    let m1 = ctx.m1;
    if let (Some(m0), Some(m3), Some(m4)) = (ctx.m0, ctx.m3, ctx.m4) {
        // Branch 1:m(-1) > 261.8% m1 → 只 :c3/(:F3)
        if let Some(m_minus_1) = ctx.m_minus_1 {
            if mag_ratio(m_minus_1, m1) > FIB_2618 {
                cands.clear();
                add_or_promote(cands, StructureLabel::C3, Certainty::Primary);
                add_or_promote(cands, StructureLabel::F3, Certainty::Possible);
                if m1_broken {
                    prefix_c3_with_x(cands);
                }
                return;
            }
        }
        // Branch 2:m(-1) ≥ 161.8% m1 AND m0 被回測慢 AND m1 耗時 ≥ 161.8% m0
        //   → Irregular Failure Flat 機率高;:c3/(:F3);m1 端點被 m2 突破 → x:c3
        if let Some(m_minus_1) = ctx.m_minus_1 {
            let m_neg_1_long = mag_ratio(m_minus_1, m1) >= FIB_1618;
            let m0_slow = ctx.m1.metrics.duration_bars >= (FIB_1618 * duration(m0) as f64) as usize;
            if m_neg_1_long && m0_slow {
                add_or_promote(cands, StructureLabel::C3, Certainty::Primary);
                add_or_promote(cands, StructureLabel::F3, Certainty::Possible);
                if m1_broken {
                    prefix_c3_with_x(cands);
                }
            }
        }
        // Branch 3:m1 最長 + breach → add [:L5]
        add_l5_if_fifth_of_fifth(ctx, cands);
        // Branch 4:m3 被回測 < 61.8% → 只 :F3/:c3/(:s5)
        if retracement_pct(&m3.monowave, &m4.monowave) < FIB_618 {
            cands.retain(|c| {
                matches!(
                    c.label,
                    StructureLabel::L5 | StructureLabel::XC3 | StructureLabel::F3 | StructureLabel::C3
                )
            });
            add_or_promote(cands, StructureLabel::F3, Certainty::Primary);
            add_or_promote(cands, StructureLabel::C3, Certainty::Primary);
            add_or_promote(cands, StructureLabel::S5, Certainty::Possible);
            if m1_broken {
                prefix_c3_with_x(cands);
            }
        }
        // Branch 5 其他 fallback
        if cands.is_empty() {
            add_or_promote(cands, StructureLabel::F3, Certainty::Primary);
            add_or_promote(cands, StructureLabel::C3, Certainty::Primary);
            add_or_promote(cands, StructureLabel::SL3, Certainty::Primary);
            add_or_promote(cands, StructureLabel::S5, Certainty::Primary);
            if m1_broken {
                prefix_c3_with_x(cands);
            }
            add_l5_if_fifth_of_fifth(ctx, cands);
        }
    }
}

// ---------------------------------------------------------------------------
// Condition 4c — 100% ≤ m0 < 161.8% m1, Headline `{:c3/(:F3)/(x:c3)}`
// ---------------------------------------------------------------------------

fn cond_4c(
    ctx: &MonowaveContext,
    cands: &mut Vec<StructureLabelCandidate>,
    cat: Option<Category>,
    m1_broken: bool,
) {
    let m1 = ctx.m1;
    match cat {
        Some(Category::I) => {
            // Cat 4c-i:add :F3/:c3;m1 端點被 m2 突破 → :c3 前加 x
            add_or_promote(cands, StructureLabel::F3, Certainty::Possible);
            add_or_promote(cands, StructureLabel::C3, Certainty::Primary);
            if m1_broken {
                prefix_c3_with_x(cands);
            }
        }
        Some(Category::II) => {
            // Cat 4c-ii:m2 在自身時間內被完全回測 AND m3 > 161.8% m1 → add :c3/(:F3)
            if let (Some(m2), Some(m3)) = (ctx.m2, ctx.m3) {
                if completely_retraced_within_time(m2, m3) && mag_ratio(m3, m1) > FIB_1618 {
                    add_or_promote(cands, StructureLabel::C3, Certainty::Primary);
                    add_or_promote(cands, StructureLabel::F3, Certainty::Possible);
                    if m1_broken {
                        prefix_c3_with_x(cands);
                    }
                } else {
                    add_or_promote(cands, StructureLabel::F3, Certainty::Primary);
                    add_or_promote(cands, StructureLabel::C3, Certainty::Primary);
                    add_or_promote(cands, StructureLabel::XC3, Certainty::Possible);
                }
            }
        }
        Some(Category::III) => {
            // Cat 4c-iii:m2 在自身時間內被完全回測 → almost certain C-Failure Flat 中段 / Non-Limiting Triangle 中段
            if let (Some(m2), Some(m3)) = (ctx.m2, ctx.m3) {
                if completely_retraced_within_time(m2, m3) {
                    add_or_promote(cands, StructureLabel::C3, Certainty::Primary);
                    add_or_promote(cands, StructureLabel::F3, Certainty::Rare);
                    if m1_broken {
                        prefix_c3_with_x(cands);
                    }
                    // [:F3] 僅在 m3 被快速 > 61.8% retraced 時可考慮
                    if let Some(m4) = ctx.m4 {
                        let m3_retraced_fast =
                            retracement_pct(&m3.monowave, &m4.monowave) > FIB_618
                                && duration(m4) <= duration(m3);
                        if !m3_retraced_fast {
                            drop_label(cands, StructureLabel::F3);
                        }
                    }
                }
            }
        }
        None => {}
    }
    let _ = m1;
}

// ---------------------------------------------------------------------------
// Condition 4d — 161.8% ≤ m0 ≤ 261.8% m1, Headline `{:F3/(:c3)/(x:c3)}`
// ---------------------------------------------------------------------------

fn cond_4d(
    ctx: &MonowaveContext,
    cands: &mut Vec<StructureLabelCandidate>,
    cat: Option<Category>,
    m1_broken: bool,
) {
    match cat {
        Some(Category::I) | Some(Category::II) => cat_4d_i_ii(ctx, cands, m1_broken),
        Some(Category::III) => cat_4d_iii(ctx, cands, m1_broken),
        None => {}
    }
}

fn cat_4d_i_ii(ctx: &MonowaveContext, cands: &mut Vec<StructureLabelCandidate>, m1_broken: bool) {
    let m1 = ctx.m1;
    if let (Some(m0), Some(m2), Some(m3), Some(m4)) = (ctx.m0, ctx.m2, ctx.m3, ctx.m4) {
        // Branch 1:m2 在自身時間內被完全回測 AND m3 被回測 ≤ 61.8%
        //          AND m3(或 m3~m5)在 ≤ m1 時間內 ≥ 161.8% m1 價長
        //   → m0 中心 missing x-wave 場景;add :F3/[:c3] + bundled markers
        let m2_retraced = completely_retraced_within_time(m2, m3);
        let m3_short_retrace = retracement_pct(&m3.monowave, &m4.monowave) <= FIB_618;
        let chain = magnitude(m3)
            + ctx.m4.map_or(0.0, magnitude)
            + ctx.m5.map_or(0.0, magnitude);
        let chain_dur = duration(m3)
            + ctx.m4.map_or(0, duration)
            + ctx.m5.map_or(0, duration);
        let chain_covers =
            chain >= FIB_1618 * magnitude(m1) && chain_dur <= duration(m1);
        if m2_retraced && m3_short_retrace && chain_covers {
            add_or_promote(cands, StructureLabel::F3, Certainty::Primary);
            add_or_promote(cands, StructureLabel::C3, Certainty::Rare);
            // bundled markers
            add_or_promote(cands, StructureLabel::XC3, Certainty::MissingWaveBundle);
            add_or_promote(cands, StructureLabel::S5, Certainty::MissingWaveBundle);
        }
        // Branch 2:m2 慢於自身時間被回測 → add :F3/:c3
        let m2_slow_retrace = duration(m3) > duration(m2)
            && retracement_pct(&m2.monowave, &m3.monowave) >= 1.0;
        if m2_slow_retrace {
            add_or_promote(cands, StructureLabel::F3, Certainty::Primary);
            add_or_promote(cands, StructureLabel::C3, Certainty::Primary);
            if m1_broken {
                prefix_c3_with_x(cands);
            }
        }
        // Branch 3:m3(plus 1 time unit)在自身時間內(或更短)被完全回測 → :F3 唯一
        if completely_retraced_plus_one_time_unit(m3, m4) {
            cands.clear();
            add_or_promote(cands, StructureLabel::F3, Certainty::Primary);
            return;
        }
        // Branch 4:m3 被回測 ≥ 61.8% 但 < 100% → :F3 唯一
        let retrace_pct = retracement_pct(&m3.monowave, &m4.monowave);
        if (FIB_618..1.0).contains(&retrace_pct) {
            cands.clear();
            add_or_promote(cands, StructureLabel::F3, Certainty::Primary);
            return;
        }
        // Branch 5:m3 被回測 < 61.8% → add :F3;m5 條件下加暗示
        if retrace_pct < FIB_618 {
            add_or_promote(cands, StructureLabel::F3, Certainty::Primary);
            // m5 不為最長 + 完全回測 → Terminal pattern → 暫不細分
            if let Some(_m5) = ctx.m5 {
                // 暗示 Terminal,Phase 2 不加新 label
            }
        }
        let _ = m0;
    }
}

fn cat_4d_iii(ctx: &MonowaveContext, cands: &mut Vec<StructureLabelCandidate>, m1_broken: bool) {
    let m1 = ctx.m1;
    if let (Some(m2), Some(m3), Some(m4)) = (ctx.m2, ctx.m3, ctx.m4) {
        // Branch 1:m3 耗時 ≤ m1 AND m2 在自身時間內被完全回測 → add :c3;m3 retraced ≥ 61.8% → +F3
        if duration(m3) <= duration(m1) && completely_retraced_within_time(m2, m3) {
            add_or_promote(cands, StructureLabel::C3, Certainty::Primary);
            if m1_broken {
                prefix_c3_with_x(cands);
            }
            if retracement_pct(&m3.monowave, &m4.monowave) >= FIB_618 {
                add_or_promote(cands, StructureLabel::F3, Certainty::Possible);
            }
        }
        // Branch 2:m3 耗時 > m1 → add :F3/:c3
        if duration(m3) > duration(m1) {
            add_or_promote(cands, StructureLabel::F3, Certainty::Primary);
            add_or_promote(cands, StructureLabel::C3, Certainty::Primary);
            if m1_broken {
                prefix_c3_with_x(cands);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Condition 4e — m0 > 261.8% m1, Headline `{:F3/(x:c3)/[:c3]}`
// ---------------------------------------------------------------------------

fn cond_4e(
    ctx: &MonowaveContext,
    cands: &mut Vec<StructureLabelCandidate>,
    cat: Option<Category>,
    m1_broken: bool,
) {
    match cat {
        Some(Category::I) | Some(Category::II) => cat_4e_i_ii(ctx, cands, m1_broken),
        Some(Category::III) => cat_4e_iii(ctx, cands, m1_broken),
        None => {}
    }
}

fn cat_4e_i_ii(ctx: &MonowaveContext, cands: &mut Vec<StructureLabelCandidate>, m1_broken: bool) {
    let m1 = ctx.m1;
    if let (Some(m0), Some(m2), Some(m3), Some(m4)) = (ctx.m0, ctx.m2, ctx.m3, ctx.m4) {
        // Branch 1:m3(plus 1)在自身時間內(或更短)被完全回測 → :F3 唯一
        if completely_retraced_plus_one_time_unit(m3, m4) {
            cands.clear();
            add_or_promote(cands, StructureLabel::F3, Certainty::Primary);
            return;
        }
        // Branch 2:m3 ≤ 161.8% m2 AND m3 不被完全回測 AND m4 比自身時間快被回測
        //   → add x:c3
        let cond_a = mag_ratio(m3, m2) <= FIB_1618;
        let cond_b = retracement_pct(&m3.monowave, &m4.monowave) < 1.0;
        let cond_c = ctx
            .m5
            .is_some_and(|m5| {
                completely_retraced(&m4.monowave, &m5.monowave) && duration(m5) < duration(m4)
            });
        if cond_a && cond_b && cond_c {
            add_or_promote(cands, StructureLabel::XC3, Certainty::Primary);
        }
        // Branch 3/4:m0 中心 missing x-wave 場景
        let m2_retraced = completely_retraced_within_time(m2, m3);
        let m_neg_1_short = ctx
            .m_minus_1
            .is_some_and(|m_neg_1| mag_ratio(m_neg_1, m0) <= FIB_618);
        let m3_short_retrace = retracement_pct(&m3.monowave, &m4.monowave) <= FIB_618;
        let chain_covers = magnitude(m3) + ctx.m4.map_or(0.0, magnitude) + ctx.m5.map_or(0.0, magnitude)
            >= magnitude(m1);
        if m2_retraced && m_neg_1_short && m3_short_retrace && chain_covers {
            add_or_promote(cands, StructureLabel::F3, Certainty::Primary);
            add_or_promote(cands, StructureLabel::C3, Certainty::Rare);
            add_or_promote(cands, StructureLabel::XC3, Certainty::MissingWaveBundle);
            add_or_promote(cands, StructureLabel::S5, Certainty::MissingWaveBundle);
            if m1_broken {
                prefix_c3_with_x(cands);
            }
        }
        // Branch 5:m2 慢於自身時間被回測 → multi Flat/Triangle;add :F3
        let m2_slow = duration(m3) > duration(m2)
            && retracement_pct(&m2.monowave, &m3.monowave) >= 1.0;
        if m2_slow {
            add_or_promote(cands, StructureLabel::F3, Certainty::Primary);
        }
        // Branch 6:m0 為 polywave → add x:c3 [缺資料 polywave 偵測,P6+]
        // Branch 7:m(-1) ≤ 61.8% m0 → add x:c3
        if m_neg_1_short {
            add_or_promote(cands, StructureLabel::XC3, Certainty::Primary);
        }
    }
}

fn cat_4e_iii(ctx: &MonowaveContext, cands: &mut Vec<StructureLabelCandidate>, _m1_broken: bool) {
    let m1 = ctx.m1;
    if let (Some(m2), Some(m3), Some(m4)) = (ctx.m2, ctx.m3, ctx.m4) {
        // Branch 1:m3 耗時 ≤ m1 AND m2 在自身時間內被完全回測 → add x:c3
        //   若 m3~m5 未超 m0 起點 AND m3 被回測 ≥ 61.8% → +F3
        if duration(m3) <= duration(m1) && completely_retraced_within_time(m2, m3) {
            add_or_promote(cands, StructureLabel::XC3, Certainty::Primary);
            if let Some(m0) = ctx.m0 {
                let chain_end = m3.monowave.end_price; // simplified — m3~m5 chain end
                let crosses_m0_start = match m0.monowave.direction {
                    MonowaveDirection::Up => chain_end < m0.monowave.start_price,
                    MonowaveDirection::Down => chain_end > m0.monowave.start_price,
                    _ => false,
                };
                if !crosses_m0_start && retracement_pct(&m3.monowave, &m4.monowave) >= FIB_618 {
                    add_or_promote(cands, StructureLabel::F3, Certainty::Possible);
                }
            }
        }
    }
}
