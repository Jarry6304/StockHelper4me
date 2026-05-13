// rule_1.rs — Rule 1(m2 < 38.2% m1)所有 Condition 與 branch
//
// 對齊 m3Spec/neely_rules.md §Rule 1(531-573 行)。
// Headline:`{:5/(:c3)/(x:c3)/[:sL3]/[:s5]}`

use super::context::MonowaveContext;
use super::predicates::*;
use crate::output::{Certainty, StructureLabel, StructureLabelCandidate};

/// Rule 1 entry:依 m0/m1 比例分派 Condition 1a-1d。
pub fn run(ctx: &MonowaveContext, cands: &mut Vec<StructureLabelCandidate>) {
    // m0 不存在(i < 1)→ 整個 Rule 1 跳過(Pre-Constructive Logic 需 m(-1)/m0/m1/m2 context)
    let Some(m0) = ctx.m0 else { return };
    let ratio = mag_ratio(m0, ctx.m1);

    if ratio < FIB_618 {
        cond_1a(ctx, cands);
    } else if ratio < FIB_1000 {
        cond_1b(ctx, cands);
    } else if ratio <= FIB_1618 {
        cond_1c(ctx, cands);
    } else {
        cond_1d(ctx, cands);
    }
}

// ---------------------------------------------------------------------------
// Condition 1a — m0 < 61.8% m1(9 branches)
// ---------------------------------------------------------------------------

fn cond_1a(ctx: &MonowaveContext, cands: &mut Vec<StructureLabelCandidate>) {
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

    // Branch 2:m(-1) 在 100–161.8% m0 AND m0 ≈ 61.8% m1 AND m4 不超出 m0 終點 → add :s5
    if let (Some(m_minus_1), Some(m4)) = (ctx.m_minus_1, ctx.m4) {
        let ratio_m_neg_1_to_m0 = mag_ratio(m_minus_1, m0);
        let m0_to_m1 = mag_ratio(m0, m1);
        let m_neg_1_in_range = (FIB_1000..=FIB_1618).contains(&ratio_m_neg_1_to_m0);
        let m0_near_618 = fib_approx(m0_to_m1, FIB_618);
        // 「m4 不超出 m0 終點」= m4.end_price 與 m0.end_price 同側比較,m1 方向上 m4 未越過 m0.end
        let m4_within = match m1.monowave.direction {
            crate::output::MonowaveDirection::Up => m4.monowave.end_price <= m0.monowave.end_price,
            crate::output::MonowaveDirection::Down => {
                m4.monowave.end_price >= m0.monowave.end_price
            }
            _ => false,
        };
        if m_neg_1_in_range && m0_near_618 && m4_within {
            add_or_promote(cands, StructureLabel::S5, Certainty::Primary);
        }
    }

    // Branch 3:m0 含 > 3 monowaves AND m1 在 m0 同時間或更短完全回測 m0
    //   → 記:m0 可能是重要 Elliott 形態結束
    //   [缺資料] m0 是否為 polywave(含 > 3 monowaves)需 Stage 7 Compaction 後才知道。
    //   Phase 2 PR 將其視為 false(單一 monowave 不算「含 > 3 monowaves」)
    //   → 此 branch 無 candidate 變動,只可能加 diagnostic flag(P6 Compaction 串接後完整)

    // Branch 4:m0 ≈ m2(價/時 相等或 61.8%)AND m(-1) ≥ 161.8% m1
    //          AND m3(或 m3~m5)在 ≤ m(-1) 時間內 ≥ m(-1) 價長
    //   → add [:c3](Running Correction);m(-2) < m(-1) → x-wave 在 m0,m1 改為 x:c3
    if let (Some(m_minus_1), Some(m2), Some(m3)) = (ctx.m_minus_1, ctx.m2, ctx.m3) {
        let m0_m2_similar = approx_equal(magnitude(m0), magnitude(m2))
            || approx_equal(magnitude(m0), magnitude(m2) * FIB_618)
            || approx_equal(duration(m0) as f64, duration(m2) as f64);
        let m_neg_1_long = mag_ratio(m_minus_1, m1) >= FIB_1618;
        // m3 (or m3..m5) covers ≥ m(-1) price length in ≤ m(-1) time
        let combined_mag = magnitude(m3)
            + ctx.m4.map_or(0.0, magnitude)
            + ctx.m5.map_or(0.0, magnitude);
        let combined_dur = duration(m3)
            + ctx.m4.map_or(0, duration)
            + ctx.m5.map_or(0, duration);
        let combined_covers = combined_mag >= magnitude(m_minus_1)
            && combined_dur <= duration(m_minus_1);

        if m0_m2_similar && m_neg_1_long && combined_covers {
            add_or_promote(cands, StructureLabel::C3, Certainty::Rare);
            // m(-2) < m(-1) → x-wave 在 m0,m1 改為 x:c3
            if ctx.m_minus_2.is_some_and(|m_neg_2| magnitude(m_neg_2) < magnitude(m_minus_1)) {
                add_or_promote(cands, StructureLabel::XC3, Certainty::Possible);
            }
        }
    }

    // Branch 5:m0 ≈ m2 AND m(-1) < 161.8% m1 AND m(-1) > m0 AND m3~m5 ≥ 161.8% m1
    //   → add :c3;若 m(-2) > m(-1) 在 m(-1) 加 :sL3
    if let (Some(m_minus_1), Some(m2), Some(m3)) = (ctx.m_minus_1, ctx.m2, ctx.m3) {
        let m0_m2_similar = approx_equal(magnitude(m0), magnitude(m2));
        let m_neg_1_under_1618 = mag_ratio(m_minus_1, m1) < FIB_1618;
        let m_neg_1_gt_m0 = magnitude(m_minus_1) > magnitude(m0);
        let m3_m5_long = magnitude(m3)
            + ctx.m4.map_or(0.0, magnitude)
            + ctx.m5.map_or(0.0, magnitude)
            >= FIB_1618 * magnitude(m1);
        if m0_m2_similar && m_neg_1_under_1618 && m_neg_1_gt_m0 && m3_m5_long {
            add_or_promote(cands, StructureLabel::C3, Certainty::Primary);
            // Note:「在 m(-1) 加 :sL3」是修改 m(-1) 的 candidate list,不是 m1 的;
            // Phase 2 不回頭改 m(-1)(對齊「順序遍歷,後監測不改前監測」原則)
            // 留 TODO comment,P5/P6 加入 backward 更新時補
        }
    }

    // Branch 6:m0 ≈ m2 AND m3 < 161.8% m1 AND m3 在自身時間內(或更短)被完全回測
    //   → add x:c3?(missing-wave bundle:x-wave 三種可能位置 — m0/m2/m1 中心)
    if let (Some(m2), Some(m3), Some(m4)) = (ctx.m2, ctx.m3, ctx.m4) {
        let m0_m2_similar = approx_equal(magnitude(m0), magnitude(m2));
        let m3_under_1618 = mag_ratio(m3, m1) < FIB_1618;
        let m3_retraced = completely_retraced_within_time(m3, m4);
        if m0_m2_similar && m3_under_1618 && m3_retraced {
            // missing-wave 標記束帶(三處可能位置,只在 m1 上加一個 XC3 bundle 標記)
            add_or_promote(cands, StructureLabel::XC3, Certainty::MissingWaveBundle);
        }
    }

    // Branch 7:m(-2) > m(-1) → drop「x-wave 在 m0」可能性
    //   實作詮釋:drop XC3 from m1 (XC3 marker 已 push 在 m1 上,branch 7 取消)
    if let (Some(m_minus_2), Some(m_minus_1)) = (ctx.m_minus_2, ctx.m_minus_1) {
        if magnitude(m_minus_2) > magnitude(m_minus_1) {
            drop_label(cands, StructureLabel::XC3);
        }
    }

    // Branch 8:m3 < 61.8% m1 → 大幅提高「x-wave 隱藏在 m1 中心」機率
    //   實作:若 candidates 已含 XC3 missing-wave bundle,升級 certainty
    if let Some(m3) = ctx.m3 {
        if mag_ratio(m3, m1) < FIB_618 {
            // 將 XC3 從 MissingWaveBundle 升 Possible(missing x-wave 機率高)
            change_certainty(cands, StructureLabel::XC3, Certainty::Possible);
        }
    }

    // Branch 9:m(-1) > m0 AND m0 < m1 AND m1 不為 m(-1)/m1/m3 中最短 AND m3 在自身時間內被完全回測
    //   → add :c3(Terminal wave-3 場景)
    if let (Some(m_minus_1), Some(m3), Some(m4)) = (ctx.m_minus_1, ctx.m3, ctx.m4) {
        let cond_a = magnitude(m_minus_1) > magnitude(m0);
        let cond_b = magnitude(m0) < magnitude(m1);
        let cond_c = !is_shortest_of_three(m1, Some(m_minus_1), Some(m3));
        let cond_d = completely_retraced_within_time(m3, m4);
        if cond_a && cond_b && cond_c && cond_d {
            add_or_promote(cands, StructureLabel::C3, Certainty::Primary);
        }
    }
}

// ---------------------------------------------------------------------------
// Condition 1b — 61.8% ≤ m0 < 100% m1(6 branches)
// ---------------------------------------------------------------------------

fn cond_1b(ctx: &MonowaveContext, cands: &mut Vec<StructureLabelCandidate>) {
    let m1 = ctx.m1;
    let Some(m0) = ctx.m0 else { return };

    // Branch 1:(無前置條件) → add :5
    add_or_promote(cands, StructureLabel::Five, Certainty::Primary);

    // Branch 2:m(-1) 在 100–161.8% m0(含)AND m4 不超出 m0 終點
    //   → add :s5;m2 終點加 x:c3?(Flat with x-wave)
    if let (Some(m_minus_1), Some(m4)) = (ctx.m_minus_1, ctx.m4) {
        let ratio_to_m0 = mag_ratio(m_minus_1, m0);
        let in_range = (FIB_1000..=FIB_1618).contains(&ratio_to_m0);
        let m4_within = match m1.monowave.direction {
            crate::output::MonowaveDirection::Up => m4.monowave.end_price <= m0.monowave.end_price,
            crate::output::MonowaveDirection::Down => {
                m4.monowave.end_price >= m0.monowave.end_price
            }
            _ => false,
        };
        if in_range && m4_within {
            add_or_promote(cands, StructureLabel::S5, Certainty::Primary);
            // m2 終點加 x:c3? — bundle 標記在 m1 上(語意:m2 為 x-wave)
            add_or_promote(cands, StructureLabel::XC3, Certainty::MissingWaveBundle);
        }
    }

    // Branch 3:m0 polywave + m1 完全回測 m0(plus 1 time unit 嚴格時計)
    //   → 記:m0 可能 Elliott 形態結束(本 PR 視為 false placeholder)
    //   [缺資料]:m0 polywave 偵測需 Compaction(P6+)

    // Branch 4:m2 部分價區與 m0 共享 AND m3 比 m1 更長更垂直(時間 ≤ m1)AND m(-1) > m1
    //   → add :sL3(Triangle 倒二)
    if let (Some(m_minus_1), Some(m2), Some(m3)) = (ctx.m_minus_1, ctx.m2, ctx.m3) {
        let share = share_price_range(&m2.monowave, &m0.monowave);
        let m3_steeper = more_vertical_and_longer(m3, m1) && duration(m3) <= duration(m1);
        let m_neg_1_gt_m1 = magnitude(m_minus_1) > magnitude(m1);
        if share && m3_steeper && m_neg_1_gt_m1 {
            add_or_promote(cands, StructureLabel::SL3, Certainty::Primary);
        }
    }

    // Branch 5:m2 部分價區與 m0 共享 AND m3 比 m1 更長更垂直(時間 ≤ m1)AND m(-1) < m1
    //          AND m0 與 m2 在價或時上明顯不同
    //          AND m4(或 m4~m6)在 ≤ 50% m1~m3 時間內回到 m1 起點
    //   → add :c3(5th Ext Terminal 完成於 m3)
    if let (Some(m_minus_1), Some(m2), Some(m3), Some(m4)) =
        (ctx.m_minus_1, ctx.m2, ctx.m3, ctx.m4)
    {
        let share = share_price_range(&m2.monowave, &m0.monowave);
        let m3_steeper = more_vertical_and_longer(m3, m1) && duration(m3) <= duration(m1);
        let m_neg_1_lt_m1 = magnitude(m_minus_1) < magnitude(m1);
        let m0_m2_different =
            !approx_equal(magnitude(m0), magnitude(m2)) || !approx_equal(duration(m0) as f64, duration(m2) as f64);
        // m4 retraced to m1 start in ≤ 50% (m1 + m3 dur)
        let m1_to_m3_dur = duration(m1) + duration(m3);
        let m4_retraces_to_m1_start = retracement_pct(
            &crate::output::Monowave {
                start_date: m1.monowave.start_date,
                end_date: m3.monowave.end_date,
                start_price: m1.monowave.start_price,
                end_price: m3.monowave.end_price,
                direction: m1.monowave.direction,
            },
            &m4.monowave,
        ) >= 1.0
            && duration(m4) * 2 <= m1_to_m3_dur;

        if share && m3_steeper && m_neg_1_lt_m1 && m0_m2_different && m4_retraces_to_m1_start {
            add_or_promote(cands, StructureLabel::C3, Certainty::Primary);
        }
    }

    // Branch 6:m3 < m1 AND m2 部分價區與 m0 共享 AND m(-1) > m0 且 m(-1) > m1
    //          AND m1 不為 m(-1)/m1/m3 中最短
    //          AND 自 m3 終點起,市場在 ≤ 50% m(-1)~m3 時間內回到 m1 起點
    //   → add :c3
    if let (Some(m_minus_1), Some(m2), Some(m3), Some(m4)) =
        (ctx.m_minus_1, ctx.m2, ctx.m3, ctx.m4)
    {
        let m3_lt_m1 = magnitude(m3) < magnitude(m1);
        let share = share_price_range(&m2.monowave, &m0.monowave);
        let m_neg_1_dominant =
            magnitude(m_minus_1) > magnitude(m0) && magnitude(m_minus_1) > magnitude(m1);
        let m1_not_shortest = !is_shortest_of_three(m1, Some(m_minus_1), Some(m3));
        let m_neg_1_to_m3_dur = duration(m_minus_1) + duration(m0) + duration(m1) + duration(m3);
        let m4_to_m1_start =
            retracement_pct(&m_minus_1.monowave, &m4.monowave) >= 1.0 && duration(m4) * 2 <= m_neg_1_to_m3_dur;
        if m3_lt_m1 && share && m_neg_1_dominant && m1_not_shortest && m4_to_m1_start {
            add_or_promote(cands, StructureLabel::C3, Certainty::Primary);
        }
    }
}

// ---------------------------------------------------------------------------
// Condition 1c — 100% ≤ m0 ≤ 161.8% m1(4 branches)
// ---------------------------------------------------------------------------

fn cond_1c(ctx: &MonowaveContext, cands: &mut Vec<StructureLabelCandidate>) {
    let m1 = ctx.m1;
    let Some(m0) = ctx.m0 else { return };

    // Branch 1:(無前置條件) → add :5
    add_or_promote(cands, StructureLabel::Five, Certainty::Primary);

    // Branch 2:m0 ≈ m1(±10%) AND m0 與 m1 價時相等或 61.8% AND m3 比 m1 更長更垂直
    //          AND m2 耗時 ≥ m0 或 m1 AND m2 ≈ 38.2% m1 AND m0 Structure 含 :F3
    //   → add [:c3](嚴格條件,m2 在重要 Fib 位收)
    if let (Some(m2), Some(m3)) = (ctx.m2, ctx.m3) {
        let m0_m1_close = approx_equal(magnitude(m0), magnitude(m1));
        let time_or_618 = approx_equal(duration(m0) as f64, duration(m1) as f64)
            || approx_equal(duration(m0) as f64, duration(m1) as f64 * FIB_618);
        let m3_steeper = more_vertical_and_longer(m3, m1);
        let m2_long = duration(m2) >= duration(m0) || duration(m2) >= duration(m1);
        let m2_near_382 = fib_approx(mag_ratio(m2, m1), FIB_382);
        let m0_has_f3 = structure_includes(m0, StructureLabel::F3);
        if m0_m1_close && time_or_618 && m3_steeper && m2_long && m2_near_382 && m0_has_f3 {
            add_or_promote(cands, StructureLabel::C3, Certainty::Rare);
        }
    }

    // Branch 3:m0 與 m2 價時相等(或 61.8%)AND m3 < 161.8% m1
    //          AND m3(plus one time unit)在自身時間內(或更短)被完全回測
    //   → m1 可能 Complex Correction with x-wave;multi-place markers
    //   實作:加 XC3 missing-wave bundle + S5 missing-wave bundle
    if let (Some(m2), Some(m3), Some(m4)) = (ctx.m2, ctx.m3, ctx.m4) {
        let m0_m2_similar = approx_equal(magnitude(m0), magnitude(m2))
            || approx_equal(magnitude(m0), magnitude(m2) * FIB_618);
        let m3_under_1618 = mag_ratio(m3, m1) < FIB_1618;
        let m3_retraced = completely_retraced_plus_one_time_unit(m3, m4);
        if m0_m2_similar && m3_under_1618 && m3_retraced {
            add_or_promote(cands, StructureLabel::XC3, Certainty::MissingWaveBundle);
            add_or_promote(cands, StructureLabel::S5, Certainty::MissingWaveBundle);

            // 子規則:若 m(-2) > m(-1) → x-wave 不在 m0 終點 → drop XC3 missing 標記
            if let (Some(m_minus_2), Some(m_minus_1)) = (ctx.m_minus_2, ctx.m_minus_1) {
                if magnitude(m_minus_2) > magnitude(m_minus_1) {
                    drop_label(cands, StructureLabel::XC3);
                }
            }
            // 子規則:若 m3 < 61.8% m1 → x-wave 大機率在 m1 中心(已是 MissingWaveBundle)
            if mag_ratio(m3, m1) < FIB_618 {
                change_certainty(cands, StructureLabel::S5, Certainty::Possible);
            }
        }
    }

    // Branch 4:m3 比 m1 更長更垂直 AND (m3 完全被回測 OR m3 被回測 ≤ 61.8%)
    //          AND m2 ≈ 38.2% m1 AND m0 Structure 含 :c3 AND m(-3) > m(-2)
    //          AND m(-2) 或 m(-1) > m0
    //   → add (:sL3)(Contracting Triangle 倒二)
    if let (Some(m_minus_3), Some(m_minus_2), Some(m_minus_1), Some(m2), Some(m3), Some(m4)) = (
        ctx.m_minus_3,
        ctx.m_minus_2,
        ctx.m_minus_1,
        ctx.m2,
        ctx.m3,
        ctx.m4,
    ) {
        let m3_steeper = more_vertical_and_longer(m3, m1);
        let m3_retraced = completely_retraced(&m3.monowave, &m4.monowave)
            || retracement_pct(&m3.monowave, &m4.monowave) <= FIB_618;
        let m2_near_382 = fib_approx(mag_ratio(m2, m1), FIB_382);
        let m0_has_c3 = structure_includes(m0, StructureLabel::C3);
        let m_neg_3_gt_m_neg_2 = magnitude(m_minus_3) > magnitude(m_minus_2);
        let m_neg_2_or_neg_1_gt_m0 =
            magnitude(m_minus_2) > magnitude(m0) || magnitude(m_minus_1) > magnitude(m0);
        if m3_steeper && m3_retraced && m2_near_382 && m0_has_c3 && m_neg_3_gt_m_neg_2 && m_neg_2_or_neg_1_gt_m0 {
            add_or_promote(cands, StructureLabel::SL3, Certainty::Possible);
        }
    }
}

// ---------------------------------------------------------------------------
// Condition 1d — m0 > 161.8% m1(1 branch)
// ---------------------------------------------------------------------------

fn cond_1d(_ctx: &MonowaveContext, cands: &mut Vec<StructureLabelCandidate>) {
    // Branch 1:任何情境 → 僅 :5(唯一可能);其他先清除
    cands.clear();
    add_or_promote(cands, StructureLabel::Five, Certainty::Primary);
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
    fn cond_1d_emits_only_five() {
        // m0 / m1 = 2.0 (> 161.8%);m2 / m1 = 0.2 (< 38.2%)
        // build minimal context with m_minus_1, m0, m1, m2
        let classified = vec![
            cmw(100.0, 110.0, MonowaveDirection::Up, 5),     // m_minus_1
            cmw(110.0, 90.0, MonowaveDirection::Down, 5),    // m0 (mag 20)
            cmw(90.0, 100.0, MonowaveDirection::Up, 5),      // m1 (mag 10, m0/m1 = 2.0 > 1.618)
            cmw(100.0, 99.0, MonowaveDirection::Down, 2),    // m2 (mag 1, m2/m1 = 0.1 < 0.382)
        ];
        let ctx = MonowaveContext::build(&classified, 2).expect("build ctx");
        let mut cands = Vec::new();
        run(&ctx, &mut cands);
        assert_eq!(cands.len(), 1);
        assert!(matches!(cands[0].label, StructureLabel::Five));
        assert!(matches!(cands[0].certainty, Certainty::Primary));
    }

    #[test]
    fn cond_1a_branch_1_emits_five_when_m2_dur_meets_threshold() {
        // m0 = 5 (mag, < 0.618 * m1 = 10) → cond 1a
        // m2 dur = 6 ≥ m1 dur = 5 → branch 1 fires
        let classified = vec![
            cmw(100.0, 100.0, MonowaveDirection::Up, 1),   // m_minus_1 placeholder
            cmw(100.0, 95.0, MonowaveDirection::Down, 5),  // m0 (mag 5, m0/m1 = 0.5 < 0.618)
            cmw(95.0, 105.0, MonowaveDirection::Up, 5),    // m1 (mag 10)
            cmw(105.0, 103.0, MonowaveDirection::Down, 6), // m2 (mag 2 < 0.382 * 10, dur 6 ≥ 5)
        ];
        let ctx = MonowaveContext::build(&classified, 2).expect("build ctx");
        let mut cands = Vec::new();
        run(&ctx, &mut cands);
        assert!(cands.iter().any(|c| matches!(c.label, StructureLabel::Five)));
    }
}
