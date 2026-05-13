// rule_3.rs — Rule 3(m2 ≈ 61.8% m1,±4% 容差,臨界區)所有 Condition 與 branch
//
// 對齊 m3Spec/neely_rules.md §Rule 3(634-697 行)。
// Headline:`{:F3/:c3/:s5/:5/(:sL3)/[:L5]}`
//
// 「5th-of-5th Extension」共通條件(只在 Cond 3a 套用於每一條子規則):
//   IF m1 為 m(-1)/m1/m(-3) 中最長 AND m2 在 ≤ m1 時間內突破 m(-2)/m0 連線
//   → add [:L5]

use super::context::MonowaveContext;
use super::predicates::*;
use crate::output::{Certainty, MonowaveDirection, StructureLabel, StructureLabelCandidate};

pub fn run(ctx: &MonowaveContext, cands: &mut Vec<StructureLabelCandidate>) {
    let Some(m0) = ctx.m0 else { return };
    let ratio = mag_ratio(m0, ctx.m1);

    if ratio < FIB_382 {
        cond_3a(ctx, cands);
    } else if ratio < FIB_618 {
        cond_3b(ctx, cands);
    } else if ratio < FIB_1000 {
        cond_3c(ctx, cands);
    } else if ratio <= FIB_1618 {
        cond_3d(ctx, cands);
    } else if ratio <= FIB_2618 {
        cond_3e(ctx, cands);
    } else {
        cond_3f(ctx, cands);
    }
}

/// 5th-of-5th Extension 共通檢測(spec 共通條件,Cond 3a 每子規則套用)。
///
/// IF m1 為 m(-1)/m1/m(-3) 中最長 AND m2 在 ≤ m1 時間內突破 m(-2)/m0 連線
/// → 該子規則的 Structure list 額外加 [:L5]
fn check_fifth_of_fifth_and_add(
    ctx: &MonowaveContext,
    cands: &mut Vec<StructureLabelCandidate>,
) {
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

// ---------------------------------------------------------------------------
// Condition 3a — m0 < 38.2% m1(6 branches + 5th-of-5th 共通)
// ---------------------------------------------------------------------------

fn cond_3a(ctx: &MonowaveContext, cands: &mut Vec<StructureLabelCandidate>) {
    let m1 = ctx.m1;

    // Branch 1:m3 > 261.8% m1 → add :c3/(:s5)
    if let Some(m3) = ctx.m3 {
        let m3_ratio = mag_ratio(m3, m1);
        if m3_ratio > FIB_2618 {
            add_or_promote(cands, StructureLabel::C3, Certainty::Primary);
            add_or_promote(cands, StructureLabel::S5, Certainty::Possible);
            // m(-1) > 161.8% m1 → drop :s5
            if let Some(m_minus_1) = ctx.m_minus_1 {
                if mag_ratio(m_minus_1, m1) > FIB_1618 {
                    drop_label(cands, StructureLabel::S5);
                }
            }
            check_fifth_of_fifth_and_add(ctx, cands);
        }

        // Branch 2:m3 在 161.8–261.8% m1(含)→ add :s5/:c3/:F3
        if (FIB_1618..=FIB_2618).contains(&m3_ratio) {
            add_or_promote(cands, StructureLabel::S5, Certainty::Primary);
            add_or_promote(cands, StructureLabel::C3, Certainty::Primary);
            add_or_promote(cands, StructureLabel::F3, Certainty::Primary);
            // m(-1) > m3 → drop :c3
            if let Some(m_minus_1) = ctx.m_minus_1 {
                if magnitude(m_minus_1) > magnitude(m3) {
                    drop_label(cands, StructureLabel::C3);
                }
                // m(-1) > m1 → :s5 僅可為 c-wave of Zigzag in Complex(m2 為 x-wave)
                if magnitude(m_minus_1) > magnitude(m1) {
                    change_certainty(cands, StructureLabel::S5, Certainty::Possible);
                }
            }
            check_fifth_of_fifth_and_add(ctx, cands);
        }

        // Branch 3:m3 在 100–161.8% m1 → add :F3/:5/:s5
        if (FIB_1000..FIB_1618).contains(&m3_ratio) {
            add_or_promote(cands, StructureLabel::F3, Certainty::Primary);
            add_or_promote(cands, StructureLabel::Five, Certainty::Primary);
            add_or_promote(cands, StructureLabel::S5, Certainty::Primary);
            // m4 < m3 → drop :F3
            if let Some(m4) = ctx.m4 {
                if magnitude(m4) < magnitude(m3) {
                    drop_label(cands, StructureLabel::F3);
                }
            }
            // m0 同時 < m(-1) 與 < m1 → drop :s5
            if let (Some(m_minus_1), Some(m0)) = (ctx.m_minus_1, ctx.m0) {
                if magnitude(m0) < magnitude(m_minus_1) && magnitude(m0) < magnitude(m1) {
                    drop_label(cands, StructureLabel::S5);
                }
            }
            check_fifth_of_fifth_and_add(ctx, cands);
        }

        // Branch 4:m3 < m1 AND m3(plus 1 time unit)在自身時間內(或更短)被完全回測
        //   → add :5/:F3
        if let Some(m4) = ctx.m4 {
            if magnitude(m3) < magnitude(m1) && completely_retraced_plus_one_time_unit(m3, m4) {
                add_or_promote(cands, StructureLabel::Five, Certainty::Primary);
                add_or_promote(cands, StructureLabel::F3, Certainty::Primary);
                check_fifth_of_fifth_and_add(ctx, cands);
            }
        }

        // Branch 5:m3 < m1 AND m3 慢於自身時間被回測 → add :s5
        if let Some(m4) = ctx.m4 {
            let slow_retrace = completely_retraced(&m3.monowave, &m4.monowave)
                && duration(m4) > duration(m3);
            if magnitude(m3) < magnitude(m1) && slow_retrace {
                add_or_promote(cands, StructureLabel::S5, Certainty::Primary);
                check_fifth_of_fifth_and_add(ctx, cands);
            }
        }

        // Branch 6:m3 < m1 AND m4 < m3 → add :s5/:F3
        if let Some(m4) = ctx.m4 {
            if magnitude(m3) < magnitude(m1) && magnitude(m4) < magnitude(m3) {
                add_or_promote(cands, StructureLabel::S5, Certainty::Primary);
                add_or_promote(cands, StructureLabel::F3, Certainty::Primary);
                // m5 > m3 → drop :F3
                if let Some(m5) = ctx.m5 {
                    if magnitude(m5) > magnitude(m3) {
                        drop_label(cands, StructureLabel::F3);
                    }
                }
                check_fifth_of_fifth_and_add(ctx, cands);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Condition 3b — 38.2% ≤ m0 < 61.8% m1(6 branches,無 [:L5] 共通條件)
// ---------------------------------------------------------------------------

fn cond_3b(ctx: &MonowaveContext, cands: &mut Vec<StructureLabelCandidate>) {
    let m1 = ctx.m1;
    if let Some(m3) = ctx.m3 {
        let m3_ratio = mag_ratio(m3, m1);

        // Branch 1:m3 > 261.8% m1 → add :c3/(:s5)
        if m3_ratio > FIB_2618 {
            add_or_promote(cands, StructureLabel::C3, Certainty::Primary);
            add_or_promote(cands, StructureLabel::S5, Certainty::Possible);
            if let Some(m_minus_1) = ctx.m_minus_1 {
                if mag_ratio(m_minus_1, m1) > FIB_1618 {
                    drop_label(cands, StructureLabel::S5);
                }
            }
        }

        // Branch 2:m3 在 161.8–261.8% m1(含) → add :c3/:s5
        if (FIB_1618..=FIB_2618).contains(&m3_ratio) {
            add_or_promote(cands, StructureLabel::C3, Certainty::Primary);
            add_or_promote(cands, StructureLabel::S5, Certainty::Primary);
            if let Some(m_minus_1) = ctx.m_minus_1 {
                if magnitude(m_minus_1) > magnitude(m1) {
                    // 排除 Terminal:c3 留 (但不主動 drop,留 cert 強化)
                    change_certainty(cands, StructureLabel::C3, Certainty::Possible);
                }
            }
        }

        // Branch 3:m3 在 100–161.8% m1 → add :5/:s5/:c3
        if (FIB_1000..FIB_1618).contains(&m3_ratio) {
            add_or_promote(cands, StructureLabel::Five, Certainty::Primary);
            add_or_promote(cands, StructureLabel::S5, Certainty::Primary);
            add_or_promote(cands, StructureLabel::C3, Certainty::Primary);
            if let Some(m_minus_1) = ctx.m_minus_1 {
                if magnitude(m_minus_1) > magnitude(m1) {
                    drop_label(cands, StructureLabel::C3);
                }
            }
            if let Some(m4) = ctx.m4 {
                if magnitude(m4) < magnitude(m3) {
                    drop_label(cands, StructureLabel::Five);
                }
                let fast_full =
                    completely_retraced(&m3.monowave, &m4.monowave) && duration(m4) < duration(m3);
                if fast_full {
                    drop_label(cands, StructureLabel::S5);
                }
            }
        }

        // Branch 4:m3 < m1 AND m3 在自身時間內被完全回測 → add :5;
        //          m4 在 ≤ 50% m(-1)~m3 時間內回到 m(-1) 起點 AND m(-1) ≤ 261.8% m1 → add :c3
        if magnitude(m3) < magnitude(m1) {
            if let Some(m4) = ctx.m4 {
                if completely_retraced_within_time(m3, m4) {
                    add_or_promote(cands, StructureLabel::Five, Certainty::Primary);
                    if let Some(m_minus_1) = ctx.m_minus_1 {
                        let total_dur = duration(m_minus_1) + duration(m1) + duration(m3);
                        let m4_returns = retracement_pct(&m_minus_1.monowave, &m4.monowave) >= 1.0
                            && duration(m4) * 2 <= total_dur;
                        if m4_returns && mag_ratio(m_minus_1, m1) <= FIB_2618 {
                            add_or_promote(cands, StructureLabel::C3, Certainty::Primary);
                        }
                    }
                }
            }
        }

        // Branch 5:m3 < m1 AND m3 慢於自身時間被回測 → add :s5
        if magnitude(m3) < magnitude(m1) {
            if let Some(m4) = ctx.m4 {
                if completely_retraced(&m3.monowave, &m4.monowave)
                    && duration(m4) > duration(m3)
                {
                    add_or_promote(cands, StructureLabel::S5, Certainty::Primary);
                }
            }
        }

        // Branch 6:m3 < m1 AND m4 < m3 → add :s5/:F3
        if let Some(m4) = ctx.m4 {
            if magnitude(m3) < magnitude(m1) && magnitude(m4) < magnitude(m3) {
                add_or_promote(cands, StructureLabel::S5, Certainty::Primary);
                add_or_promote(cands, StructureLabel::F3, Certainty::Primary);
                if let Some(m5) = ctx.m5 {
                    if magnitude(m5) > magnitude(m3) {
                        drop_label(cands, StructureLabel::F3);
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Condition 3c — 61.8% ≤ m0 < 100% m1(6 branches)
// ---------------------------------------------------------------------------

fn cond_3c(ctx: &MonowaveContext, cands: &mut Vec<StructureLabelCandidate>) {
    let m1 = ctx.m1;
    if let Some(m3) = ctx.m3 {
        let m3_ratio = mag_ratio(m3, m1);

        // Branch 1:m3 > 261.8% m1 → add :c3/:sL3
        if m3_ratio > FIB_2618 {
            add_or_promote(cands, StructureLabel::C3, Certainty::Primary);
            add_or_promote(cands, StructureLabel::SL3, Certainty::Primary);
            if let Some(m_minus_1) = ctx.m_minus_1 {
                if mag_ratio(m_minus_1, m1) > FIB_1618 {
                    drop_label(cands, StructureLabel::SL3);
                }
                if let Some(m_minus_2) = ctx.m_minus_2 {
                    if mag_ratio(m_minus_1, m1) <= FIB_1618
                        && mag_ratio(m_minus_2, m_minus_1) >= FIB_618
                    {
                        drop_label(cands, StructureLabel::C3);
                    }
                }
            }
        }

        // Branch 2:m3 在 161.8–261.8% m1(含)→ add :F3/:c3/:sL3/:s5
        if (FIB_1618..=FIB_2618).contains(&m3_ratio) {
            add_or_promote(cands, StructureLabel::F3, Certainty::Primary);
            add_or_promote(cands, StructureLabel::C3, Certainty::Primary);
            add_or_promote(cands, StructureLabel::SL3, Certainty::Primary);
            add_or_promote(cands, StructureLabel::S5, Certainty::Primary);
            // m3 完全被回測快 → drop :s5
            if let Some(m4) = ctx.m4 {
                if completely_retraced(&m3.monowave, &m4.monowave) && duration(m4) < duration(m3) {
                    drop_label(cands, StructureLabel::S5);
                }
            }
            if let Some(m_minus_1) = ctx.m_minus_1 {
                if mag_ratio(m_minus_1, m1) > FIB_1618 {
                    drop_label(cands, StructureLabel::SL3);
                }
                if let Some(m_minus_2) = ctx.m_minus_2 {
                    if mag_ratio(m_minus_1, m1) <= FIB_1618 {
                        let m_neg_1_retraced = retracement_pct(
                            &m_minus_1.monowave,
                            &m1.monowave,
                        ) >= FIB_618;
                        if m_neg_1_retraced {
                            drop_label(cands, StructureLabel::C3);
                        }
                        let _ = m_minus_2;
                    }
                }
            }
            if let Some(m4) = ctx.m4 {
                if magnitude(m4) < magnitude(m3) {
                    drop_label(cands, StructureLabel::F3);
                }
            }
        }

        // Branch 3:m3 在 100–161.8% m1 → add :F3/:c3/:sL3/:s5
        if (FIB_1000..FIB_1618).contains(&m3_ratio) {
            add_or_promote(cands, StructureLabel::F3, Certainty::Primary);
            add_or_promote(cands, StructureLabel::C3, Certainty::Primary);
            add_or_promote(cands, StructureLabel::SL3, Certainty::Primary);
            add_or_promote(cands, StructureLabel::S5, Certainty::Primary);
            if let Some(m4) = ctx.m4 {
                if magnitude(m4) < magnitude(m3) {
                    drop_label(cands, StructureLabel::F3);
                }
                if completely_retraced(&m3.monowave, &m4.monowave) && duration(m4) < duration(m3) {
                    drop_label(cands, StructureLabel::S5);
                }
            }
            if let Some(m_minus_1) = ctx.m_minus_1 {
                if mag_ratio(m_minus_1, m1) > FIB_1618 {
                    drop_label(cands, StructureLabel::SL3);
                }
            }
        }

        // Branch 4:m3 < m1 AND m3 在自身時間內被完全回測 → add :c3/:F3
        if magnitude(m3) < magnitude(m1) {
            if let Some(m4) = ctx.m4 {
                if completely_retraced_within_time(m3, m4) {
                    add_or_promote(cands, StructureLabel::C3, Certainty::Primary);
                    add_or_promote(cands, StructureLabel::F3, Certainty::Primary);
                    if let Some(m_minus_1) = ctx.m_minus_1 {
                        let r = mag_ratio(m_minus_1, m1);
                        if !(FIB_1382..=FIB_2618).contains(&r) {
                            change_certainty(cands, StructureLabel::C3, Certainty::Rare);
                        }
                    }
                }
            }
        }

        // Branch 5:m3 < m1 AND m3 慢於自身時間被回測 → add :F3/(:s5)
        if magnitude(m3) < magnitude(m1) {
            if let Some(m4) = ctx.m4 {
                if completely_retraced(&m3.monowave, &m4.monowave) && duration(m4) > duration(m3) {
                    add_or_promote(cands, StructureLabel::F3, Certainty::Primary);
                    add_or_promote(cands, StructureLabel::S5, Certainty::Possible);
                    if let Some(m5) = ctx.m5 {
                        // m5 在自身時間內被 m4 完全回測 → drop (:s5)
                        // 注意:m5 是後續,m4 才能回測 m5;這裡 spec 寫的應是 m5 自身的後續
                        // 用 m4 vs m5 比代理
                        if completely_retraced_within_time(m5, m4) {
                            drop_label(cands, StructureLabel::S5);
                        }
                    }
                }
            }
        }

        // Branch 6:m3 < m1 AND m4 < m3 → add :s5/:c3/(:F3)
        if let Some(m4) = ctx.m4 {
            if magnitude(m3) < magnitude(m1) && magnitude(m4) < magnitude(m3) {
                add_or_promote(cands, StructureLabel::S5, Certainty::Primary);
                add_or_promote(cands, StructureLabel::C3, Certainty::Primary);
                add_or_promote(cands, StructureLabel::F3, Certainty::Possible);
                if let Some(m5) = ctx.m5 {
                    if magnitude(m5) > magnitude(m3) {
                        drop_label(cands, StructureLabel::F3);
                    }
                }
                if let Some(m_minus_1) = ctx.m_minus_1 {
                    if mag_ratio(m_minus_1, m1) > FIB_2618 {
                        drop_label(cands, StructureLabel::S5);
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Condition 3d — 100% ≤ m0 ≤ 161.8% m1(3 branches)
// ---------------------------------------------------------------------------

fn cond_3d(ctx: &MonowaveContext, cands: &mut Vec<StructureLabelCandidate>) {
    let m1 = ctx.m1;
    let Some(m0) = ctx.m0 else { return };
    if let (Some(m2), Some(m3)) = (ctx.m2, ctx.m3) {
        // 注意:Cond 3d 用 m3/m2 比(非 m3/m1)
        let m3_to_m2 = mag_ratio(m3, m2);

        // Branch 1:m3 > 261.8% m2 → add :5/:c3/(:sL3)
        if m3_to_m2 > FIB_2618 {
            add_or_promote(cands, StructureLabel::Five, Certainty::Primary);
            add_or_promote(cands, StructureLabel::C3, Certainty::Primary);
            add_or_promote(cands, StructureLabel::SL3, Certainty::Possible);
            if let Some(m_minus_1) = ctx.m_minus_1 {
                let r = mag_ratio(m_minus_1, m0);
                if !(FIB_618..=FIB_1618).contains(&r) {
                    drop_label(cands, StructureLabel::SL3);
                }
            }
            // m2 被回測慢於自身 → drop (:sL3) 與 :c3
            if let Some(m3_m4_check) = ctx.m3 {
                let m2_retrace_slow =
                    duration(m3_m4_check) > duration(m2)
                        && retracement_pct(&m2.monowave, &m3_m4_check.monowave) >= 1.0;
                if m2_retrace_slow {
                    drop_label(cands, StructureLabel::SL3);
                    drop_label(cands, StructureLabel::C3);
                }
            }
            if mag_ratio(m3, m1) > FIB_1618 {
                drop_label(cands, StructureLabel::Five);
            }
        }

        // Branch 2:m3 在 161.8–261.8% m2(含)→ add :c3/:sL3/:5
        if (FIB_1618..=FIB_2618).contains(&m3_to_m2) {
            add_or_promote(cands, StructureLabel::C3, Certainty::Primary);
            add_or_promote(cands, StructureLabel::SL3, Certainty::Primary);
            add_or_promote(cands, StructureLabel::Five, Certainty::Primary);
            if let Some(m_minus_1) = ctx.m_minus_1 {
                let r = mag_ratio(m_minus_1, m0);
                if !(FIB_618..=FIB_1618).contains(&r) {
                    // 細分:比 m1 與 m(-3)~m0
                    let m1_to_chain = mag_ratio(
                        m1,
                        ctx.m_minus_3
                            .or(ctx.m_minus_2)
                            .unwrap_or(m_minus_1),
                    );
                    if m1_to_chain < FIB_382 {
                        drop_label(cands, StructureLabel::SL3);
                    } else if (FIB_382..FIB_618).contains(&m1_to_chain) {
                        change_certainty(cands, StructureLabel::SL3, Certainty::Possible);
                    }
                }
                let r2 = mag_ratio(m_minus_1, m0);
                if (FIB_618..=FIB_1618).contains(&r2) {
                    drop_label(cands, StructureLabel::C3);
                }
            }
            if let Some(m4) = ctx.m4 {
                if mag_ratio(m4, m0) < FIB_618 {
                    change_certainty(cands, StructureLabel::Five, Certainty::Possible);
                }
            }
        }

        // Branch 3:m3 在 100–161.8% m2 → add :5/(:c3)/[:F3]
        if (FIB_1000..FIB_1618).contains(&m3_to_m2) {
            add_or_promote(cands, StructureLabel::Five, Certainty::Primary);
            add_or_promote(cands, StructureLabel::C3, Certainty::Possible);
            add_or_promote(cands, StructureLabel::F3, Certainty::Rare);
            if let Some(m4) = ctx.m4 {
                if magnitude(m4) > magnitude(m3) {
                    drop_label(cands, StructureLabel::C3);
                    drop_label(cands, StructureLabel::F3);
                }
                if let Some(m5) = ctx.m5 {
                    let m5_fast_steep =
                        more_vertical_and_longer(m5, m4) && magnitude(m5) >= magnitude(m1);
                    if magnitude(m4) < magnitude(m3) && m5_fast_steep {
                        drop_label(cands, StructureLabel::Five);
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Condition 3e — 161.8% ≤ m0 ≤ 261.8% m1(3 branches)
// ---------------------------------------------------------------------------

fn cond_3e(ctx: &MonowaveContext, cands: &mut Vec<StructureLabelCandidate>) {
    let m1 = ctx.m1;
    if let (Some(m2), Some(m3)) = (ctx.m2, ctx.m3) {
        let m3_to_m2 = mag_ratio(m3, m2);

        // Branch 1:m3 > 261.8% m2 → add :5/:c3/(:sL3)
        if m3_to_m2 > FIB_2618 {
            add_or_promote(cands, StructureLabel::Five, Certainty::Primary);
            add_or_promote(cands, StructureLabel::C3, Certainty::Primary);
            add_or_promote(cands, StructureLabel::SL3, Certainty::Possible);
            if mag_ratio(m3, m1) > FIB_1618 {
                drop_label(cands, StructureLabel::Five);
            }
        }

        // Branch 2:m3 在 161.8–261.8% m2(含)→ add :5/:c3 + missing x bundle 在 m0 中心
        if (FIB_1618..=FIB_2618).contains(&m3_to_m2) {
            add_or_promote(cands, StructureLabel::Five, Certainty::Primary);
            add_or_promote(cands, StructureLabel::C3, Certainty::Primary);
            // m0 中心 missing x:c3? 右 / :s5? 左 → bundle
            add_or_promote(cands, StructureLabel::XC3, Certainty::MissingWaveBundle);
            add_or_promote(cands, StructureLabel::S5, Certainty::MissingWaveBundle);
            if let Some(m4) = ctx.m4 {
                let m2_retrace_slow =
                    duration(m4) > duration(m2)
                        && retracement_pct(&m2.monowave, &m4.monowave) >= 1.0;
                if m2_retrace_slow {
                    drop_label(cands, StructureLabel::C3);
                }
            }
            if mag_ratio(m3, m1) > FIB_1618 {
                drop_label(cands, StructureLabel::Five);
            }
        }

        // Branch 3:m3 在 100–161.8% m2 → add :5/(:F3)
        if (FIB_1000..FIB_1618).contains(&m3_to_m2) {
            add_or_promote(cands, StructureLabel::Five, Certainty::Primary);
            add_or_promote(cands, StructureLabel::F3, Certainty::Possible);
            if let Some(m4) = ctx.m4 {
                if magnitude(m4) > magnitude(m3) {
                    drop_label(cands, StructureLabel::F3);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Condition 3f — m0 > 261.8% m1(3 branches)
// ---------------------------------------------------------------------------

fn cond_3f(ctx: &MonowaveContext, cands: &mut Vec<StructureLabelCandidate>) {
    let m1 = ctx.m1;
    if let (Some(m2), Some(m3)) = (ctx.m2, ctx.m3) {
        let m3_to_m2 = mag_ratio(m3, m2);

        // Branch 1:m3 > 261.8% m2 → add :5/(:c3) + missing x bundle 在 m0 中心
        if m3_to_m2 > FIB_2618 {
            add_or_promote(cands, StructureLabel::Five, Certainty::Primary);
            add_or_promote(cands, StructureLabel::C3, Certainty::Possible);
            if let Some(m4) = ctx.m4 {
                let m2_retrace_slow =
                    duration(m4) > duration(m2)
                        && retracement_pct(&m2.monowave, &m4.monowave) >= 1.0;
                if m2_retrace_slow {
                    drop_label(cands, StructureLabel::C3);
                }
            }
            if mag_ratio(m3, m1) > FIB_1618 {
                drop_label(cands, StructureLabel::Five);
            }
            // 若 (:c3) 採用 AND m(-1) 與 m1 無共享價區 → missing x bundle
            if let Some(m_minus_1) = ctx.m_minus_1 {
                if !share_price_range(&m_minus_1.monowave, &m1.monowave) {
                    add_or_promote(cands, StructureLabel::XC3, Certainty::MissingWaveBundle);
                    add_or_promote(cands, StructureLabel::S5, Certainty::MissingWaveBundle);
                }
            }
        }

        // Branch 2:m3 在 161.8–261.8% m2 → same as Branch 1,但 m3 > m2 → drop (:c3)
        if (FIB_1618..=FIB_2618).contains(&m3_to_m2) {
            add_or_promote(cands, StructureLabel::Five, Certainty::Primary);
            if magnitude(m3) > magnitude(m2) {
                drop_label(cands, StructureLabel::C3);
            } else {
                add_or_promote(cands, StructureLabel::C3, Certainty::Possible);
            }
        }

        // Branch 3:m3 在 100–161.8% m2 → add :5/(:F3)
        if (FIB_1000..FIB_1618).contains(&m3_to_m2) {
            add_or_promote(cands, StructureLabel::Five, Certainty::Primary);
            add_or_promote(cands, StructureLabel::F3, Certainty::Possible);
            if let Some(m4) = ctx.m4 {
                if magnitude(m4) > magnitude(m3) {
                    drop_label(cands, StructureLabel::F3);
                }
            }
        }
    }
    // 抑制未用警告
    let _ = MonowaveDirection::Up;
}
